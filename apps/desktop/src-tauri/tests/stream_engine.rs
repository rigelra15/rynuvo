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
    Arc::new(|_source: &str, opts: CaptureOpts, video, _audio| {
        let stop = Arc::new(AtomicBool::new(false));
        let s2 = stop.clone();
        let (w, h) = (320u32.min(opts.max_width), 240u32.min(opts.max_height));
        std::thread::spawn(move || {
            let mut t = 0u8;
            while !s2.load(Ordering::Relaxed) {
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
    let u = app.user(nick).await;
    let base = format!("http://{}", app.addr);
    let voice = VoiceCtl::new(Arc::new(|| Box::new(NullBackend(None)) as Box<dyn AudioBackend>), Arc::new(|_| {}));
    let events = Arc::new(Mutex::new(Vec::new()));
    let ev2 = events.clone();
    let stream = StreamCtl::new(voice.clone(), synthetic(), Arc::new(move |e| ev2.lock().unwrap().push(e)), tokio::runtime::Handle::current());
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
    a.stream.go_live("synthetic:0", "720p30").unwrap();
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
async fn viewer_join_gets_keyframe() {
    let app = TestApp::spawn().await;
    let u = app.user("Owner").await;
    let ch = app.create_channel(&u.token, "voice", "Mabar").await.id;
    let a = peer(&app, "Dimas", ch).await;
    let b = peer(&app, "Ayu", ch).await;
    let c = peer(&app, "Raka", ch).await;
    a.stream.go_live("synthetic:0", "720p30").unwrap();
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
    a.stream.go_live("synthetic:0", "720p30").unwrap();
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
    let err = a.stream.go_live("synthetic:0", "1080p60").unwrap_err();
    assert_eq!(err, "Join a voice channel first");
}
