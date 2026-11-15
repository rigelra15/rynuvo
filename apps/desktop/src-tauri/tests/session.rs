use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use rynuvo_desktop_lib::core::{
    http::Api,
    identity::{MemoryStore, SecretStore},
    session::{ConnState, Session, SessionEvent, backoff_ms},
};
use rynuvo_proto::WsServerMsg;
use rynuvo_server::testutil::TestApp;

#[test]
fn backoff_caps_at_15s() {
    for attempt in 0..20 {
        let ms = backoff_ms(attempt);
        assert!(ms <= 18_000, "attempt {attempt}: {ms}");
    }
    let first = backoff_ms(0);
    assert!((400..=600).contains(&first), "{first}");
    assert!(backoff_ms(10) >= 12_000);
}

#[tokio::test]
async fn join_stores_token_in_store() {
    let app = TestApp::spawn().await;
    app.set_invite(TestApp::INVITE).await;
    let base = format!("http://{}", app.addr);
    let store = MemoryStore::default();
    let api = Api::new(&base, None).unwrap();
    let resp = api.join(TestApp::INVITE, "Dimas", "test").await.unwrap();
    store.set(&base, &resp.token);
    assert_eq!(store.get(&base).as_deref(), Some(resp.token.as_str()));
    let authed = Api::new(&base, Some(resp.token)).unwrap();
    assert_eq!(authed.me().await.unwrap().nickname, "Dimas");
}

type Events = Arc<Mutex<Vec<SessionEvent>>>;

fn collector() -> (Events, Arc<dyn Fn(SessionEvent) + Send + Sync>) {
    let ev: Events = Arc::default();
    let e2 = ev.clone();
    (ev, Arc::new(move |e| e2.lock().unwrap().push(e)))
}

