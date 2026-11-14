use std::time::Duration;

use rynuvo_proto::{
    WsServerMsg,
    voice_wire::{DOWN_AUDIO, DOWN_KEEPALIVE_ACK, Down, UP_AUDIO, UP_KEEPALIVE, VoiceKey, open_down, seal_up},
};
use rynuvo_server::testutil::{TestApp, WsClient};
use serde_json::json;
use tokio::net::UdpSocket;

struct Peer {
    ws: WsClient,
    session: u32,
    key: VoiceKey,
    udp: UdpSocket,
    seq: u32,
}

async fn until<T>(ws: &mut WsClient, mut f: impl FnMut(WsServerMsg) -> Option<T>) -> T {
    loop {
        let m = ws.try_next(3000).await.expect("expected a matching message");
        if let Some(t) = f(m) {
            return t;
        }
    }
}

impl Peer {
    async fn join(app: &TestApp, token: &str, ch: i64) -> Peer {
        let mut ws = app.ws(token).await;
        until(&mut ws, |m| matches!(m, WsServerMsg::Ready { .. }).then_some(())).await;
        ws.send_json(json!({"t":"voiceJoin","d":{"channelId":ch}})).await;
        let (session, key) = until(&mut ws, |m| match m {
            WsServerMsg::VoiceJoined { session, key, .. } => Some((session, VoiceKey::from_b64(&key).unwrap())),
            _ => None,
        })
        .await;
        let udp = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let mut p = Peer { ws, session, key, udp, seq: 0 };
        p.keepalive(app).await;
        assert_eq!(p.recv(500).await.expect("keepalive ack").kind, DOWN_KEEPALIVE_ACK);
        p
    }

    fn packet(&mut self, kind: u8, payload: &[u8]) -> Vec<u8> {
        self.seq += 1;
        seal_up(&self.key, kind, self.session, self.seq, payload)
    }

    async fn keepalive(&mut self, app: &TestApp) {
        let p = self.packet(UP_KEEPALIVE, &[]);
        self.udp.send_to(&p, app.udp).await.unwrap();
    }

    async fn audio(&mut self, app: &TestApp, payload: &[u8]) {
        let p = self.packet(UP_AUDIO, payload);
        self.udp.send_to(&p, app.udp).await.unwrap();
    }

    async fn recv(&self, ms: u64) -> Option<Down> {
        let mut buf = [0u8; 1500];
        let (n, _) = tokio::time::timeout(Duration::from_millis(ms), self.udp.recv_from(&mut buf)).await.ok()?.ok()?;
        Some(open_down(&self.key, &buf[..n]).expect("decrypts with my key"))
    }

    async fn recv_audio(&self, ms: u64) -> Option<Down> {
        let deadline = tokio::time::Instant::now() + Duration::from_millis(ms);
        loop {
            let left = deadline.saturating_duration_since(tokio::time::Instant::now()).as_millis() as u64;
            let d = self.recv(left.max(1)).await?;
            if d.kind == DOWN_AUDIO {
                return Some(d);
            }
        }
    }
}

async fn two_in_voice(app: &TestApp) -> (Peer, Peer, i64, i64) {
    let a = app.user("Dimas").await;
    let b = app.user("Ayu").await;
    let v = app.create_channel(&a.token, "voice", "Mabar").await;
    let pa = Peer::join(app, &a.token, v.id).await;
    let pb = Peer::join(app, &b.token, v.id).await;
    (pa, pb, a.user.id, b.user.id)
}

#[tokio::test]
async fn udp_audio_forwarded_to_peer() {
    let app = TestApp::spawn().await;
    let (mut pa, pb, a_id, _) = two_in_voice(&app).await;
    pa.audio(&app, b"opus-frame").await;
    let d = pb.recv_audio(1000).await.expect("B receives A's audio");
    assert_eq!(d.from_user, a_id);
    assert_eq!(d.payload, b"opus-frame");
    assert!(pa.recv_audio(200).await.is_none(), "no echo back to the sender");
}

#[tokio::test]
async fn keepalive_acked() {
    let app = TestApp::spawn().await;
    let (mut pa, _pb, _, _) = two_in_voice(&app).await;
    pa.keepalive(&app).await;
    assert_eq!(pa.recv(500).await.unwrap().kind, DOWN_KEEPALIVE_ACK);
}

#[tokio::test]
async fn muted_audio_dropped() {
    let app = TestApp::spawn().await;
    let (mut pa, pb, _, _) = two_in_voice(&app).await;
    pa.ws.send_json(json!({"t":"voiceState","d":{"muted":true,"deafened":false}})).await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    pa.audio(&app, b"x").await;
    assert!(pb.recv_audio(300).await.is_none());
}

