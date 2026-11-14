use std::{sync::Arc, time::Duration};

use rynuvo_desktop_lib::voice::link::{DownAudio, LinkCfg, LinkPath, VoiceLink, WsSender};
use rynuvo_proto::{WsServerMsg, voice_wire::VoiceKey};
use rynuvo_server::testutil::{TestApp, WsClient};
use serde_json::json;
use tokio::sync::mpsc;

struct Client {
    link: VoiceLink,
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
    ws.send_json(json!({"t":"voiceJoin","d":{"channelId":ch}})).await;
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
    let cfg = LinkCfg { host: "127.0.0.1".into(), udp_port: udp_port_override.unwrap_or(udp_port), session, key };
    let link = VoiceLink::start(cfg, sender, down_tx);
    let l2 = link.clone();
    tokio::spawn(async move {
        loop {
            while let Ok(b) = ws_rx.try_recv() {
                ws.send_binary(b).await;
            }
            if let Some(b) = ws.next_binary(20).await {
                l2.feed_ws(&b);
            }
        }
    });
    (Client { link, down, user: u.user.id }, ch)
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
