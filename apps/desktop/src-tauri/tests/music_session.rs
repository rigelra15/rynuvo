//! Listen together end to end: a music command over a real session reaches the server, its state
//! comes back, the controller loads the song and the voice engine plays it in time.
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use ringbuf::traits::Consumer;
use rynuvo_desktop_lib::{
    core::{
        http::Api,
        identity::MemoryStore,
        session::{Session, SessionEvent},
    },
    music::{
        ctl::{Loader, MusicCtl, Output},
        webm::Song,
    },
    voice::{
        backend::{AudioBackend, AudioIo},
        ctl::VoiceCtl,
    },
};
use rynuvo_proto::{MusicCmd, MusicEntryReq, WsClientMsg};
use rynuvo_server::testutil::TestApp;

struct FakeBackend(Arc<Mutex<Option<AudioIo>>>);

impl AudioBackend for FakeBackend {
    fn start(&mut self, _i: Option<&str>, _o: Option<&str>, io: AudioIo) -> Result<(), String> {
        *self.0.lock().unwrap() = Some(io);
        Ok(())
    }
    fn stop(&mut self) {
        self.0.lock().unwrap().take();
    }
}

/// Plays through the real voice engine and remembers what it was told.
struct Out(VoiceCtl, Arc<Mutex<Vec<(u64, u32, bool)>>>);

impl Output for Out {
    fn set(&self, entry: u64, song: Arc<Song>, at_ms: u32, paused: bool) {
        self.1.lock().unwrap().push((entry, at_ms, paused));
        self.0.music_set(entry, song, at_ms, paused);
    }
    fn clear(&self) {
        self.0.music_clear();
    }
    fn channel(&self) -> Option<i64> {
        self.0.channel()
    }
}

async fn eventually(what: &str, f: impl Fn() -> bool) {
    for _ in 0..100 {
        if f() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("timed out waiting for: {what}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_queued_song_plays_through_the_voice_engine_in_time() {
    let app = TestApp::spawn().await;
    let u = app.user("Dimas").await;
    let text = app.create_channel(&u.token, "text", "general").await.id;
    let voice = app.create_channel(&u.token, "voice", "Mabar").await.id;
    let base = format!("http://{}", app.addr);

    let io = Arc::new(Mutex::new(None));
    let io2 = io.clone();
    let ctl = VoiceCtl::new(Arc::new(move || Box::new(FakeBackend(io2.clone())) as Box<dyn AudioBackend>), Arc::new(|_| {}));
    let sets = Arc::new(Mutex::new(Vec::new()));
    let load: Loader = Arc::new(|_v: String| {
        Box::pin(async { rynuvo_desktop_lib::music::webm::demux(include_bytes!("fixtures/tone.webm")) })
    });
    let music = MusicCtl::new(Arc::new(Out(ctl.clone(), sets.clone())), load, Arc::new(|_| {}));
    let (c2, m2) = (ctl.clone(), music.clone());
    let emit = Arc::new(move |e: SessionEvent| {
        c2.on_session(&e);
        if let SessionEvent::Server(m) = &e {
            m2.on_server(m, rynuvo_desktop_lib::music::now_ms());
        }
    });
    let s = Session::start(Api::new(&base, Some(u.token.clone())).unwrap(), Arc::new(MemoryStore::default()), base, emit);
    ctl.attach(s, "127.0.0.1".into());
    let uid = u.user.id;
    eventually("online", || app.state.hub.online_ids().contains(&uid)).await;
    ctl.join(voice);
    eventually("engine started", || io.lock().unwrap().is_some()).await;
    let heard = || async {
        let mut e = 0.0f32;
        for _ in 0..30 {
            let mut buf = [0f32; 960];
            if let Some(io) = io.lock().unwrap().as_mut() {
                let n = io.playback.pop_slice(&mut buf);
                e += buf[..n].iter().map(|x| x * x).sum::<f32>();
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        e
    };
    heard().await; // the join chime
    assert!(heard().await < 1e-3, "silence before the music");

    let entry = MusicEntryReq { video_id: "tone0000000".into(), title: "Tone".into(), artist: "Test".into(), album: None, art_url: String::new(), duration_ms: 1500, video: false };
    ctl.send_ws(WsClientMsg::Music { cmd: MusicCmd::Add { entry }, text_channel_id: Some(text) });
    eventually("the song is set playing", || !sets.lock().unwrap().is_empty()).await;
    let (_, at, paused) = sets.lock().unwrap()[0];
    assert!(!paused);
    assert!(at < 400, "starts near the beginning, at {at} ms");

    // The engine's output now carries the song.
    let heard = heard().await;
    assert!(heard > 0.5, "music reaches the speakers: {heard}");
}