#[tokio::test]
async fn deafened_peer_skipped() {
    let app = TestApp::spawn().await;
    let (mut pa, mut pb, _, _) = two_in_voice(&app).await;
    pb.ws.send_json(json!({"t":"voiceState","d":{"muted":false,"deafened":true}})).await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    pa.audio(&app, b"x").await;
    assert!(pb.recv_audio(300).await.is_none());
}

#[tokio::test]
async fn relay_follows_new_source_address() {
    let app = TestApp::spawn().await;
    let (mut pa, pb, a_id, _) = two_in_voice(&app).await;
    // A's NAT mapping changes: same session, new socket.
    pa.udp = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    pa.audio(&app, b"after-rebind").await;
    assert_eq!(pb.recv_audio(1000).await.unwrap().from_user, a_id);
    // Replies to A now go to the new address.
    pa.keepalive(&app).await;
    assert_eq!(pa.recv(500).await.expect("ack at new address").kind, DOWN_KEEPALIVE_ACK);
}

#[tokio::test]
async fn ws_binary_path_forwarded() {
    let app = TestApp::spawn().await;
    let (mut pa, mut pb, a_id, b_id) = two_in_voice(&app).await;
    // A switches to the WebSocket path (UDP blocked).
    let p = pa.packet(UP_KEEPALIVE, &[]);
    pa.ws.send_binary(p).await;
    let ack = pa.ws.next_binary(1000).await.expect("ack over ws");
    assert_eq!(open_down(&pa.key, &ack).unwrap().kind, DOWN_KEEPALIVE_ACK);
    // A (ws) -> B (udp)
    let p = pa.packet(UP_AUDIO, b"from-ws");
    pa.ws.send_binary(p).await;
    let d = pb.recv_audio(1000).await.expect("B gets audio sent over ws");
    assert_eq!((d.from_user, d.payload.as_slice()), (a_id, &b"from-ws"[..]));
    // B (udp) -> A (ws)
    pb.audio(&app, b"to-ws").await;
    let bin = pa.ws.next_binary(1000).await.expect("A gets audio over ws");
    let d = open_down(&pa.key, &bin).unwrap();
    assert_eq!((d.kind, d.from_user, d.payload.as_slice()), (DOWN_AUDIO, b_id, &b"to-ws"[..]));
}

#[tokio::test]
async fn relay_ignores_garbage_and_wrong_key() {
    let app = TestApp::spawn().await;
    let (mut pa, pb, _, _) = two_in_voice(&app).await;
    let junk = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    for i in 0..1000u32 {
        let mut p = vec![UP_AUDIO];
        p.extend_from_slice(&pa.session.to_be_bytes());
        p.extend_from_slice(&i.to_be_bytes());
        p.extend_from_slice(&[i as u8; 40]);
        junk.send_to(&p, app.udp).await.unwrap();
    }
    let forged = seal_up(&VoiceKey([1u8; 32]), UP_AUDIO, pa.session, 99, b"forged");
    junk.send_to(&forged, app.udp).await.unwrap();
    junk.send_to(&vec![UP_AUDIO; 5000], app.udp).await.unwrap();
    assert!(pb.recv_audio(400).await.is_none(), "nothing forwarded");
    // The relay still works, and the forged packet didn't hijack A's route.
    pa.keepalive(&app).await;
    assert_eq!(pa.recv(500).await.unwrap().kind, DOWN_KEEPALIVE_ACK);
}

#[tokio::test]
async fn stun_still_works_on_shared_port() {
    let app = TestApp::spawn().await;
    let c = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let mut req = vec![0x00, 0x01, 0x00, 0x00];
    req.extend_from_slice(&0x2112A442u32.to_be_bytes());
    req.extend_from_slice(&[5u8; 12]);
    c.send_to(&req, app.udp).await.unwrap();
    let mut buf = [0u8; 512];
    let (n, _) = tokio::time::timeout(Duration::from_secs(2), c.recv_from(&mut buf)).await.unwrap().unwrap();
    assert_eq!(&buf[0..2], &[0x01, 0x01]);
    assert!(n > 20);
}

#[tokio::test]
async fn usage_counts_relayed_bytes() {
    let app = TestApp::spawn().await;
    let (mut pa, pb, a_id, _) = two_in_voice(&app).await;
    for _ in 0..5 {
        pa.audio(&app, &[0u8; 100]).await;
        pb.recv_audio(500).await.unwrap();
    }
    let m = app.state.voice.member(a_id).unwrap();
    assert!(m.bytes_in >= 5 * 100, "bytes_in {}", m.bytes_in);
}
