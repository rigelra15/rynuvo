//! Direct (P2P) path between a streamer and a viewer, with relay fallback.
use std::{
    net::SocketAddr,
    sync::{Arc, Mutex},
    time::Duration,
};

use rynuvo_desktop_lib::stream::peer::{PeerLink, PeerPath, Role, order_candidates};
use rynuvo_proto::voice_wire::VoiceKey;
use tokio::sync::mpsc;

struct Side {
    link: PeerLink,
    rx: mpsc::UnboundedReceiver<Vec<u8>>,
    relayed: Arc<Mutex<Vec<Vec<u8>>>>,
    signals: mpsc::UnboundedReceiver<serde_json::Value>,
}

fn side(role: Role) -> Side {
    let (sig_tx, signals) = mpsc::unbounded_channel();
    let relayed = Arc::new(Mutex::new(Vec::new()));
    let r2 = relayed.clone();
    let (tx, rx) = mpsc::unbounded_channel();
    // No STUN server in tests: the hint only picks the local interface (loopback).
    let hint: Vec<SocketAddr> = vec!["127.0.0.1:9".parse().unwrap()];
    let link = PeerLink::start(
        role,
        VoiceKey([5; 32]),
        hint,
        Arc::new(move |v| {
            let _ = sig_tx.send(v);
        }),
        Arc::new(move |b| r2.lock().unwrap().push(b)),
        tx,
    );
    Side { link, rx, relayed, signals }
}

async fn wait_path(s: &Side, want: impl Fn(PeerPath) -> bool, ms: u64) -> PeerPath {
    let t = std::time::Instant::now();
    loop {
        let p = s.link.path();
        if want(p) || t.elapsed() > Duration::from_millis(ms) {
            return p;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

#[tokio::test]
async fn direct_path_on_loopback() {
    let mut a = side(Role::Streamer);
    let mut b = side(Role::Viewer);
    let sa = tokio::time::timeout(Duration::from_secs(5), a.signals.recv()).await.unwrap().unwrap();
    let sb = tokio::time::timeout(Duration::from_secs(5), b.signals.recv()).await.unwrap().unwrap();
    a.link.on_signal(&sb);
    b.link.on_signal(&sa);
    let p = wait_path(&a, |p| matches!(p, PeerPath::Direct(_)), 3000).await;
    assert!(matches!(p, PeerPath::Direct(_)), "streamer path: {p:?}");
    assert!(matches!(wait_path(&b, |p| matches!(p, PeerPath::Direct(_)), 3000).await, PeerPath::Direct(_)));
    a.relayed.lock().unwrap().clear();
    a.link.send(b"frame-1");
    let got = tokio::time::timeout(Duration::from_secs(2), b.rx.recv()).await.unwrap().unwrap();
    assert_eq!(got, b"frame-1");
    b.link.send(b"report");
    assert_eq!(tokio::time::timeout(Duration::from_secs(2), a.rx.recv()).await.unwrap().unwrap(), b"report");
    assert!(a.relayed.lock().unwrap().is_empty(), "direct traffic must not use the relay");
    a.link.stop();
    b.link.stop();
}

#[tokio::test]
async fn falls_back_to_relay_when_unreachable() {
    let mut a = side(Role::Streamer);
    let _ = tokio::time::timeout(Duration::from_secs(5), a.signals.recv()).await;
    // The viewer's candidates point nowhere.
    a.link.on_signal(&serde_json::json!({"cands": ["127.0.0.1:9"]}));
    a.link.send(b"early"); // before a direct path exists, data goes through the relay
    assert_eq!(a.relayed.lock().unwrap().len(), 1);
    let p = wait_path(&a, |p| p == PeerPath::Relay, 4500).await;
    assert_eq!(p, PeerPath::Relay);
    a.link.send(b"later");
    assert_eq!(a.relayed.lock().unwrap().len(), 2);
    // Relay data from the other side is passed through.
    a.link.on_relay(b"via-server");
    assert_eq!(a.rx.recv().await.unwrap(), b"via-server");
    a.link.stop();
}

#[test]
fn ipv6_preferred_in_ordering() {
    let c: Vec<SocketAddr> = vec!["1.2.3.4:5".parse().unwrap(), "[2001:db8::1]:6".parse().unwrap(), "10.0.0.2:7".parse().unwrap()];
    let o = order_candidates(c);
    assert!(o[0].is_ipv6());
    assert_eq!(o.len(), 3);
}
