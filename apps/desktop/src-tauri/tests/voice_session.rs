//! Voice control over a real session: rejoin after reconnect, stop when the server ends voice.
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use rynuvo_desktop_lib::{
    core::{
        http::Api,
        identity::MemoryStore,
        session::{Session, SessionEvent},
    },
    voice::{
        backend::{AudioBackend, AudioIo},
        ctl::VoiceCtl,
    },
};
use rynuvo_server::testutil::TestApp;

#[derive(Default)]
struct Counts {
    starts: usize,
    stops: usize,
    io: Option<AudioIo>,
}

struct FakeBackend(Arc<Mutex<Counts>>);

impl AudioBackend for FakeBackend {
    fn start(&mut self, _i: Option<&str>, _o: Option<&str>, io: AudioIo) -> Result<(), String> {
        let mut c = self.0.lock().unwrap();
        c.starts += 1;
        c.io = Some(io);
        Ok(())
    }
    fn stop(&mut self) {
        let mut c = self.0.lock().unwrap();
        if c.io.take().is_some() {
            c.stops += 1;
        }
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

struct Rig {
    app: TestApp,
    ctl: VoiceCtl,
    counts: Arc<Mutex<Counts>>,
    user: i64,
    token: String,
    channel: i64,
}

async fn rig() -> Rig {
    let app = TestApp::spawn().await;
    let u = app.user("Dimas").await;
    let ch = app.create_channel(&u.token, "voice", "Mabar").await.id;
    rig_on(app, u, ch).await
}

async fn rig_on(app: TestApp, u: rynuvo_proto::JoinResp, ch: i64) -> Rig {
    let base = format!("http://{}", app.addr);
    let counts = Arc::new(Mutex::new(Counts::default()));
    let c2 = counts.clone();
    let ctl = VoiceCtl::new(Arc::new(move || Box::new(FakeBackend(c2.clone())) as Box<dyn AudioBackend>), Arc::new(|_| {}));
    let ctl2 = ctl.clone();
    let emit = Arc::new(move |e: SessionEvent| {
        if let SessionEvent::Server(m) = &e {
            ctl2.on_server(m);
        }
    });
    let api = Api::new(&base, Some(u.token.clone())).unwrap();
    let s = Session::start(api, Arc::new(MemoryStore::default()), base, emit);
    ctl.attach(s, "127.0.0.1".into());
    let uid = u.user.id;
    eventually("online", || app.state.hub.online_ids().contains(&uid)).await;
    Rig { app, ctl, counts, user: uid, token: u.token, channel: ch }
}

/// A second client on the same server.
async fn second(app: &TestApp, nick: &str, ch: i64) -> (VoiceCtl, Arc<Mutex<Counts>>, i64) {
    let u = app.user(nick).await;
    let base = format!("http://{}", app.addr);
    let counts = Arc::new(Mutex::new(Counts::default()));
    let c2 = counts.clone();
    let ctl = VoiceCtl::new(Arc::new(move || Box::new(FakeBackend(c2.clone())) as Box<dyn AudioBackend>), Arc::new(|_| {}));
    let ctl2 = ctl.clone();
    let emit = Arc::new(move |e: SessionEvent| {
        if let SessionEvent::Server(m) = &e {
            ctl2.on_server(m);
        }
    });
    let s = Session::start(Api::new(&base, Some(u.token.clone())).unwrap(), Arc::new(MemoryStore::default()), base, emit);
    ctl.attach(s, "127.0.0.1".into());
    let uid = u.user.id;
    eventually("second online", || app.state.hub.online_ids().contains(&uid)).await;
    ctl.join(ch);
    eventually("second engine", || counts.lock().unwrap().starts == 1).await;
    (ctl, counts, uid)
}

#[tokio::test(flavor = "multi_thread")]
async fn two_clients_hear_each_other() {
    use ringbuf::traits::{Consumer, Producer};
    let r = rig().await;
    r.ctl.set_threshold(0.0);
    r.ctl.join(r.channel);
    eventually("engine started", || r.counts.lock().unwrap().starts == 1).await;
    let (b, bcounts, _) = second(&r.app, "Raka", r.channel).await;

    // Let B's own join chime finish before listening for A.
    let mut buf = [0f32; 960];
    for _ in 0..60 {
        if let Some(io) = bcounts.lock().unwrap().io.as_mut() {
            io.playback.pop_slice(&mut buf);
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    let tone: Vec<f32> = (0..480).map(|i| 0.4 * (2.0 * std::f32::consts::PI * 300.0 * i as f32 / 48_000.0).sin()).collect();
    let mut peak = 0f32;
    for _ in 0..300 {
        if let Some(io) = r.counts.lock().unwrap().io.as_mut() {
            io.capture.push_slice(&tone);
        }
        if let Some(io) = bcounts.lock().unwrap().io.as_mut() {
            let n = io.playback.pop_slice(&mut buf);
            peak = buf[..n].iter().fold(peak, |m, x| m.max(x.abs()));
        }
        if peak > 0.05 {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert!(peak > 0.05, "B hears A (peak {peak})");
    b.leave();
    r.ctl.leave();
}

#[tokio::test(flavor = "multi_thread")]
async fn rejoins_voice_after_reconnect() {
    let r = rig().await;
    r.ctl.join(r.channel);
    eventually("in voice", || r.app.state.voice.member(r.user).is_some()).await;
    eventually("engine started", || r.counts.lock().unwrap().starts == 1).await;
    r.ctl.set_deafened(true);
    eventually("deafened on server", || r.app.state.voice.member(r.user).is_some_and(|m| m.deafened && m.muted)).await;
    let old_conn = r.app.state.voice.member(r.user).unwrap().conn_id;

    r.app.state.hub.disconnect_all();
    eventually("rejoined on the new connection", || {
        r.app.state.voice.member(r.user).is_some_and(|m| m.conn_id != old_conn && m.channel_id == r.channel)
    })
    .await;
    eventually("deafen kept", || r.app.state.voice.member(r.user).is_some_and(|m| m.deafened && m.muted)).await;
    assert_eq!(r.ctl.channel(), Some(r.channel));
    r.ctl.leave();
}

#[tokio::test(flavor = "multi_thread")]
async fn voice_ended_stops_engine() {
    let r = rig().await;
    r.ctl.join(r.channel);
    eventually("engine started", || r.counts.lock().unwrap().starts == 1).await;
    let res = r
        .app
        .client
        .delete(r.app.url(&format!("/api/v1/channels/{}", r.channel)))
        .bearer_auth(&r.token)
        .send()
        .await
        .unwrap();
    assert!(res.status().is_success(), "{}", res.status());
    eventually("engine stopped", || r.counts.lock().unwrap().stops >= 1).await;
    assert_eq!(r.ctl.channel(), None);
}

#[tokio::test(flavor = "multi_thread")]
async fn leave_stops_engine_and_server_membership() {
    let r = rig().await;
    r.ctl.join(r.channel);
    eventually("engine started", || r.counts.lock().unwrap().starts == 1).await;
    r.ctl.leave();
    eventually("left on server", || r.app.state.voice.member(r.user).is_none()).await;
    assert!(r.counts.lock().unwrap().stops >= 1);
}
