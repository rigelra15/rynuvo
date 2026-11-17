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
    let emit = Arc::new(move |e: SessionEvent| ctl2.on_session(&e));
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

/// A stale "you left" for the channel we are (re)joining must not cancel the join.
#[tokio::test(flavor = "multi_thread")]
async fn stale_self_leave_during_join_is_ignored() {
    let r = rig().await;
    r.ctl.join(r.channel);
    r.ctl.on_server(&rynuvo_proto::WsServerMsg::VoiceMemberLeft { user_id: r.user, channel_id: r.channel });
    eventually("engine started despite stale leave", || r.counts.lock().unwrap().starts == 1).await;
    assert_eq!(r.ctl.state().channel_id, Some(r.channel));
}

/// Removed from the circle (WebSocket closed with "revoked"): voice must stop, not keep the mic open.
#[tokio::test(flavor = "multi_thread")]
async fn removed_from_circle_stops_voice() {
    let app = TestApp::spawn().await;
    let admin = app.admin().await;
    let u = app.user("Dimas").await;
    let ch = app.create_channel(&u.token, "voice", "Mabar").await.id;
    let uid = u.user.id;
    let r = rig_on(app, u, ch).await;
    r.ctl.join(r.channel);
    eventually("engine started", || r.counts.lock().unwrap().starts == 1).await;
    let api = Api::new(&format!("http://{}", r.app.addr), Some(admin.token.clone())).unwrap();
    api.admin_kick(uid).await.unwrap();
    eventually("engine stopped", || r.counts.lock().unwrap().stops >= 1).await;
    assert_eq!(r.ctl.state().channel_id, None);
}

/// A device that takes seconds to open (Bluetooth, a stuck WASAPI endpoint) must not block leaving.
struct SlowBackend(Arc<Mutex<Counts>>);

impl AudioBackend for SlowBackend {
    fn start(&mut self, _i: Option<&str>, _o: Option<&str>, io: AudioIo) -> Result<(), String> {
        self.0.lock().unwrap().starts += 1;
        std::thread::sleep(Duration::from_secs(3));
        self.0.lock().unwrap().io = Some(io);
        Ok(())
    }
    fn stop(&mut self) {}
}

#[tokio::test(flavor = "multi_thread")]
async fn leave_does_not_wait_for_slow_device() {
    let app = TestApp::spawn().await;
    let u = app.user("Dimas").await;
    let ch = app.create_channel(&u.token, "voice", "Mabar").await.id;
    let base = format!("http://{}", app.addr);
    let counts = Arc::new(Mutex::new(Counts::default()));
    let c2 = counts.clone();
    let ctl = VoiceCtl::new(Arc::new(move || Box::new(SlowBackend(c2.clone())) as Box<dyn AudioBackend>), Arc::new(|_| {}));
    let ctl2 = ctl.clone();
    let s = Session::start(Api::new(&base, Some(u.token.clone())).unwrap(), Arc::new(MemoryStore::default()), base, Arc::new(move |e: SessionEvent| ctl2.on_session(&e)));
    ctl.attach(s, "127.0.0.1".into());
    let uid = u.user.id;
    eventually("online", || app.state.hub.online_ids().contains(&uid)).await;
    ctl.join(ch);
    eventually("device opening", || counts.lock().unwrap().starts == 1).await;
    let t = std::time::Instant::now();
    ctl.leave();
    assert!(t.elapsed() < Duration::from_millis(500), "leave blocked for {:?}", t.elapsed());
}

// ---- End-to-end encryption ----

use base64::{Engine, engine::general_purpose::STANDARD as B64};
use rynuvo_desktop_lib::{
    core::identity::SecretStore,
    e2ee::{KeyState, device::load_or_create},
};
use rynuvo_proto::{
    WsServerMsg,
    e2ee::{DeviceKey, FRAME_OVERHEAD, KIND_VOICE, Scope, open_envelope, seal_envelope, MediaKey},
};

/// What a client saw on its WebSocket that matters for key handling.
#[derive(Debug, Clone, PartialEq)]
enum Seen {
    Key { from: i64, generation: u32 },
    Left(i64),
}

