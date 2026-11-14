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
