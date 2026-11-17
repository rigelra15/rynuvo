//! Keeps this app's playback in time with the server's music state for the voice channel I'm in:
//! fetches each song (and the next one in the queue) from YouTube Music, and every tick tells the
//! voice engine where the song should be now.
use std::{
    collections::HashMap,
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
};

use rynuvo_proto::{Id, MusicState, WsServerMsg};
use serde::Serialize;

use super::webm::Song;

/// How often playback is checked against the server's clock.
pub const TICK_MS: u64 = 250;
/// Songs kept decoded-ready (≈3 MB each).
const CACHE: usize = 3;

pub type Loader = Arc<dyn Fn(String) -> Pin<Box<dyn Future<Output = Result<Arc<Song>, String>> + Send>> + Send + Sync>;

/// Where the controller sends playback: the voice engine.
pub trait Output: Send + Sync {
    fn set(&self, entry: u64, song: Arc<Song>, at_ms: u32, paused: bool);
    fn clear(&self);
    /// The voice channel I'm in.
    fn channel(&self) -> Option<Id>;
}

/// This app's progress with a song, for the UI.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MusicStatus {
    pub video_id: String,
    /// "loading", "ready" or "error".
    pub status: &'static str,
    pub message: Option<String>,
}

pub type StatusSink = Arc<dyn Fn(MusicStatus) + Send + Sync>;

/// Where the song is now (ms), from a state received when the local clock read `received_at`.
pub fn position(s: &MusicState, received_at: i64, now: i64) -> u32 {
    let dur = s.now.as_ref().map_or(0, |e| e.duration_ms);
    let at = match s.paused_at {
        Some(p) => p as i64,
        None => now - received_at + s.server_time - s.started_at,
    };
    at.clamp(0, dur as i64) as u32
}

/// The song is scheduled to start later (the server gives everyone time to load it): hold at 0:00.
pub fn before_start(s: &MusicState, received_at: i64, now: i64) -> bool {
    s.paused_at.is_none() && now - received_at + s.server_time < s.started_at
}

#[derive(Default)]
struct St {
    /// Per voice channel: the state and the local time it arrived.
    states: HashMap<Id, (MusicState, i64)>,
    songs: Vec<(String, Arc<Song>)>,
    loading: Vec<String>,
    failed: HashMap<String, String>,
    playing: Option<u64>,
}

#[derive(Clone)]
pub struct MusicCtl {
    st: Arc<Mutex<St>>,
    out: Arc<dyn Output>,
    load: Loader,
    status: StatusSink,
}

impl MusicCtl {
    pub fn new(out: Arc<dyn Output>, load: Loader, status: StatusSink) -> Self {
        MusicCtl { st: Arc::default(), out, load, status }
    }

    pub fn on_server(&self, m: &WsServerMsg, now: i64) {
        let mut st = self.st.lock().unwrap();
        match m {
            WsServerMsg::Ready { music, .. } => {
                st.states = music.iter().map(|s| (s.channel_id, (s.clone(), now))).collect();
            }
            WsServerMsg::MusicState(s) if s.now.is_none() && s.queue.is_empty() => {
                st.states.remove(&s.channel_id);
            }
            WsServerMsg::MusicState(s) => {
                st.states.insert(s.channel_id, (s.clone(), now));
            }
            _ => return,
        }
        drop(st);
        self.tick(now);
    }

    /// Bring playback in line with the state of my voice channel.
    pub fn tick(&self, now: i64) {
        let ch = self.out.channel();
        let mut st = self.st.lock().unwrap();
        let state = ch.and_then(|c| st.states.get(&c)).cloned();
        let Some((s, at)) = state.filter(|(s, _)| s.now.is_some()) else {
            if st.playing.take().is_some() {
                drop(st);
                self.out.clear();
            }
            return;
        };
        let entry = s.now.as_ref().unwrap();
        let mut wanted = vec![entry.video_id.clone()];
        wanted.extend(s.queue.first().map(|e| e.video_id.clone()));
        for v in &wanted {
            self.fetch(&mut st, v);
        }
        match st.songs.iter().find(|(v, _)| *v == entry.video_id).map(|(_, s)| s.clone()) {
            Some(song) => {
                st.playing = Some(entry.id);
                drop(st);
                let hold = s.paused_at.is_some() || before_start(&s, at, now);
                self.out.set(entry.id, song, position(&s, at, now), hold);
            }
            None => {
                if st.playing.take().is_some() {
                    drop(st);
                    self.out.clear();
                }
            }
        }
    }

