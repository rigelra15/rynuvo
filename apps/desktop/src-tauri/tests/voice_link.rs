use std::{sync::Arc, time::Duration};

use rynuvo_desktop_lib::voice::link::{DownAudio, LinkCfg, LinkPath, VoiceLink, WsSender};
use rynuvo_proto::{WsServerMsg, voice_wire::VoiceKey};
use rynuvo_server::testutil::{TestApp, WsClient};
use serde_json::json;
use tokio::sync::mpsc;

struct Client {
    link: VoiceLink,
    json: mpsc::UnboundedSender<serde_json::Value>,
    down: mpsc::UnboundedReceiver<DownAudio>,
    user: i64,
}

async fn until<T>(ws: &mut WsClient, mut f: impl FnMut(WsServerMsg) -> Option<T>) -> T {
    loop {
        if let Some(t) = f(ws.try_next(3000).await.expect("message")) {
            return t;
        }
    }
}

/// Join voice over a test WebSocket and start a link; the WebSocket keeps carrying binary frames for the link.
async fn client(app: &TestApp, nick: &str, ch: Option<i64>, udp_port_override: Option<u16>) -> (Client, i64) {
    let u = app.user(nick).await;
    let ch = match ch {
        Some(c) => c,
        None => app.create_channel(&u.token, "voice", "Mabar").await.id,
    };
    let mut ws = app.ws(&u.token).await;
    until(&mut ws, |m| matches!(m, WsServerMsg::Ready { .. }).then_some(())).await;
    ws.send_json(json!({"t":"voiceJoin","d":{"channelId":ch,"pubkey":rynuvo_proto::e2ee::DeviceKey::generate().public().to_b64()}})).await;
    let (session, key, udp_port) = until(&mut ws, |m| match m {
        WsServerMsg::VoiceJoined { session, key, udp_port, .. } => Some((session, VoiceKey::from_b64(&key).unwrap(), udp_port)),
        _ => None,
    })
    .await;
    let (ws_tx, mut ws_rx) = mpsc::unbounded_channel::<Vec<u8>>();
    let sender: WsSender = Arc::new(move |b| {
        let _ = ws_tx.send(b);
    });
    let (down_tx, down) = mpsc::unbounded_channel();
    let cfg = LinkCfg { host: "127.0.0.1".into(), udp_port: udp_port_override.unwrap_or(udp_port), session, key, keys: None, channel: 0 };
    let link = VoiceLink::start(cfg, sender, down_tx);
    let l2 = link.clone();
    let (json_tx, mut json_rx) = mpsc::unbounded_channel::<serde_json::Value>();
    tokio::spawn(async move {
        loop {
            while let Ok(j) = json_rx.try_recv() {
                ws.send_json(j).await;
            }
            while let Ok(b) = ws_rx.try_recv() {
                ws.send_binary(b).await;
            }
            if let Some(b) = ws.next_binary(20).await {
                l2.feed_ws(&b);
            }
        }
    });
    (Client { link, json: json_tx, down, user: u.user.id }, ch)
}

async fn deliver(from: &Client, to: &mut Client, payload: &[u8]) {
    for _ in 0..40 {
        from.link.send_audio(payload.to_vec());
        if let Ok(Some(d)) = tokio::time::timeout(Duration::from_millis(100), to.down.recv()).await {
            assert_eq!(d.from, from.user);
            assert_eq!(d.payload, payload);
            return;
        }
    }
    panic!("audio never arrived");
}

#[tokio::test]
async fn two_links_exchange_audio_over_udp() {
    let app = TestApp::spawn().await;
    let (mut a, ch) = client(&app, "Dimas", None, None).await;
    let (mut b, _) = client(&app, "Ayu", Some(ch), None).await;
    deliver(&a, &mut b, b"hello").await;
    deliver(&b, &mut a, b"hi back").await;
    assert_eq!(a.link.path(), LinkPath::Udp);
    a.link.stop();
    b.link.stop();
}

