//! Listen together: per voice channel, a queue and a play state. Every app plays the song itself from
//! YouTube Music, lined up to `started_at`; the server never talks to YouTube. Pure logic here (time is
//! passed in), wiring to WebSocket and chat in `music::wire`.
use std::collections::VecDeque;

use rynuvo_proto::{Id, MusicEntry, MusicEntryReq, MusicState};

pub mod wire;

/// A finished song moves on this long after its end, so every app has played its last second.
pub const END_GRACE_MS: i64 = 1500;
pub const MAX_QUEUE: usize = 50;
pub const MAX_DURATION_MS: u32 = 20 * 60 * 1000;

#[derive(Clone, Debug, PartialEq)]
pub struct Playing {
    pub entry: MusicEntry,
    /// Server time at which position 0 played (moves on resume and seek).
    pub started_at: i64,
    /// Position when paused.
    pub paused_at: Option<u32>,
}

#[derive(Default, Debug)]
pub struct Session {
    pub playing: Option<Playing>,
    pub queue: VecDeque<MusicEntry>,
    /// Text channel the bot answers in when a control comes from a button.
    pub reply_to: Option<Id>,
    /// The channel's lyrics source (an id from `LYRICS_SOURCES`), "auto" until someone picks.
    pub lyrics: Option<String>,
    pub repeat: Repeat,
    next_id: u64,
}

/// Loop mode.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Repeat {
    #[default]
    Off,
    /// The song plays again when it ends (skip still moves on).
    Song,
    /// Finished and skipped songs go to the back of the queue.
    Queue,
}

impl Repeat {
    pub fn id(self) -> &'static str {
        match self {
            Repeat::Off => "off",
            Repeat::Song => "song",
            Repeat::Queue => "queue",
        }
    }

    /// "song"/"track"/"one", "queue"/"all", "off"/"none".
    pub fn parse(s: &str) -> Option<Repeat> {
        match s.trim().to_ascii_lowercase().as_str() {
            "off" | "none" => Some(Repeat::Off),
            "song" | "track" | "one" | "single" => Some(Repeat::Song),
            "queue" | "all" => Some(Repeat::Queue),
            _ => None,
        }
    }
}

/// Who acts, and what the room looks like, for permission checks.
pub struct Actor<'a> {
    pub id: Id,
    pub admin: bool,
    /// Owns the voice channel, or is an admin: may pick the lyrics source.
    pub manager: bool,
    /// Is this user in the voice channel right now?
    pub present: &'a dyn Fn(Id) -> bool,
    pub nick: &'a dyn Fn(Id) -> String,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Cmd {
    Add(MusicEntryReq),
    Lyrics(String),
    /// None = the next mode.
    Repeat(Option<String>),
    Skip,
    Pause,
    Resume,
    Seek(u32),
    /// 1-based queue position (the playing song is not counted).
    Remove(usize),
    Stop,
    Queue,
    NowPlaying,
    NotFound(String),
}

#[derive(Debug, Default, PartialEq)]
pub struct Outcome {
    /// The state changed: broadcast it.
    pub changed: bool,
    /// What the bot says in chat.
    pub reply: Option<String>,
}

pub fn clock(ms: u32) -> String {
    let s = ms / 1000;
    format!("{}:{:02}", s / 60, s % 60)
}

pub(crate) fn line(e: &MusicEntry) -> String {
    format!("{} — {} ({})", e.artist, e.title, clock(e.duration_ms))
}

/// The requester controls their song; if they left the channel, anyone may. Admins always may.
pub fn may_control(requested_by: Id, a: &Actor) -> bool {
    a.admin || a.id == requested_by || !(a.present)(requested_by)
}