    fn fetch(&self, st: &mut St, video_id: &str) {
        if st.songs.iter().any(|(v, _)| v == video_id) || st.loading.iter().any(|v| v == video_id) || st.failed.contains_key(video_id) {
            return;
        }
        st.loading.push(video_id.to_string());
        (self.status)(MusicStatus { video_id: video_id.to_string(), status: "loading", message: None });
        let me = self.clone();
        let v = video_id.to_string();
        let fut = (self.load)(v.clone());
        tokio::spawn(async move {
            let r = fut.await;
            let status = {
                let mut st = me.st.lock().unwrap();
                st.loading.retain(|x| *x != v);
                match r {
                    Ok(song) => {
                        st.songs.push((v.clone(), song));
                        let extra = st.songs.len().saturating_sub(CACHE);
                        st.songs.drain(..extra);
                        MusicStatus { video_id: v, status: "ready", message: None }
                    }
                    Err(e) => {
                        tracing::warn!("music {v}: {e}");
                        st.failed.insert(v.clone(), e.clone());
                        MusicStatus { video_id: v, status: "error", message: Some(e) }
                    }
                }
            };
            (me.status)(status);
            me.tick(crate::music::now_ms());
        });
    }

    /// What this app knows about a song: loading, ready, or why it failed.
    pub fn status_of(&self, video_id: &str) -> Option<MusicStatus> {
        let st = self.st.lock().unwrap();
        let v = video_id.to_string();
        if st.songs.iter().any(|(x, _)| *x == v) {
            Some(MusicStatus { video_id: v, status: "ready", message: None })
        } else if let Some(e) = st.failed.get(&v) {
            Some(MusicStatus { video_id: v, status: "error", message: Some(e.clone()) })
        } else if st.loading.contains(&v) {
            Some(MusicStatus { video_id: v, status: "loading", message: None })
        } else {
            None
        }
    }