async fn wait_for(ev: &Events, pred: impl Fn(&[SessionEvent]) -> bool) {
    for _ in 0..100 {
        if pred(&ev.lock().unwrap()) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("condition not met; events: {:?}", ev.lock().unwrap());
}

fn count_created(ev: &[SessionEvent]) -> usize {
    ev.iter().filter(|e| matches!(e, SessionEvent::Server(WsServerMsg::MessageCreated(_)))).count()
}

#[tokio::test]
async fn resume_after_reconnect_fetches_missed_messages() {
    let app = TestApp::spawn().await;
    let a = app.user("Dimas").await;
    let b = app.user("Ayu").await;
    let c = app.create_channel(&a.token, "text", "umum").await;
    let base = format!("http://{}", app.addr);
    let api = Api::new(&base, Some(a.token.clone())).unwrap();
    let store: Arc<dyn SecretStore> = Arc::new(MemoryStore::default());
    store.set(&base, &a.token);
    let (ev, emit) = collector();
    let h = Session::start(api.clone(), store, base.clone(), emit);
    wait_for(&ev, |e| e.iter().any(|x| matches!(x, SessionEvent::Server(WsServerMsg::Ready { .. })))).await;
    h.mark_loaded(c.id, 0);

    let bapi = Api::new(&base, Some(b.token.clone())).unwrap();
    app.state.hub.disconnect_all();
    for i in 0..3 {
        bapi.post_message(c.id, &format!("m{i}"), None).await.unwrap();
    }
    wait_for(&ev, |e| e.iter().filter(|x| matches!(x, SessionEvent::Conn(ConnState::Online))).count() >= 2).await;
    wait_for(&ev, |e| count_created(e) >= 3).await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    let created: Vec<String> = ev
        .lock()
        .unwrap()
        .iter()
        .filter_map(|e| match e {
            SessionEvent::Server(WsServerMsg::MessageCreated(m)) => Some(m.content.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(created, ["m0", "m1", "m2"], "exactly once, in order");
    h.stop();
}

#[tokio::test]
async fn auth_revoked_stops_reconnect() {
    let app = TestApp::spawn().await;
    let admin = app.admin().await;
    let a = app.user("Dimas").await;
    let base = format!("http://{}", app.addr);
    let store: Arc<dyn SecretStore> = Arc::new(MemoryStore::default());
    store.set(&base, &a.token);
    let (ev, emit) = collector();
    let _h = Session::start(Api::new(&base, Some(a.token.clone())).unwrap(), store.clone(), base.clone(), emit);
    wait_for(&ev, |e| e.iter().any(|x| matches!(x, SessionEvent::Conn(ConnState::Online)))).await;
    Api::new(&base, Some(admin.token)).unwrap().admin_kick(a.user.id).await.unwrap();
    wait_for(&ev, |e| e.iter().any(|x| matches!(x, SessionEvent::Revoked))).await;
    let n = ev.lock().unwrap().len();
    tokio::time::sleep(Duration::from_secs(2)).await;
    let after = &ev.lock().unwrap()[n..];
    assert!(after.iter().all(|e| !matches!(e, SessionEvent::Conn(ConnState::Connecting))), "no reconnect: {after:?}");
    assert!(store.get(&base).is_none());
}

/// A server that sends Ready and then goes silent without closing (half-open connection).
async fn silent_server() -> String {
    use futures_util::StreamExt;
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = l.local_addr().unwrap();
    tokio::spawn(async move {
        loop {
            let Ok((s, _)) = l.accept().await else { return };
            tokio::spawn(async move {
                let mut ws = tokio_tungstenite::accept_async(s).await.unwrap();
                let _hello = ws.next().await;
                let ready = r#"{"t":"ready","d":{"me":{"id":1,"nickname":"x","isAdmin":false},"users":[],"channels":[],"online":[],"alerts":[]}}"#;
                use futures_util::SinkExt;
                ws.send(tokio_tungstenite::tungstenite::Message::text(ready)).await.unwrap();
                // Hold the socket open but never read or write again.
                tokio::time::sleep(Duration::from_secs(600)).await;
                drop(ws);
            });
        }
    });
    format!("http://{addr}")
}

#[tokio::test]
async fn silent_server_triggers_reconnect() {
    use rynuvo_desktop_lib::core::session::Timing;
    let base = silent_server().await;
    let store: Arc<dyn SecretStore> = Arc::new(MemoryStore::default());
    let (ev, emit) = collector();
    let timing = Timing { ping_every: Duration::from_millis(200), read_timeout: Duration::from_millis(800) };
    let h = Session::start_with(Api::new(&base, Some("t".into())).unwrap(), store, base.clone(), emit, timing);
    wait_for(&ev, |e| e.iter().any(|x| matches!(x, SessionEvent::Conn(ConnState::Online)))).await;
    wait_for(&ev, |e| e.iter().any(|x| matches!(x, SessionEvent::Conn(ConnState::Reconnecting { .. })))).await;
    h.stop();
}

#[tokio::test]
async fn session_routes_binary_frames() {
    use rynuvo_proto::{
        WsClientMsg,
        voice_wire::{DOWN_KEEPALIVE_ACK, UP_KEEPALIVE, VoiceKey, open_down, seal_up},
    };
    let app = TestApp::spawn().await;
    let a = app.user("Dimas").await;
    let v = app.create_channel(&a.token, "voice", "Mabar").await;
    let base = format!("http://{}", app.addr);
    let store: Arc<dyn SecretStore> = Arc::new(MemoryStore::default());
    let (ev, emit) = collector();
    let h = Session::start(Api::new(&base, Some(a.token.clone())).unwrap(), store, base.clone(), emit);
    wait_for(&ev, |e| e.iter().any(|x| matches!(x, SessionEvent::Conn(ConnState::Online)))).await;
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    h.set_binary_sink(Some(tx));
    h.send(WsClientMsg::VoiceJoin { channel_id: v.id });
    wait_for(&ev, |e| e.iter().any(|x| matches!(x, SessionEvent::Server(WsServerMsg::VoiceJoined { .. })))).await;
    let (session, key) = ev
        .lock()
        .unwrap()
        .iter()
        .find_map(|x| match x {
            SessionEvent::Server(WsServerMsg::VoiceJoined { session, key, .. }) => Some((*session, VoiceKey::from_b64(key).unwrap())),
            _ => None,
        })
        .unwrap();
    h.send_binary(seal_up(&key, UP_KEEPALIVE, session, 1, &[]));
    let ack = tokio::time::timeout(Duration::from_secs(2), rx.recv()).await.unwrap().unwrap();
    assert_eq!(open_down(&key, &ack).unwrap().kind, DOWN_KEEPALIVE_ACK);
    h.stop();
}

/// The server stops reading (dead route after a network switch): once the socket's send buffer fills,
/// a blocked send must not freeze the session until TCP gives up — it reconnects within ~10 s (a slow
/// uplink busy with Go Live video must not count as dead, so the limit is not tighter).
#[tokio::test]
async fn blocked_send_triggers_reconnect() {
    use rynuvo_desktop_lib::core::session::Timing;
    let base = silent_server().await;
    let store: Arc<dyn SecretStore> = Arc::new(MemoryStore::default());
    let (ev, emit) = collector();
    let timing = Timing { ping_every: Duration::from_secs(60), read_timeout: Duration::from_secs(60) };
    let h = Session::start_with(Api::new(&base, Some("t".into())).unwrap(), store, base.clone(), emit, timing);
    wait_for(&ev, |e| e.iter().any(|x| matches!(x, SessionEvent::Conn(ConnState::Online)))).await;
    for _ in 0..400 {
        h.send_binary(vec![0u8; 64 * 1024]);
    }
    let t = std::time::Instant::now();
    while t.elapsed() < Duration::from_secs(12) {
        if ev.lock().unwrap().iter().any(|x| matches!(x, SessionEvent::Conn(ConnState::Reconnecting { .. }))) {
            h.stop();
            return;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("session stuck on a blocked send");
}

#[tokio::test]
async fn reconnect_on_request() {
    let app = TestApp::spawn().await;
    let a = app.user("Dimas").await;
    let base = format!("http://{}", app.addr);
    let (ev, emit) = collector();
    let h = Session::start(Api::new(&base, Some(a.token.clone())).unwrap(), Arc::new(MemoryStore::default()), base.clone(), emit);
    wait_for(&ev, |e| e.iter().any(|x| matches!(x, SessionEvent::Conn(ConnState::Online)))).await;
    h.reconnect();
    wait_for(&ev, |e| e.iter().any(|x| matches!(x, SessionEvent::Conn(ConnState::Reconnecting { .. })))).await;
    h.stop();
}
