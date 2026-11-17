//! Go Live end to end over a local server: synthetic capture → encoder → relay → viewer access units.
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use rynuvo_desktop_lib::{
    core::{
        http::Api,
        identity::MemoryStore,
        session::{Session, SessionEvent},
    },
    stream::{
        capture::CaptureOpts,
        ctl::{StartCapture, StreamCtl},
        engine::StreamEvent,
        encode::Frame,
    },
    voice::{
        backend::{AudioBackend, AudioIo},
        ctl::VoiceCtl,
    },
};
use rynuvo_server::testutil::TestApp;

struct NullBackend(Option<AudioIo>);

impl AudioBackend for NullBackend {
    fn start(&mut self, _i: Option<&str>, _o: Option<&str>, io: AudioIo) -> Result<(), String> {
        self.0 = Some(io);
        Ok(())
    }
    fn stop(&mut self) {
        self.0 = None;
    }
}

async fn eventually(what: &str, secs: u64, f: impl Fn() -> bool) {
    for _ in 0..secs * 20 {
        if f() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("timed out waiting for: {what}");
}

/// A moving gradient at 30 fps, as tightly packed BGRA.
fn synthetic() -> StartCapture {
    synthetic_with(None, false)
}

/// `end_after`: the source goes away (window closed) after that long. `broken`: 0×0 frames no
/// encoder accepts.
fn synthetic_with(end_after: Option<Duration>, broken: bool) -> StartCapture {
    Arc::new(move |_source: &str, opts: CaptureOpts, video, _audio, ended| {
        let stop = Arc::new(AtomicBool::new(false));
        let s2 = stop.clone();
        let (w, h) = (320u32.min(opts.max_width), 240u32.min(opts.max_height));
        std::thread::spawn(move || {
            let mut t = 0u8;
            let started = std::time::Instant::now();
            while !s2.load(Ordering::Relaxed) {
                if end_after.is_some_and(|d| started.elapsed() >= d) {
                    ended("The shared window was closed".into());
                    return;
                }
                if broken {
                    video(Frame::Bgra { width: 0, height: 0, data: Vec::new() });
                    std::thread::sleep(Duration::from_millis(33));
                    continue;
                }
                let mut data = vec![0u8; (w * h * 4) as usize];
                for (i, px) in data.chunks_exact_mut(4).enumerate() {
                    let (x, y) = (i as u32 % w, i as u32 / w);
                    px.copy_from_slice(&[(x as u8).wrapping_add(t), (y as u8).wrapping_sub(t), t, 255]);
                }
                video(Frame::Bgra { width: w, height: h, data });
                t = t.wrapping_add(3);
                std::thread::sleep(Duration::from_millis(33));
            }
        });
        Ok(Box::new(move || stop.store(true, Ordering::Relaxed)) as Box<dyn FnOnce() + Send>)
    })
}

struct Peer {
    voice: VoiceCtl,
    stream: StreamCtl,
    events: Arc<Mutex<Vec<StreamEvent>>>,
    user: i64,
}

async fn peer(app: &TestApp, nick: &str, ch: i64) -> Peer {
    peer_with(app, nick, ch, synthetic()).await
}

async fn peer_with(app: &TestApp, nick: &str, ch: i64, capture: StartCapture) -> Peer {
    let u = app.user(nick).await;
    let base = format!("http://{}", app.addr);
    let voice = VoiceCtl::new(Arc::new(|| Box::new(NullBackend(None)) as Box<dyn AudioBackend>), Arc::new(|_| {}));
    let events = Arc::new(Mutex::new(Vec::new()));
    let ev2 = events.clone();
    let stream = StreamCtl::new(voice.clone(), capture, Arc::new(move |e| ev2.lock().unwrap().push(e)), tokio::runtime::Handle::current());
    let (v2, s2) = (voice.clone(), stream.clone());
    let emit = Arc::new(move |e: SessionEvent| {
        v2.on_session(&e);
        s2.on_session(&e);
    });
    let s = Session::start(Api::new(&base, Some(u.token.clone())).unwrap(), Arc::new(MemoryStore::default()), base, emit);
    voice.attach(s, "127.0.0.1".into());
    let uid = u.user.id;
    eventually("online", 5, || app.state.hub.online_ids().contains(&uid)).await;
    voice.join(ch);
    eventually("in voice", 5, || app.state.voice.members().iter().any(|m| m.user_id == uid)).await;
    Peer { voice, stream, events, user: uid }
}

type Aus = Arc<Mutex<Vec<Vec<u8>>>>;

fn collector() -> (Aus, Arc<dyn Fn(Vec<u8>) + Send + Sync>) {
    let aus: Aus = Arc::default();
    let a2 = aus.clone();
    (aus, Arc::new(move |b| a2.lock().unwrap().push(b)))
}

/// Access unit framing to the webview: [codec u8][key u8][ts u32 BE][AU].
fn is_key(au: &[u8]) -> bool {
    au.len() > 6 && au[1] == 1
}

#[tokio::test(flavor = "multi_thread")]
async fn two_clients_stream_over_relay() {
    let app = TestApp::spawn().await;
    let u = app.user("Owner").await;
    let ch = app.create_channel(&u.token, "voice", "Mabar").await.id;
    let a = peer(&app, "Dimas", ch).await;
    let b = peer(&app, "Ayu", ch).await;
    // No STUN servers: no direct candidates, so everything rides the relay.
    a.stream.go_live("synthetic:0", "720p30", true).unwrap();
    eventually("stream registered", 5, || app.state.voice.streams().iter().any(|s| s.user_id == a.user)).await;
    let (aus, out) = collector();
    b.stream.watch(a.user, vec!["h264".into()], out).unwrap();
    eventually("access units arrive", 10, || aus.lock().unwrap().len() >= 10).await;
    let got = aus.lock().unwrap().clone();
    assert!(is_key(&got[0]), "the first access unit a viewer gets is a key frame");
    assert_eq!(got[0][0], 1, "H.264, the only codec this viewer can decode");
    // Annex B start code right after the 6-byte header.
    assert_eq!(&got[0][6..10], &[0, 0, 0, 1]);
    eventually("viewer stats", 5, || b.events.lock().unwrap().iter().any(|e| matches!(e, StreamEvent::Stats { user_id, .. } if *user_id == a.user))).await;
    a.stream.stop_live();
    let _ = (&a.voice, &b.voice);
}

#[tokio::test(flavor = "multi_thread")]
async fn the_cursor_choice_reaches_the_capture() {
    let app = TestApp::spawn().await;
    let u = app.user("Owner").await;
    let ch = app.create_channel(&u.token, "voice", "Mabar").await.id;
    let seen = Arc::new(Mutex::new(Vec::new()));
    let (s2, inner) = (seen.clone(), synthetic());
    let capture: StartCapture = Arc::new(move |src: &str, opts: CaptureOpts, v, a, e| {
        s2.lock().unwrap().push(opts.cursor);
        inner(src, opts, v, a, e)
    });
    let a = peer_with(&app, "Dimas", ch, capture).await;
    a.stream.go_live("synthetic:0", "720p30", false).unwrap();
    a.stream.stop_live();
    a.stream.go_live("synthetic:0", "720p30", true).unwrap();
    a.stream.stop_live();
    assert_eq!(*seen.lock().unwrap(), vec![false, true]);
}

#[tokio::test(flavor = "multi_thread")]
async fn viewer_join_gets_keyframe() {
    let app = TestApp::spawn().await;
    let u = app.user("Owner").await;
    let ch = app.create_channel(&u.token, "voice", "Mabar").await.id;
    let a = peer(&app, "Dimas", ch).await;
    let b = peer(&app, "Ayu", ch).await;
    let c = peer(&app, "Raka", ch).await;
    a.stream.go_live("synthetic:0", "720p30", true).unwrap();
    eventually("stream registered", 5, || app.state.voice.streams().iter().any(|s| s.user_id == a.user)).await;
    let (b_aus, b_out) = collector();
    b.stream.watch(a.user, vec!["h264".into(), "hevc".into()], b_out).unwrap();
    eventually("B watching", 10, || b_aus.lock().unwrap().len() >= 30).await;
    let started = std::time::Instant::now();
    let (c_aus, c_out) = collector();
    c.stream.watch(a.user, vec!["h264".into()], c_out).unwrap();
    eventually("C gets frames", 10, || !c_aus.lock().unwrap().is_empty()).await;
    assert!(is_key(&c_aus.lock().unwrap()[0]), "a mid-stream joiner starts on a key frame");
    assert!(started.elapsed() < Duration::from_secs(3), "within a few seconds: {:?}", started.elapsed());
    // One encoder per stream: with C (H.264 only) watching, B's frames are H.264 too from here on.
    let n = b_aus.lock().unwrap().len();
    eventually("B continues", 5, || b_aus.lock().unwrap().len() > n + 10).await;
    assert_eq!(b_aus.lock().unwrap().last().unwrap()[0], 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn stream_ends_on_streamer_leave() {
    let app = TestApp::spawn().await;
    let u = app.user("Owner").await;
    let ch = app.create_channel(&u.token, "voice", "Mabar").await.id;
    let a = peer(&app, "Dimas", ch).await;
    let b = peer(&app, "Ayu", ch).await;
    a.stream.go_live("synthetic:0", "720p30", true).unwrap();
    eventually("stream registered", 5, || app.state.voice.streams().iter().any(|s| s.user_id == a.user)).await;
    let (aus, out) = collector();
    b.stream.watch(a.user, vec!["h264".into()], out).unwrap();
    eventually("frames", 10, || !aus.lock().unwrap().is_empty()).await;
    a.voice.leave();
    eventually("viewer told the stream ended", 5, || {
        b.events.lock().unwrap().iter().any(|e| matches!(e, StreamEvent::Ended { user_id, .. } if *user_id == a.user))
    })
    .await;
    eventually("streamer told it ended", 5, || a.events.lock().unwrap().iter().any(|e| matches!(e, StreamEvent::Ended { user_id, .. } if *user_id == a.user))).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn go_live_needs_voice() {
    let app = TestApp::spawn().await;
    let u = app.user("Owner").await;
    let ch = app.create_channel(&u.token, "voice", "Mabar").await.id;
    let a = peer(&app, "Dimas", ch).await;
    a.voice.leave();
    let err = a.stream.go_live("synthetic:0", "1080p60", true).unwrap_err();
    assert_eq!(err, "Join a voice channel first");
}

fn ended_for(p: &Peer, user: i64) -> bool {
    p.events.lock().unwrap().iter().any(|e| matches!(e, StreamEvent::Ended { user_id, .. } if *user_id == user))
}

#[tokio::test(flavor = "multi_thread")]
async fn closed_source_ends_the_stream_for_everyone() {
    let app = TestApp::spawn().await;
    let u = app.user("Owner").await;
    let ch = app.create_channel(&u.token, "voice", "Mabar").await.id;
    let a = peer_with(&app, "Dimas", ch, synthetic_with(Some(Duration::from_secs(2)), false)).await;
    let b = peer(&app, "Ayu", ch).await;
    a.stream.go_live("synthetic:0", "720p30", true).unwrap();
    eventually("stream registered", 5, || app.state.voice.streams().iter().any(|s| s.user_id == a.user)).await;
    let (aus, out) = collector();
    b.stream.watch(a.user, vec!["h264".into()], out).unwrap();
    eventually("frames", 10, || !aus.lock().unwrap().is_empty()).await;
    eventually("server forgets the stream", 5, || app.state.voice.streams().is_empty()).await;
    eventually("viewer told", 5, || ended_for(&b, a.user)).await;
    eventually("streamer told", 5, || a.events.lock().unwrap().iter().any(|e| matches!(e, StreamEvent::Error { message } if message.contains("closed")))).await;
    assert!(!a.stream.is_live());
}

#[tokio::test(flavor = "multi_thread")]
async fn encoder_failure_ends_the_stream() {
    let app = TestApp::spawn().await;
    let u = app.user("Owner").await;
    let ch = app.create_channel(&u.token, "voice", "Mabar").await.id;
    let a = peer_with(&app, "Dimas", ch, synthetic_with(None, true)).await;
    let b = peer(&app, "Ayu", ch).await;
    a.stream.go_live("synthetic:0", "720p30", true).unwrap();
    eventually("stream registered", 5, || app.state.voice.streams().iter().any(|s| s.user_id == a.user)).await;
    let (_aus, out) = collector();
    b.stream.watch(a.user, vec!["h264".into()], out).unwrap();
    eventually("server forgets the stream", 10, || app.state.voice.streams().is_empty()).await;
    assert!(!a.stream.is_live());
    eventually("viewer told", 5, || ended_for(&b, a.user)).await;
}

/// The viewer gives up on its own (nothing arrives): the server must free the viewer slot and the
/// streamer must stop sending to it.
#[tokio::test(flavor = "multi_thread")]
async fn viewer_timeout_releases_the_watch() {
    let app = TestApp::spawn().await;
    let u = app.user("Owner").await;
    let ch = app.create_channel(&u.token, "voice", "Mabar").await.id;
    let a = peer(&app, "Dimas", ch).await;
    let b = peer(&app, "Ayu", ch).await;
    a.stream.go_live("synthetic:0", "720p30", true).unwrap();
    eventually("stream registered", 5, || app.state.voice.streams().iter().any(|s| s.user_id == a.user)).await;
    let (aus, out) = collector();
    b.stream.watch(a.user, vec!["h264".into()], out).unwrap();
    eventually("frames", 10, || !aus.lock().unwrap().is_empty()).await;
    let q = &app.state.relay.quota;
    q.add(q.limit()); // relay stops carrying video
    eventually("viewer times out", 15, || ended_for(&b, a.user)).await;
    eventually("streamer drops the viewer", 5, || {
        a.events.lock().unwrap().iter().rev().find_map(|e| match e {
            StreamEvent::Live { viewers, .. } => Some(viewers.is_empty()),
            _ => None,
        }) == Some(true)
    })
    .await;
}

// ---------------------------------------------------------------------------------------------
// End-to-end encryption, at the engine level: a broadcast and its watches wired through a test
// relay that records every pair message (what the server would see), with no direct path.

mod e2ee_lab {
    use std::{
        collections::HashMap,
        sync::{Arc, Mutex},
        time::Instant,
    };

    use rynuvo_desktop_lib::{
        e2ee::{KeyState, Outgoing, SharedKeys},
        stream::{
            engine::{self, Broadcast, CaptureFn, Net, StreamEvent, Watch},
            frag::{Assembled, Reassembler},
        },
    };
    use rynuvo_proto::{
        Id,
        e2ee::{DeviceKey, Scope},
        voice_wire::VoiceKey,
    };

    pub const S: Id = 1;

    /// (from, to, when, message)
    pub type Sent = Arc<Mutex<Vec<(Id, Id, Instant, Vec<u8>)>>>;

    #[derive(Clone)]
    pub struct Lab {
        pub keys: Arc<HashMap<Id, SharedKeys>>,
        pub sent: Sent,
        bcast: Arc<Mutex<Option<Arc<Broadcast>>>>,
        watches: Arc<Mutex<HashMap<Id, Watch>>>,
        /// Nets get no keys (and no test switch): nothing may be sent or shown.
        pub keyless: bool,
    }

    impl Lab {
        /// Everyone knows everyone's device key; nobody holds a media key yet.
        pub fn new(users: &[Id]) -> Lab {
            let devs: Vec<(Id, DeviceKey)> = users.iter().map(|&u| (u, DeviceKey::generate())).collect();
            let keys = devs
                .iter()
                .map(|(u, dk)| {
                    let mut k = KeyState::new(*u, dk.clone());
                    for (o, odk) in &devs {
                        if o != u {
                            k.set_peer_pubkey(*o, odk.public());
                        }
                    }
                    (*u, Arc::new(Mutex::new(k)))
                })
                .collect();
            Lab { keys: Arc::new(keys), sent: Arc::default(), bcast: Arc::default(), watches: Arc::default(), keyless: false }
        }

        fn route(&self, from: Id, to: Id, b: Vec<u8>) {
            self.sent.lock().unwrap().push((from, to, Instant::now(), b.clone()));
            if from == S {
                if let Some(w) = self.watches.lock().unwrap().get(&to) {
                    w.on_relay(&b);
                }
            } else {
                let bc = self.bcast.lock().unwrap().clone();
                if let Some(bc) = bc {
                    bc.on_relay(from, &b);
                }
            }
        }

        pub fn net(&self, me: Id) -> Net {
            let lab = self.clone();
            Net {
                signal: Arc::new(|_, _| {}), // no candidates swapped: everything rides the relay
                relay: Arc::new(move |to, b| lab.route(me, to, b)),
                stun: Vec::new(),
                rt: tokio::runtime::Handle::current(),
                keys: (!self.keyless).then(|| self.keys[&me].clone()),
                me,
                unencrypted_for_tests: false,
            }
        }

        /// What the server does with an envelope: hand it to its recipient.
        pub fn deliver(&self, from: Id, out: Vec<Outgoing>) -> Vec<(Id, u32, bool)> {
            out.into_iter()
                .map(|o| {
                    let ok = self.keys[&o.to].lock().unwrap().on_envelope(from, &o.scope, o.generation, &o.envelope, Instant::now());
                    (o.to, o.generation, ok)
                })
                .collect()
        }

        pub fn go_live(&self, capture: CaptureFn) -> Arc<Broadcast> {
            let b = Arc::new(Broadcast::start(capture, engine::preset("720p30"), self.net(S), Arc::new(|_: StreamEvent| {})).unwrap());
            *self.bcast.lock().unwrap() = Some(b.clone());
            b
        }

        /// `viewer` starts watching; returns the streamer's envelopes for them (not yet delivered).
        pub fn watch(&self, viewer: Id, out: engine::FrameOut) -> Vec<Outgoing> {
            let key = VoiceKey::random();
            let w = Watch::new(S);
            w.connect(key.clone(), &self.net(viewer), out, Arc::new(|_, _| {}), Arc::new(|_| {}));
            self.watches.lock().unwrap().insert(viewer, w);
            let bc = self.bcast.lock().unwrap().clone().unwrap();
            bc.add_viewer(viewer, &["h264".to_string()], key)
        }

        pub fn stop_watch(&self, viewer: Id) -> Vec<Outgoing> {
            if let Some(w) = self.watches.lock().unwrap().remove(&viewer) {
                w.stop();
            }
            let bc = self.bcast.lock().unwrap().clone().unwrap();
            bc.remove_viewer(viewer)
        }

        /// The streamer learned a new device key for `viewer` (as `VoiceCtl` does): returns the rotation.
        pub fn rekey_viewer(&self, viewer: Id) -> Vec<Outgoing> {
            let bc = self.bcast.lock().unwrap().clone().unwrap();
            bc.rekey_viewer(viewer)
        }

        pub fn stop(&self) {
            self.watches.lock().unwrap().clear();
            self.bcast.lock().unwrap().take();
        }

        /// Whole (sealed) frames the streamer sent `to`, each with when its last fragment left.
        pub fn frames_to(&self, to: Id) -> Vec<(Instant, Assembled)> {
            let mut r = Reassembler::new();
            let mut out = Vec::new();
            for (from, t, at, m) in self.sent.lock().unwrap().iter() {
                if *from != S || *t != to {
                    continue;
                }
                let ready = match m.first() {
                    Some(&engine::T_VIDEO) => r.push(&m[1..]),
                    Some(&engine::T_FEC) => r.push_fec(&m[1..]),
                    _ => continue,
                };
                out.extend(ready.into_iter().map(|a| (*at, a)));
            }
            out
        }

        pub fn scope() -> Scope {
            Scope::Stream(S)
        }
    }
}

fn synthetic_capture() -> rynuvo_desktop_lib::stream::engine::CaptureFn {
    let s = synthetic();
    Arc::new(move |opts, video, audio, ended| s("synthetic:0", opts, video, audio, ended))
}

#[tokio::test(flavor = "multi_thread")]
async fn stream_frames_are_end_to_end_encrypted() {
    use e2ee_lab::*;
    use rynuvo_proto::e2ee::{FRAME_OVERHEAD, KIND_VIDEO, frame_generation};
    const B: i64 = 2;
    let lab = Lab::new(&[S, B]);
    lab.go_live(synthetic_capture());
    let (aus, out) = collector();
    let env = lab.watch(B, out);
    assert!(lab.deliver(S, env).iter().all(|(to, _, ok)| *to == B && *ok));
    eventually("viewer gets frames", 10, || aus.lock().unwrap().len() >= 10).await;
    let got = aus.lock().unwrap().clone();
    assert!(is_key(&got[0]), "first access unit is a key frame");
    assert_eq!(&got[0][6..10], &[0, 0, 0, 1], "decrypted back to Annex B");
    lab.stop();

    // Nothing the relay carried holds a piece of any picture.
    let sent = lab.sent.lock().unwrap().clone();
    for au in &got {
        let plain = &au[6..];
        let probe = &plain[..plain.len().min(32)];
        for (_, _, _, m) in &sent {
            assert!(!m.windows(probe.len()).any(|w| w == probe), "a pair message carries plaintext video");
        }
    }
    // Each frame on the wire is the viewer's frame sealed with the stream key (24 bytes more).
    let frames = lab.frames_to(B);
    assert!(frames.len() >= 10);
    let plains: Vec<&[u8]> = got.iter().map(|au| &au[6..]).collect();
    let mut matched = 0;
    for (_, f) in frames.iter().take(10) {
        assert!(frame_generation(&f.data).is_some());
        let opened = lab.keys[&B].lock().unwrap().open(&Lab::scope(), S, KIND_VIDEO, &f.data, std::time::Instant::now());
        let opened = opened.expect("the viewer's key opens what the streamer sent");
        assert_eq!(f.data.len(), opened.len() + FRAME_OVERHEAD);
        assert!(!plains.contains(&f.data.as_slice()));
        if plains.contains(&opened.as_slice()) {
            matched += 1;
        }
    }
    assert!(matched >= 5, "the viewer outputs the original frames: {matched}");
}

#[tokio::test(flavor = "multi_thread")]
async fn viewer_without_key_gets_no_picture_and_asks_keyframe() {
    use e2ee_lab::*;
    const B: i64 = 2;
    let lab = Lab::new(&[S, B]);
    lab.go_live(synthetic_capture());
    let (aus, out) = collector();
    let _never_delivered = lab.watch(B, out);
    eventually("frames reach the viewer", 10, || !lab.frames_to(B).is_empty()).await;
    tokio::time::sleep(Duration::from_secs(3)).await;
    assert!(aus.lock().unwrap().is_empty(), "no picture without the stream key");
    let keyreqs = lab
        .sent
        .lock()
        .unwrap()
        .iter()
        .filter(|(from, to, _, m)| *from == B && *to == S && m.as_slice() == [rynuvo_desktop_lib::stream::engine::T_KEYREQ])
        .count();
    assert!(keyreqs >= 1, "the viewer asks for a key frame");
    assert!(keyreqs <= 10, "but not for every frame: {keyreqs} in ~3 s");
    lab.stop();
}

#[tokio::test(flavor = "multi_thread")]
async fn viewer_leave_rotates_stream_key() {
    use e2ee_lab::*;
    use rynuvo_proto::e2ee::{KIND_VIDEO, frame_generation};
    const B: i64 = 2;
    const C: i64 = 3;
    let lab = Lab::new(&[S, B, C]);
    lab.go_live(synthetic_capture());
    let (b_aus, b_out) = collector();
    let (c_aus, c_out) = collector();
    let env = lab.watch(B, b_out);
    lab.deliver(S, env);
    let env = lab.watch(C, c_out);
    lab.deliver(S, env);
    eventually("both watch", 10, || b_aus.lock().unwrap().len() >= 10 && c_aus.lock().unwrap().len() >= 10).await;
    let gen0 = frame_generation(&lab.frames_to(B).last().unwrap().1.data).unwrap();

    let left = std::time::Instant::now();
    let env = lab.stop_watch(C);
    let got = lab.deliver(S, env);
    assert_eq!(got.len(), 1, "only the remaining viewer gets the new key: {got:?}");
    assert_eq!(got[0], (B, gen0.wrapping_add(1), true));

    let n = b_aus.lock().unwrap().len();
    tokio::time::sleep(Duration::from_millis(1500)).await;
    eventually("B keeps watching", 5, || b_aus.lock().unwrap().len() > n + 10).await;
    lab.stop();

    let after: Vec<_> = lab.frames_to(B).into_iter().filter(|(at, _)| *at > left).collect();
    // The forced key frame is sealed under the new key (it waits for the key to become active).
    let new_key = after.iter().find(|(_, f)| f.key && frame_generation(&f.data) == Some(gen0.wrapping_add(1)));
    let (at, _) = new_key.expect("a key frame under the new key follows the rotation");
    assert!(*at < left + Duration::from_millis(700), "promptly: {:?}", *at - left);
    let late: Vec<_> = after.iter().filter(|(at, _)| *at > left + Duration::from_secs(1)).collect();
    assert!(!late.is_empty());
    let now = std::time::Instant::now();
    for (_, f) in late {
        assert_eq!(frame_generation(&f.data), Some(gen0.wrapping_add(1)));
        assert!(lab.keys[&B].lock().unwrap().open(&Lab::scope(), S, KIND_VIDEO, &f.data, now).is_some());
        assert!(lab.keys[&C].lock().unwrap().open(&Lab::scope(), S, KIND_VIDEO, &f.data, now).is_none(), "the viewer who left can't open new frames");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn engine_without_keys_sends_nothing() {
    use e2ee_lab::*;
    use rynuvo_desktop_lib::stream::engine::{T_AUDIO, T_FEC, T_VIDEO};
    const B: i64 = 2;
    let mut lab = Lab::new(&[S, B]);
    lab.keyless = true;
    lab.go_live(synthetic_capture());
    let (aus, out) = collector();
    let _ = lab.watch(B, out);
    tokio::time::sleep(Duration::from_secs(2)).await;
    lab.stop();
    let media = lab
        .sent
        .lock()
        .unwrap()
        .iter()
        .filter(|(from, _, _, m)| *from == S && matches!(m.first(), Some(&(T_VIDEO | T_AUDIO | T_FEC))))
        .count();
    assert_eq!(media, 0, "no media leaves without a stream key");
    assert!(aus.lock().unwrap().is_empty());
}

/// A viewer's device key changed without leaving: every viewer gets a new stream key, the changed one
/// sealed to their new device key only.
#[tokio::test(flavor = "multi_thread")]
async fn viewer_device_key_change_rotates_stream_key() {
    use e2ee_lab::*;
    use rynuvo_proto::e2ee::{DeviceKey, frame_generation, open_envelope};
    const B: i64 = 2;
    const C: i64 = 3;
    let lab = Lab::new(&[S, B, C]);
    lab.go_live(synthetic_capture());
    let (b_aus, b_out) = collector();
    let (c_aus, c_out) = collector();
    lab.deliver(S, lab.watch(B, b_out));
    lab.deliver(S, lab.watch(C, c_out));
    eventually("both watch", 10, || b_aus.lock().unwrap().len() >= 5 && c_aus.lock().unwrap().len() >= 5).await;
    let gen0 = frame_generation(&lab.frames_to(B).last().unwrap().1.data).unwrap();

    let new_dev = DeviceKey::generate();
    lab.keys[&S].lock().unwrap().set_peer_pubkey(B, new_dev.public());
    let out = lab.rekey_viewer(B);
    let mut to: Vec<i64> = out.iter().map(|o| o.to).collect();
    to.sort();
    assert_eq!(to, vec![B, C], "every viewer gets the new key");
    assert!(out.iter().all(|o| o.generation == gen0.wrapping_add(1)));
    let s_pub = lab.keys[&S].lock().unwrap().my_public();
    let o = out.iter().find(|o| o.to == B).unwrap();
    assert!(open_envelope(&new_dev, S, &s_pub, B, &Lab::scope(), o.generation, &o.envelope).is_some());
    assert!(lab.rekey_viewer(9).is_empty(), "not a viewer: nothing");
    lab.stop();
}