#[tokio::test]
async fn falls_back_to_ws_when_udp_silent() {
    let app = TestApp::spawn().await;
    // A UDP port that swallows everything: the relay is unreachable over UDP for A.
    let black_hole = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let dead_port = black_hole.local_addr().unwrap().port();
    let (mut a, ch) = client(&app, "Dimas", None, Some(dead_port)).await;
    let (mut b, _) = client(&app, "Ayu", Some(ch), None).await;
    let mut switched = false;
    for _ in 0..40 {
        if a.link.path() == LinkPath::Ws {
            switched = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert!(switched, "switched to the WebSocket path within 4 s");
    deliver(&a, &mut b, b"via ws").await;
    deliver(&b, &mut a, b"to ws").await;
    drop(black_hole);
}

/// Two people talk at once: each speaker's frames must reach the listener numbered 1, 2, 3…
/// with no gaps from the other speaker's packets or from keepalive acks.
#[tokio::test]
async fn audio_seq_is_per_speaker() {
    let app = TestApp::spawn().await;
    let (a, ch) = client(&app, "Dimas", None, None).await;
    let (b, _) = client(&app, "Ayu", Some(ch), None).await;
    let (mut c, _) = client(&app, "Raka", Some(ch), None).await;
    // Wait until all three routes are known to the relay.
    deliver(&a, &mut c, b"warm").await;
    deliver(&b, &mut c, b"warm").await;
    while c.down.try_recv().is_ok() {}
    tokio::time::sleep(Duration::from_millis(1200)).await; // let a keepalive ack go by
    for _ in 0..10 {
        a.link.send_audio(b"a".to_vec());
        b.link.send_audio(b"b".to_vec());
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    tokio::time::sleep(Duration::from_millis(300)).await;
    let mut seqs: std::collections::HashMap<i64, Vec<u32>> = Default::default();
    while let Ok(d) = c.down.try_recv() {
        seqs.entry(d.from).or_default().push(d.seq);
    }
    for (from, s) in seqs {
        assert_eq!(s.len(), 10, "all frames from {from} arrived");
        let steps: Vec<u32> = s.windows(2).map(|w| w[1].wrapping_sub(w[0])).collect();
        assert!(steps.iter().all(|&d| d == 1), "speaker {from} seqs have gaps: {s:?}");
    }
}

/// Both paths dead (network switched): the link rebinds its UDP socket and asks for a reconnect.
#[tokio::test]
async fn dead_link_rebinds_and_asks_for_reconnect() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let hole = Arc::new(tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap());
    let port = hole.local_addr().unwrap().port();
    let ports = Arc::new(std::sync::Mutex::new(std::collections::HashSet::new()));
    let (h2, p2) = (hole.clone(), ports.clone());
    tokio::spawn(async move {
        let mut buf = [0u8; 1500];
        while let Ok((_, from)) = h2.recv_from(&mut buf).await {
            p2.lock().unwrap().insert(from.port());
        }
    });
    let swallow: WsSender = Arc::new(|_| {});
    let (down_tx, _down) = mpsc::unbounded_channel();
    let cfg = LinkCfg { host: "127.0.0.1".into(), udp_port: port, session: 1, key: VoiceKey([1; 32]), keys: None, channel: 0 };
    let link = VoiceLink::start(cfg, swallow, down_tx);
    let dead = Arc::new(AtomicUsize::new(0));
    let d2 = dead.clone();
    link.set_on_dead(Arc::new(move || {
        d2.fetch_add(1, Ordering::Relaxed);
    }));
    tokio::time::sleep(Duration::from_millis(7000)).await;
    assert!(dead.load(Ordering::Relaxed) >= 1, "no reconnect requested");
    assert!(ports.lock().unwrap().len() >= 2, "UDP socket was never rebound: {:?}", ports.lock().unwrap());
    link.stop();
}

/// Go Live data through the relay: the streamer's UP_VIDEO to a watching viewer arrives as video
/// from the streamer, and nothing reaches someone who is not watching.
#[tokio::test]
async fn video_is_relayed_only_to_watchers() {
    let app = TestApp::spawn().await;
    let (mut a, ch) = client(&app, "Dimas", None, None).await;
    let (mut b, _) = client(&app, "Ayu", Some(ch), None).await;
    deliver(&a, &mut b, b"warm").await;
    deliver(&b, &mut a, b"warm").await;
    let (vtx, mut vrx) = mpsc::unbounded_channel::<(i64, Vec<u8>)>();
    b.link.set_video_sink(Arc::new(move |from, data: &[u8]| {
        let _ = vtx.send((from, data.to_vec()));
    }));
    a.link.send_video(b.user, b"too early");
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(vrx.try_recv().is_err(), "not watching yet: nothing relayed");
    a.json.send(json!({"t":"streamStart","d":{"codecs":["h264"],"preset":"1080p60"}})).unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;
    b.json.send(json!({"t":"streamWatch","d":{"userId":a.user,"codecs":["h264"]}})).unwrap();
    for _ in 0..40 {
        a.link.send_video(b.user, b"frame");
        if let Ok(Some((from, data))) = tokio::time::timeout(Duration::from_millis(100), vrx.recv()).await {
            assert_eq!(from, a.user);
            assert_eq!(data, b"frame");
            return;
        }
    }
    panic!("video never arrived");
}