pub fn valid(req: &MusicEntryReq) -> bool {
    let id_ok = !req.video_id.is_empty() && req.video_id.len() <= 32 && req.video_id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    let short = |s: &str, n: usize| s.chars().count() <= n;
    id_ok
        && !req.title.trim().is_empty()
        && short(&req.title, 200)
        && short(&req.artist, 200)
        && req.album.as_deref().is_none_or(|a| short(a, 200))
        && short(&req.art_url, 1000)
        && (req.art_url.is_empty() || req.art_url.starts_with("https://"))
        && (1000..=MAX_DURATION_MS).contains(&req.duration_ms)
}

impl Session {
    pub fn position(&self, now: i64) -> u32 {
        match &self.playing {
            None => 0,
            Some(p) => p.paused_at.unwrap_or_else(|| (now - p.started_at).clamp(0, p.entry.duration_ms as i64) as u32),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.playing.is_none() && self.queue.is_empty()
    }

    /// The playing song has ended (and the grace has passed).
    pub fn finished(&self, now: i64) -> bool {
        self.playing.as_ref().is_some_and(|p| p.paused_at.is_none() && now >= p.started_at + p.entry.duration_ms as i64 + END_GRACE_MS)
    }

    /// Start the next song (or stop), following the loop mode. `ended`: the song finished on its own
    /// (a looped song plays again); false for a skip. Returns the song now playing.
    pub fn advance(&mut self, now: i64, ended: bool) -> Option<&MusicEntry> {
        if let Some(p) = self.playing.take() {
            match self.repeat {
                Repeat::Song if ended => self.queue.push_front(p.entry),
                Repeat::Queue => self.queue.push_back(p.entry),
                _ => {}
            }
        }
        self.playing = self.queue.pop_front().map(|entry| Playing { entry, started_at: now, paused_at: None });
        self.playing.as_ref().map(|p| &p.entry)
    }

    pub fn state(&self, channel_id: Id, now: i64) -> MusicState {
        MusicState {
            channel_id,
            now: self.playing.as_ref().map(|p| p.entry.clone()),
            started_at: self.playing.as_ref().map_or(0, |p| p.started_at),
            paused_at: self.playing.as_ref().and_then(|p| p.paused_at),
            queue: self.queue.iter().cloned().collect(),
            server_time: now,
            lyrics: self.lyrics.clone().unwrap_or_else(|| "auto".into()),
            repeat: self.repeat.id().into(),
        }
    }

    fn now_playing_line(&self) -> String {
        match &self.playing {
            Some(p) => format!("▶ Now playing: {}", line(&p.entry)),
            None => "Nothing is playing".into(),
        }
    }

    pub fn handle(&mut self, cmd: Cmd, a: &Actor, now: i64) -> Outcome {
        let say = |s: String| Outcome { changed: false, reply: Some(s) };
        let did = |s: String| Outcome { changed: true, reply: Some(s) };
        let nick = |id: Id| (a.nick)(id);
        match cmd {
            Cmd::Lyrics(src) => {
                let Some((id, name)) = rynuvo_proto::LYRICS_SOURCES.iter().find(|(id, _)| *id == src) else {
                    return Outcome::default();
                };
                if !a.manager {
                    return say("Only the channel's owner or an admin can change the lyrics source".into());
                }
                self.lyrics = Some(id.to_string());
                did(format!("{} switched lyrics to {name}", nick(a.id)))
            }
            Cmd::Repeat(mode) => {
                let next = match mode {
                    None => match self.repeat {
                        Repeat::Off => Repeat::Queue,
                        Repeat::Queue => Repeat::Song,
                        Repeat::Song => Repeat::Off,
                    },
                    Some(m) => match Repeat::parse(&m) {
                        Some(r) => r,
                        None => return say("Loop can be: off, song or queue".into()),
                    },
                };
                if next == self.repeat {
                    return say(match next {
                        Repeat::Off => "Loop is already off".into(),
                        r => format!("Loop is already on: {}", r.id()),
                    });
                }
                self.repeat = next;
                did(match next {
                    Repeat::Off => format!("{} turned loop off", nick(a.id)),
                    Repeat::Song => format!("🔂 {} turned on loop: song", nick(a.id)),
                    Repeat::Queue => format!("🔁 {} turned on loop: queue", nick(a.id)),
                })
            }
            Cmd::NotFound(q) => say(format!("Nothing found for \"{}\"", q.chars().take(100).collect::<String>())),
            Cmd::Add(req) => {
                if !valid(&req) {
                    return say("That song can't be played".into());
                }
                if self.queue.len() >= MAX_QUEUE {
                    return say(format!("The queue is full ({MAX_QUEUE} songs)"));
                }
                self.next_id += 1;
                let e = MusicEntry {
                    id: self.next_id,
                    video_id: req.video_id,
                    title: req.title,
                    artist: req.artist,
                    album: req.album,
                    art_url: req.art_url,
                    duration_ms: req.duration_ms,
                    requested_by: a.id,
                };
                if self.playing.is_none() {
                    let text = format!("▶ Now playing: {} · requested by {}", line(&e), nick(a.id));
                    self.playing = Some(Playing { entry: e, started_at: now, paused_at: None });
                    did(text)
                } else {
                    let text = format!("+ Added to queue (#{}): {}", self.queue.len() + 1, line(&e));
                    self.queue.push_back(e);
                    did(text)
                }
            }
            Cmd::Queue => {
                if self.is_empty() {
                    return say("The queue is empty".into());
                }
                let mut s = self.now_playing_line();
                for (i, e) in self.queue.iter().enumerate() {
                    s.push_str(&format!("\n{}. {} · {}", i + 1, line(e), nick(e.requested_by)));
                }
                say(s)
            }
            Cmd::NowPlaying => match &self.playing {
                None => say("Nothing is playing".into()),
                Some(p) => say(format!(
                    "▶ {} · {} / {} · requested by {}",
                    format!("{} — {}", p.entry.artist, p.entry.title),
                    clock(self.position(now)),
                    clock(p.entry.duration_ms),
                    nick(p.entry.requested_by)
                )),
            },
            Cmd::Remove(n) => {
                let Some(e) = n.checked_sub(1).and_then(|i| self.queue.get(i)) else {
                    return say(format!("There's no song #{n} in the queue"));
                };
                if !may_control(e.requested_by, a) {
                    return say(format!("Only {} can remove #{n}", nick(e.requested_by)));
                }
                let e = self.queue.remove(n - 1).expect("checked above");
                did(format!("✕ {} removed {} from the queue", nick(a.id), e.title))
            }
            Cmd::Stop => {
                if self.is_empty() {
                    return say("Nothing is playing".into());
                }
                let all_mine = self.playing.iter().map(|p| &p.entry).chain(self.queue.iter()).all(|e| may_control(e.requested_by, a));
                if !all_mine {
                    return say("You can stop the music only when every song left is yours".into());
                }
                self.playing = None;
                self.queue.clear();
                did(format!("⏹ {} stopped the music", nick(a.id)))
            }
            Cmd::Skip | Cmd::Pause | Cmd::Resume | Cmd::Seek(_) => {
                let pos = self.position(now);
                let Some(p) = self.playing.as_mut() else { return say("Nothing is playing".into()) };
                let verb = match cmd {
                    Cmd::Skip => "skip",
                    Cmd::Pause => "pause",
                    Cmd::Resume => "resume",
                    _ => "seek",
                };
                if !may_control(p.entry.requested_by, a) {
                    return say(format!("Only {} can {verb} this song", nick(p.entry.requested_by)));
                }
                match cmd {
                    Cmd::Skip => {
                        let title = p.entry.title.clone();
                        let mut text = format!("⏭ {} skipped {title}", nick(a.id));
                        if self.advance(now, false).is_some() {
                            text.push('\n');
                            text.push_str(&self.now_playing_line());
                            text.push_str(&format!(" · requested by {}", nick(self.playing.as_ref().unwrap().entry.requested_by)));
                        }
                        did(text)
                    }
                    Cmd::Pause if p.paused_at.is_some() => say("Already paused".into()),
                    Cmd::Pause => {
                        p.paused_at = Some(pos);
                        did(format!("⏸ {} paused", nick(a.id)))
                    }
                    Cmd::Resume => match p.paused_at.take() {
                        None => say("Already playing".into()),
                        Some(at) => {
                            p.started_at = now - at as i64;
                            did(format!("▶ {} resumed", nick(a.id)))
                        }
                    },
                    Cmd::Seek(ms) => {
                        let ms = ms.min(p.entry.duration_ms.saturating_sub(1000));
                        match p.paused_at.as_mut() {
                            Some(at) => *at = ms,
                            None => p.started_at = now - ms as i64,
                        }
                        did(format!("⏩ {} jumped to {}", nick(a.id), clock(ms)))
                    }
                    _ => unreachable!(),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const POKY: Id = 1;
    const XENO: Id = 2;
    const ADMIN: Id = 9;

    fn req(t: &str) -> MusicEntryReq {
        MusicEntryReq { video_id: format!("vid{t}"), title: t.into(), artist: "LANY".into(), album: None, art_url: String::new(), duration_ms: 200_000 }
    }

    fn nick(id: Id) -> String {
        match id {
            POKY => "PokyVL".into(),
            XENO => "Xeno".into(),
            _ => "Admin".into(),
        }
    }

    fn run(s: &mut Session, cmd: Cmd, who: Id, present: &[Id], now: i64) -> Outcome {
        let present = present.to_vec();
        let p = move |id: Id| present.contains(&id);
        s.handle(cmd, &Actor { id: who, admin: who == ADMIN, manager: who == ADMIN, present: &p, nick: &nick }, now)
    }

    #[test]
    fn only_the_channel_manager_picks_the_lyrics_source() {
        let mut s = Session::default();
        assert_eq!(s.state(5, 0).lyrics, "auto");
        let o = run(&mut s, Cmd::Lyrics("lrclib".into()), XENO, &[XENO], 0);
        assert!(!o.changed);
        assert_eq!(o.reply.unwrap(), "Only the channel's owner or an admin can change the lyrics source");
        let o = run(&mut s, Cmd::Lyrics("betterlyrics".into()), ADMIN, &[ADMIN], 0);
        assert!(o.changed);
        assert_eq!(o.reply.unwrap(), "Admin switched lyrics to BetterLyrics");
        assert_eq!(s.state(5, 0).lyrics, "betterlyrics");
        let o = run(&mut s, Cmd::Lyrics("nope".into()), ADMIN, &[ADMIN], 0);
        assert!(!o.changed, "unknown sources are ignored");
        assert_eq!(s.state(5, 0).lyrics, "betterlyrics");
    }

    #[test]
    fn first_song_plays_and_the_next_queues() {
        let mut s = Session::default();
        let o = run(&mut s, Cmd::Add(req("Stuck")), POKY, &[POKY], 1000);
        assert!(o.changed);
        assert_eq!(o.reply.unwrap(), "▶ Now playing: LANY — Stuck (3:20) · requested by PokyVL");
        let o = run(&mut s, Cmd::Add(req("ILYSB")), XENO, &[POKY, XENO], 2000);
        assert_eq!(o.reply.unwrap(), "+ Added to queue (#1): LANY — ILYSB (3:20)");
        assert_eq!(s.playing.as_ref().unwrap().entry.title, "Stuck");
        assert_eq!(s.queue.len(), 1);
    }

    #[test]
    fn only_the_requester_skips_the_playing_song() {
        let mut s = Session::default();
        run(&mut s, Cmd::Add(req("Stuck")), POKY, &[POKY], 0);
        run(&mut s, Cmd::Add(req("ILYSB")), XENO, &[POKY, XENO], 0);
        let o = run(&mut s, Cmd::Skip, XENO, &[POKY, XENO], 10);
        assert!(!o.changed);
        assert_eq!(o.reply.unwrap(), "Only PokyVL can skip this song");
        let o = run(&mut s, Cmd::Pause, XENO, &[POKY, XENO], 10);
        assert_eq!(o.reply.unwrap(), "Only PokyVL can pause this song");
        let o = run(&mut s, Cmd::Skip, POKY, &[POKY, XENO], 10);
        assert!(o.changed);
        assert_eq!(o.reply.unwrap(), "⏭ PokyVL skipped Stuck\n▶ Now playing: LANY — ILYSB (3:20) · requested by Xeno");
        assert_eq!(s.playing.as_ref().unwrap().started_at, 10, "next song starts now");
    }

    #[test]
    fn anyone_may_control_once_the_requester_left_and_admin_always() {
        let mut s = Session::default();
        run(&mut s, Cmd::Add(req("Stuck")), POKY, &[POKY], 0);
        assert!(run(&mut s, Cmd::Pause, ADMIN, &[POKY, ADMIN], 5).changed, "admin");
        assert!(run(&mut s, Cmd::Resume, XENO, &[XENO], 6).changed, "PokyVL left the channel");
    }

    #[test]
    fn position_follows_pause_resume_and_seek() {
        let mut s = Session::default();
        run(&mut s, Cmd::Add(req("Stuck")), POKY, &[POKY], 1_000);
        assert_eq!(s.position(31_000), 30_000);
        run(&mut s, Cmd::Pause, POKY, &[POKY], 31_000);
        assert_eq!(s.position(90_000), 30_000, "frozen while paused");
        run(&mut s, Cmd::Resume, POKY, &[POKY], 90_000);
        assert_eq!(s.position(100_000), 40_000);
        let o = run(&mut s, Cmd::Seek(120_000), POKY, &[POKY], 100_000);
        assert_eq!(o.reply.unwrap(), "⏩ PokyVL jumped to 2:00");
        assert_eq!(s.position(101_000), 121_000);
        assert_eq!(s.state(7, 101_000).started_at, 100_000 - 120_000);
    }

    #[test]
    fn a_song_ends_after_its_duration_plus_grace() {
        let mut s = Session::default();
        run(&mut s, Cmd::Add(req("Stuck")), POKY, &[POKY], 0);
        assert!(!s.finished(200_000));
        assert!(s.finished(200_000 + END_GRACE_MS));
        run(&mut s, Cmd::Pause, POKY, &[POKY], 10_000);
        assert!(!s.finished(10_000_000), "a paused song never ends");
    }

    #[test]
    fn remove_and_stop_respect_who_requested() {
        let mut s = Session::default();
        run(&mut s, Cmd::Add(req("Stuck")), POKY, &[POKY, XENO], 0);
        run(&mut s, Cmd::Add(req("A")), XENO, &[POKY, XENO], 0);
        run(&mut s, Cmd::Add(req("B")), POKY, &[POKY, XENO], 0);
        assert_eq!(run(&mut s, Cmd::Remove(1), POKY, &[POKY, XENO], 0).reply.unwrap(), "Only Xeno can remove #1");
        assert_eq!(run(&mut s, Cmd::Remove(5), POKY, &[POKY, XENO], 0).reply.unwrap(), "There's no song #5 in the queue");
        assert_eq!(run(&mut s, Cmd::Stop, POKY, &[POKY, XENO], 0).reply.unwrap(), "You can stop the music only when every song left is yours");
        assert!(run(&mut s, Cmd::Remove(1), XENO, &[POKY, XENO], 0).changed);
        assert_eq!(run(&mut s, Cmd::Stop, POKY, &[POKY, XENO], 0).reply.unwrap(), "⏹ PokyVL stopped the music");
        assert!(s.is_empty());
    }

    #[test]
    fn bad_entries_are_refused() {
        let mut ok = req("x");
        assert!(valid(&ok));
        ok.video_id = "../../x".into();
        assert!(!valid(&ok));
        let mut long = req("x");
        long.duration_ms = MAX_DURATION_MS + 1;
        assert!(!valid(&long));
        let mut art = req("x");
        art.art_url = "javascript:alert(1)".into();
        assert!(!valid(&art));
    }

    #[test]
    fn queue_and_now_playing_replies() {
        let mut s = Session::default();
        assert_eq!(run(&mut s, Cmd::Queue, POKY, &[POKY], 0).reply.unwrap(), "The queue is empty");
        run(&mut s, Cmd::Add(req("Stuck")), POKY, &[POKY, XENO], 0);
        run(&mut s, Cmd::Add(req("A")), XENO, &[POKY, XENO], 0);
        assert_eq!(
            run(&mut s, Cmd::Queue, POKY, &[POKY], 0).reply.unwrap(),
            "▶ Now playing: LANY — Stuck (3:20)\n1. LANY — A (3:20) · Xeno"
        );
        assert_eq!(run(&mut s, Cmd::NowPlaying, XENO, &[POKY], 76_000).reply.unwrap(), "▶ LANY — Stuck · 1:16 / 3:20 · requested by PokyVL");
    }
    #[test]
    fn loop_cycles_off_queue_song_and_takes_a_mode() {
        let mut s = Session::default();
        assert_eq!(s.state(5, 0).repeat, "off");
        let o = run(&mut s, Cmd::Repeat(None), XENO, &[XENO], 0);
        assert!(o.changed);
        assert_eq!(o.reply.unwrap(), "🔁 Xeno turned on loop: queue");
        assert_eq!(run(&mut s, Cmd::Repeat(None), XENO, &[XENO], 0).reply.unwrap(), "🔂 Xeno turned on loop: song");
        assert_eq!(run(&mut s, Cmd::Repeat(None), XENO, &[XENO], 0).reply.unwrap(), "Xeno turned loop off");
        assert_eq!(run(&mut s, Cmd::Repeat(Some("song".into())), POKY, &[POKY], 0).reply.unwrap(), "🔂 PokyVL turned on loop: song");
        assert_eq!(s.state(5, 0).repeat, "song");
        let o = run(&mut s, Cmd::Repeat(Some("song".into())), POKY, &[POKY], 0);
        assert!(!o.changed);
        assert_eq!(o.reply.unwrap(), "Loop is already on: song");
    }

    #[test]
    fn looping_a_song_plays_it_again_but_skip_moves_on() {
        let mut s = Session::default();
        run(&mut s, Cmd::Add(req("Stuck")), POKY, &[POKY], 0);
        run(&mut s, Cmd::Add(req("A")), POKY, &[POKY], 0);
        run(&mut s, Cmd::Repeat(Some("song".into())), POKY, &[POKY], 0);
        assert_eq!(s.advance(300_000, true).unwrap().title, "Stuck");
        assert_eq!(s.playing.as_ref().unwrap().started_at, 300_000, "from the top");
        assert_eq!(s.queue.len(), 1);
        run(&mut s, Cmd::Skip, POKY, &[POKY], 310_000);
        assert_eq!(s.playing.as_ref().unwrap().entry.title, "A");
    }

    #[test]
    fn looping_the_queue_sends_each_song_to_the_back() {
        let mut s = Session::default();
        run(&mut s, Cmd::Add(req("Stuck")), POKY, &[POKY], 0);
        run(&mut s, Cmd::Add(req("A")), POKY, &[POKY], 0);
        run(&mut s, Cmd::Repeat(Some("queue".into())), POKY, &[POKY], 0);
        assert_eq!(s.advance(300_000, true).unwrap().title, "A");
        assert_eq!(s.queue.iter().map(|e| e.title.as_str()).collect::<Vec<_>>(), ["Stuck"]);
        run(&mut s, Cmd::Skip, POKY, &[POKY], 310_000);
        assert_eq!(s.playing.as_ref().unwrap().entry.title, "Stuck", "skipping keeps it in the loop too");
        assert_eq!(s.queue.len(), 1);
    }
}