    /// Run the tick until the app exits.
    pub fn spawn_ticker(&self) {
        let me = self.clone();
        tokio::spawn(async move {
            let mut iv = tokio::time::interval(std::time::Duration::from_millis(TICK_MS));
            loop {
                iv.tick().await;
                me.tick(crate::music::now_ms());
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rynuvo_proto::MusicEntry;

    fn entry(id: u64, v: &str) -> MusicEntry {
        MusicEntry { id, video_id: v.into(), title: v.into(), artist: "LANY".into(), album: None, art_url: String::new(), duration_ms: 180_000, requested_by: 1, video: false }
    }

    fn state(now: Option<MusicEntry>, queue: Vec<MusicEntry>, started_at: i64, paused_at: Option<u32>, server_time: i64) -> MusicState {
        MusicState { channel_id: 5, now, started_at, paused_at, queue, server_time, lyrics: "auto".into(), repeat: "off".into(), skip_votes: vec![], skip_needed: 0 }
    }

    #[test]
    fn position_follows_the_server_clock() {
        // Server says: started at 10_000, its clock reads 12_000; my clock read 500 then.
        let s = state(Some(entry(1, "a")), vec![], 10_000, None, 12_000);
        assert_eq!(position(&s, 500, 500), 2_000);
        assert_eq!(position(&s, 500, 1_500), 3_000, "one second later here");
        assert_eq!(position(&s, 500, 10_000_000), 180_000, "never past the end");
        let p = state(Some(entry(1, "a")), vec![], 10_000, Some(4_321), 99_999);
        assert_eq!(position(&p, 0, 77_777), 4_321, "paused stays put");
        let early = state(Some(entry(1, "a")), vec![], 10_000, None, 9_000);
        assert_eq!(position(&early, 0, 0), 0);
        assert!(before_start(&early, 0, 999), "held until the scheduled start");
        assert!(!before_start(&early, 0, 1_000));
        assert!(!before_start(&s, 500, 500));
    }

    #[derive(Default)]
    struct Fake {
        ch: Mutex<Option<Id>>,
        log: Mutex<Vec<String>>,
    }
    impl Output for Fake {
        fn set(&self, entry: u64, _song: Arc<Song>, at_ms: u32, paused: bool) {
            self.log.lock().unwrap().push(format!("set {entry} {at_ms} {paused}"));
        }
        fn clear(&self) {
            self.log.lock().unwrap().push("clear".into());
        }
        fn channel(&self) -> Option<Id> {
            *self.ch.lock().unwrap()
        }
    }

    fn ctl(fake: Arc<Fake>, loads: Arc<Mutex<Vec<String>>>) -> MusicCtl {
        let load: Loader = Arc::new(move |v: String| {
            loads.lock().unwrap().push(v.clone());
            Box::pin(async move {
                if v == "bad" {
                    return Err("That song can't be played".into());
                }
                crate::music::webm::demux(include_bytes!("../../tests/fixtures/tone.webm"))
            })
        });
        MusicCtl::new(fake, load, Arc::new(|_| {}))
    }

    #[tokio::test]
    async fn plays_my_channels_song_in_time_and_prefetches_the_next() {
        let fake = Arc::new(Fake::default());
        let loads = Arc::new(Mutex::new(Vec::new()));
        let c = ctl(fake.clone(), loads.clone());
        *fake.ch.lock().unwrap() = Some(5);
        let s = state(Some(entry(1, "a")), vec![entry(2, "b")], 10_000, None, 12_000);
        c.on_server(&WsServerMsg::MusicState(s), 100);
        tokio::task::yield_now().await;
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        assert_eq!(*loads.lock().unwrap(), vec!["a", "b"], "the next song is fetched ahead");
        fake.log.lock().unwrap().clear();
        c.tick(1_100);
        assert_eq!(*fake.log.lock().unwrap(), vec!["set 1 3000 false"]);
        // Leaving the voice channel stops it here.
        *fake.ch.lock().unwrap() = None;
        c.tick(1_200);
        c.tick(1_300);
        assert_eq!(fake.log.lock().unwrap().last().unwrap(), "clear");
        assert_eq!(fake.log.lock().unwrap().iter().filter(|l| *l == "clear").count(), 1, "cleared once");
    }

    #[tokio::test]
    async fn another_channels_music_is_not_played_and_stop_clears() {
        let fake = Arc::new(Fake::default());
        let loads = Arc::new(Mutex::new(Vec::new()));
        let c = ctl(fake.clone(), loads.clone());
        *fake.ch.lock().unwrap() = Some(9);
        c.on_server(&WsServerMsg::MusicState(state(Some(entry(1, "a")), vec![], 0, None, 0)), 0);
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        assert!(loads.lock().unwrap().is_empty(), "nothing fetched for a channel I'm not in");
        *fake.ch.lock().unwrap() = Some(5);
        c.tick(10);
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        c.tick(20);
        assert!(fake.log.lock().unwrap().last().unwrap().starts_with("set 1"));
        c.on_server(&WsServerMsg::MusicState(state(None, vec![], 0, None, 0)), 30);
        assert_eq!(fake.log.lock().unwrap().last().unwrap(), "clear");
    }

    #[tokio::test]
    async fn a_song_that_fails_is_not_retried_every_tick() {
        let fake = Arc::new(Fake::default());
        let loads = Arc::new(Mutex::new(Vec::new()));
        let c = ctl(fake.clone(), loads.clone());
        *fake.ch.lock().unwrap() = Some(5);
        c.on_server(&WsServerMsg::MusicState(state(Some(entry(1, "bad")), vec![], 0, None, 0)), 0);
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        for t in 1..5 {
            c.tick(t * 250);
        }
        assert_eq!(loads.lock().unwrap().len(), 1);
        assert_eq!(c.status_of("bad").unwrap().status, "error");
        assert!(fake.log.lock().unwrap().iter().all(|l| !l.starts_with("set")));
    }
}