type Log = Arc<Mutex<Vec<Seen>>>;

/// A voice client whose incoming key messages are logged. `device` pins the device key.
struct Tapped {
    ctl: VoiceCtl,
    counts: Arc<Mutex<Counts>>,
    log: Log,
}

async fn tapped_as(app: &TestApp, token: &str, uid: i64, device: Option<DeviceKey>) -> Tapped {
    let base = format!("http://{}", app.addr);
    let counts = Arc::new(Mutex::new(Counts::default()));
    let c2 = counts.clone();
    let ctl = VoiceCtl::new(Arc::new(move || Box::new(FakeBackend(c2.clone())) as Box<dyn AudioBackend>), Arc::new(|_| {}));
    if let Some(d) = device {
        ctl.set_device(d);
    }
    let log: Log = Arc::default();
    let (ctl2, log2) = (ctl.clone(), log.clone());
    let emit = Arc::new(move |e: SessionEvent| {
        if let SessionEvent::Server(m) = &e {
            match m {
                WsServerMsg::E2eeKey { from, generation, .. } => {
                    log2.lock().unwrap().push(Seen::Key { from: *from, generation: *generation })
                }
                WsServerMsg::VoiceMemberLeft { user_id, .. } => log2.lock().unwrap().push(Seen::Left(*user_id)),
                _ => {}
            }
        }
        ctl2.on_session(&e);
    });
    let s = Session::start(Api::new(&base, Some(token.to_string())).unwrap(), Arc::new(MemoryStore::default()), base, emit);
    ctl.attach(s, "127.0.0.1".into());
    eventually("tapped online", || app.state.hub.online_ids().contains(&uid)).await;
    Tapped { ctl, counts, log }
}

/// A new user, in voice channel `ch`, with its key messages logged.
async fn tapped(app: &TestApp, nick: &str, ch: i64) -> (Tapped, i64) {
    let u = app.user(nick).await;
    let t = tapped_as(app, &u.token, u.user.id, None).await;
    t.ctl.join(ch);
    eventually("tapped engine", || t.counts.lock().unwrap().starts == 1).await;
    (t, u.user.id)
}

fn keys_from(log: &Log, from: i64) -> Vec<u32> {
    log.lock()
        .unwrap()
        .iter()
        .filter_map(|s| match s {
            Seen::Key { from: f, generation } if *f == from => Some(*generation),
            _ => None,
        })
        .collect()
}

/// Drain B's speaker and report the loudest sample heard.
fn heard(c: &Arc<Mutex<Counts>>) -> f32 {
    use ringbuf::traits::Consumer;
    let mut buf = [0f32; 960];
    let mut peak = 0f32;
    if let Some(io) = c.lock().unwrap().io.as_mut() {
        loop {
            let n = io.playback.pop_slice(&mut buf);
            if n == 0 {
                break;
            }
            peak = buf[..n].iter().fold(peak, |m, x| m.max(x.abs()));
        }
    }
    peak
}

/// What the relay forwards is ciphertext: a third member's raw link (no e2ee) sees frames that only open
/// with A's media key, and never contain A's Opus bytes; B, with the key, hears A.
#[tokio::test(flavor = "multi_thread")]
async fn audio_is_end_to_end_encrypted() {
    use ringbuf::traits::Producer;
    use rynuvo_desktop_lib::voice::link::{LinkCfg, VoiceLink, WsSender};
    use rynuvo_proto::voice_wire::VoiceKey;
    use serde_json::json;
    use tokio::sync::mpsc;

    let app = TestApp::spawn().await;
    let owner = app.user("Dimas").await;
    let ch = app.create_channel(&owner.token, "voice", "Mabar").await.id;
    let a = tapped_as(&app, &owner.token, owner.user.id, None).await;
    let a_id = owner.user.id;
    a.ctl.set_threshold(0.0);
    a.ctl.join(ch);
    eventually("A engine", || a.counts.lock().unwrap().starts == 1).await;

    // The observer: a plain WebSocket + link that never decrypts media.
    let obs = app.user("Intip").await;
    let obs_dev = DeviceKey::generate();
    let mut ws = app.ws(&obs.token).await;
    let a_pub = loop {
        match ws.next().await {
            WsServerMsg::Ready { voice, .. } => {
                break voice.into_iter().find(|m| m.user_id == a_id).expect("A in voice").pubkey;
            }
            _ => continue,
        }
    };
    ws.send_json(json!({"t":"voiceJoin","d":{"channelId":ch,"pubkey":obs_dev.public().to_b64()}})).await;
    let (session, key, udp_port) = loop {
        if let WsServerMsg::VoiceJoined { session, key, udp_port, .. } = ws.next().await {
            break (session, VoiceKey::from_b64(&key).unwrap(), udp_port);
        }
    };
    let swallow: WsSender = Arc::new(|_| {});
    let (down_tx, mut down) = mpsc::unbounded_channel();
    let cfg = LinkCfg { host: "127.0.0.1".into(), udp_port, session, key, keys: None, channel: ch, unencrypted_for_tests: true };
    let link = VoiceLink::start(cfg, swallow, down_tx);
    let (env_tx, mut env_rx) = mpsc::unbounded_channel();
    tokio::spawn(async move {
        while let Some(m) = ws.try_next(30_000).await {
            if let WsServerMsg::E2eeKey { from, scope, generation, envelope } = m {
                let _ = env_tx.send((from, scope, generation, envelope));
            }
        }
    });

    let (b, _) = tapped(&app, "Raka", ch).await;
    tokio::time::sleep(Duration::from_millis(600)).await; // B's join chime
    heard(&b.counts);

    let mut obs_keys = KeyState::new(obs.user.id, obs_dev);
    obs_keys.set_peer_pubkey(a_id, rynuvo_proto::e2ee::PublicKey::from_b64(&a_pub).unwrap());
    let scope = Scope::Voice(ch);
    let tone: Vec<f32> = (0..480).map(|i| 0.4 * (2.0 * std::f32::consts::PI * 300.0 * i as f32 / 48_000.0).sin()).collect();
    let (mut peak, mut opened) = (0f32, 0usize);
    for _ in 0..400 {
        if let Some(io) = a.counts.lock().unwrap().io.as_mut() {
            io.capture.push_slice(&tone);
        }
        peak = peak.max(heard(&b.counts));
        while let Ok((from, sc, g, env)) = env_rx.try_recv() {
            obs_keys.on_envelope(from, &sc, g, &B64.decode(env).unwrap(), std::time::Instant::now());
        }
        while let Ok(d) = down.try_recv() {
            assert_eq!(d.from, a_id);
            let ct = d.payload;
            if let Some(opus) = obs_keys.open(&scope, a_id, KIND_VOICE, &ct, std::time::Instant::now()) {
                assert_eq!(ct.len(), opus.len() + FRAME_OVERHEAD);
                assert!(!opus.is_empty());
                assert!(!ct.windows(opus.len()).any(|w| w == opus.as_slice()), "Opus bytes visible on the relay");
                opened += 1;
            }
        }
        if peak > 0.05 && opened >= 5 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(opened >= 5, "relayed frames are A's sealed Opus (opened {opened})");
    assert!(peak > 0.05, "B decrypts and hears A (peak {peak})");
    link.stop();
    b.ctl.leave();
    a.ctl.leave();
}

/// C leaves: A and B each send the other a new key (generation + 1); C gets nothing.
#[tokio::test(flavor = "multi_thread")]
async fn leave_rotates_keys() {
    let app = TestApp::spawn().await;
    let owner = app.user("Dimas").await;
    let ch = app.create_channel(&owner.token, "voice", "Mabar").await.id;
    let (a, a_id) = (tapped_as(&app, &owner.token, owner.user.id, None).await, owner.user.id);
    a.ctl.join(ch);
    eventually("A engine", || a.counts.lock().unwrap().starts == 1).await;
    let (b, b_id) = tapped(&app, "Raka", ch).await;
    let (c, c_id) = tapped(&app, "Ayu", ch).await;
    eventually("A has B's and C's keys", || !keys_from(&a.log, b_id).is_empty() && !keys_from(&a.log, c_id).is_empty()).await;
    eventually("B has A's and C's keys", || !keys_from(&b.log, a_id).is_empty() && !keys_from(&b.log, c_id).is_empty()).await;
    let g_a = keys_from(&b.log, a_id)[0];
    let g_b = keys_from(&a.log, b_id)[0];
    assert!(keys_from(&b.log, a_id).iter().all(|g| *g == g_a), "no rotation on join");

    let c_before = c.log.lock().unwrap().len();
    c.ctl.leave();
    eventually("A rotated to B", || keys_from(&b.log, a_id).contains(&g_a.wrapping_add(1))).await;
    eventually("B rotated to A", || keys_from(&a.log, b_id).contains(&g_b.wrapping_add(1))).await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    let c_after: Vec<Seen> = c.log.lock().unwrap()[c_before..].to_vec();
    assert!(!c_after.iter().any(|s| matches!(s, Seen::Key { .. })), "the leaver gets no keys: {c_after:?}");
    a.ctl.leave();
    b.ctl.leave();
}

/// A comes back on a new connection with the same device key: not a leave, so B keeps its key and
/// answers A's new session with it.
#[tokio::test(flavor = "multi_thread")]
async fn rejoin_same_device_keeps_peers_quiet() {
    let app = TestApp::spawn().await;
    let owner = app.user("Dimas").await;
    let ch = app.create_channel(&owner.token, "voice", "Mabar").await.id;
    let a_id = owner.user.id;
    let dev = DeviceKey::generate();
    let a1 = tapped_as(&app, &owner.token, a_id, Some(dev.clone())).await;
    a1.ctl.join(ch);
    eventually("A engine", || a1.counts.lock().unwrap().starts == 1).await;
    assert_eq!(app.state.voice.member(a_id).unwrap().pubkey, dev.public().to_b64());
    let (b, b_id) = tapped(&app, "Raka", ch).await;
    eventually("A has B's key", || !keys_from(&a1.log, b_id).is_empty()).await;
    eventually("B has A's key", || !keys_from(&b.log, a_id).is_empty()).await;
    let g_b = keys_from(&a1.log, b_id)[0];
    let old_conn = app.state.voice.member(a_id).unwrap().conn_id;

    let a2 = tapped_as(&app, &owner.token, a_id, Some(dev.clone())).await;
    a2.ctl.join(ch);
    eventually("A back on the new connection", || app.state.voice.member(a_id).is_some_and(|m| m.conn_id != old_conn)).await;
    assert_eq!(app.state.voice.member(a_id).unwrap().pubkey, dev.public().to_b64(), "same device key on the server");
    eventually("A's new session gets B's key", || keys_from(&a2.log, b_id).contains(&g_b)).await;
    tokio::time::sleep(Duration::from_millis(1500)).await;
    assert!(!b.log.lock().unwrap().contains(&Seen::Left(a_id)), "B saw no leave");
    let from_b: Vec<u32> = keys_from(&a1.log, b_id).into_iter().chain(keys_from(&a2.log, b_id)).collect();
    assert!(from_b.iter().all(|g| *g == g_b), "B did not rotate: {from_b:?} (was {g_b})");
    a2.ctl.leave();
    b.ctl.leave();
}

/// A keychain that loses writes.
#[derive(Default)]
struct ForgetfulStore(MemoryStore);

impl SecretStore for ForgetfulStore {
    fn get(&self, server: &str) -> Option<String> {
        self.0.get(server)
    }
    fn set(&self, server: &str, token: &str) {
        self.0.set(server, token)
    }
    fn clear(&self, server: &str) {
        self.0.clear(server)
    }
    fn set_e2ee(&self, _server: &str, _b64: &str) {}
}

#[test]
fn memory_key_when_keyring_fails() {
    let server = "http://rynuvo.test";
    let kept = MemoryStore::default();
    assert_eq!(load_or_create(&kept, server).public(), load_or_create(&kept, server).public(), "a working keychain keeps the key");
    assert!(kept.get_e2ee(server).unwrap().is_some());
    assert!(kept.get(server).is_none(), "the device key does not take the token's entry");

    let store = ForgetfulStore::default();
    let k1 = load_or_create(&store, server);
    let k2 = load_or_create(&store, server);
    assert_ne!(k1.public(), k2.public(), "nothing stored: a fresh key each run");
    // Both work: an envelope from one opens with the other.
    let scope = Scope::Voice(1);
    let mk = MediaKey::random();
    let env = seal_envelope(&k1, 1, 2, &k2.public(), &scope, 7, &mk).expect("seal");
    assert_eq!(open_envelope(&k2, 1, &k1.public(), 2, &scope, 7, &env), Some(mk));
}

/// The server swaps B's announced device key without a leave (e.g. after first announcing an attacker
/// key): A must treat it as a new device. The key A's crypto uses and the code A shows both follow the
/// new key, A rotates its call key to the others, and nothing new is sealed to the old key.
#[tokio::test(flavor = "multi_thread")]
async fn reannounced_pubkey_replaces_the_key_in_use() {
    use rynuvo_proto::{VoiceMember, e2ee::PublicKey};
    let app = TestApp::spawn().await;
    let owner = app.user("Dimas").await;
    let ch = app.create_channel(&owner.token, "voice", "Mabar").await.id;
    let (a, a_id) = (tapped_as(&app, &owner.token, owner.user.id, None).await, owner.user.id);
    a.ctl.join(ch);
    eventually("A engine", || a.counts.lock().unwrap().starts == 1).await;
    let b_dev = DeviceKey::generate();
    let bu = app.user("Raka").await;
    let b = tapped_as(&app, &bu.token, bu.user.id, Some(b_dev.clone())).await;
    let b_id = bu.user.id;
    b.ctl.join(ch);
    eventually("B engine", || b.counts.lock().unwrap().starts == 1).await;
    let (c, c_id) = tapped(&app, "Ayu", ch).await;
    eventually("B and C have A's key", || !keys_from(&b.log, a_id).is_empty() && !keys_from(&c.log, a_id).is_empty()).await;
    let keys = a.ctl.e2ee_keys().unwrap();
    eventually("A uses B's real key", || keys.lock().unwrap().peer_pubkey(b_id) == Some(b_dev.public())).await;
    let g_a = keys_from(&c.log, a_id)[0];

    let new_dev = DeviceKey::generate();
    let m = VoiceMember { user_id: b_id, channel_id: ch, muted: false, deafened: false, joined_at: 0, call_since: 0, pubkey: new_dev.public().to_b64() };
    a.ctl.on_server(&WsServerMsg::VoiceMemberUpdated(m));

    assert_eq!(keys.lock().unwrap().peer_pubkey(b_id), Some(new_dev.public()), "crypto follows the announced key");
    let ck = a.ctl.call_keys().expect("in a call");
    assert_eq!(ck.others.iter().find(|(u, _)| *u == b_id).map(|(_, k)| k.clone()), Some(new_dev.public()), "the code covers the key in use");
    let scope = Scope::Voice(ch);
    let out = keys.lock().unwrap().share_current(&scope, &[b_id], std::time::Instant::now());
    let a_pub: PublicKey = keys.lock().unwrap().my_public();
    for o in &out {
        assert!(open_envelope(&new_dev, a_id, &a_pub, b_id, &scope, o.generation, &o.envelope).is_some(), "sealed to the new key");
        assert!(open_envelope(&b_dev, a_id, &a_pub, b_id, &scope, o.generation, &o.envelope).is_none(), "not to the old one");
    }
    eventually("A rotated its key to C", || keys_from(&c.log, a_id).contains(&g_a.wrapping_add(1))).await;
    let _ = c_id;
    a.ctl.leave();
    b.ctl.leave();
    c.ctl.leave();
}
