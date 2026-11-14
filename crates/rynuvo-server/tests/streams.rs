use std::time::Duration;

use rynuvo_proto::{
    StreamInfo, WsServerMsg,
    voice_wire::{DOWN_AUDIO, DOWN_KEEPALIVE_ACK, DOWN_VIDEO, Down, UP_AUDIO, UP_KEEPALIVE, UP_VIDEO, VoiceKey, open_down, seal_up},
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


impl Peer {
    async fn video(&mut self, app: &TestApp, to: i64, frag: &[u8]) {
        let mut pl = to.to_be_bytes().to_vec();
        pl.extend_from_slice(frag);
        let p = self.packet(UP_VIDEO, &pl);
        self.udp.send_to(&p, app.udp).await.unwrap();
    }
    async fn recv_kind(&self, kind: u8, ms: u64) -> Option<Down> {
        let deadline = tokio::time::Instant::now() + Duration::from_millis(ms);
        loop {
            let left = deadline.saturating_duration_since(tokio::time::Instant::now()).as_millis() as u64;
            let d = self.recv(left.max(1)).await?;
            if d.kind == kind {
                return Some(d);
            }
        }
    }
}

fn start() -> serde_json::Value {
    json!({"t":"streamStart","d":{"codecs":["hevc","h264"],"preset":"1080p60"}})
}
fn watch(user: i64) -> serde_json::Value {
    json!({"t":"streamWatch","d":{"userId":user,"codecs":["h264"]}})
}

async fn started(ws: &mut WsClient, user: i64) -> StreamInfo {
    until(ws, |m| match m {
        WsServerMsg::StreamStarted(s) if s.user_id == user => Some(s),
        _ => None,
    })
    .await
}

async fn viewer_msg(ws: &mut WsClient) -> (i64, i64, String, bool, Vec<String>) {
    until(ws, |m| match m {
        WsServerMsg::StreamViewer { streamer, viewer, key, joined, codecs } => Some((streamer, viewer, key, joined, codecs)),
        _ => None,
    })
    .await
}

#[tokio::test]
async fn stream_start_requires_voice() {
    let app = TestApp::spawn().await;
    let a = app.user("Dimas").await;
    let mut ws = app.ws(&a.token).await;
    until(&mut ws, |m| matches!(m, WsServerMsg::Ready { .. }).then_some(())).await;
    ws.send_json(start()).await;
    while let Some(m) = ws.try_next(400).await {
        assert!(!matches!(m, WsServerMsg::StreamStarted(_)), "started a stream outside voice");
    }
}

#[tokio::test]
async fn watch_gets_keys_both_sides() {
    let app = TestApp::spawn().await;
    let (mut pa, mut pb, a_id, b_id) = two_in_voice(&app).await;
    pa.ws.send_json(start()).await;
    let info = started(&mut pb.ws, a_id).await;
    assert_eq!(info.codecs, vec!["hevc", "h264"]);
    pb.ws.send_json(watch(a_id)).await;
    let (s1, v1, k1, j1, c1) = viewer_msg(&mut pa.ws).await;
    let (s2, v2, k2, j2, _) = viewer_msg(&mut pb.ws).await;
    assert_eq!((s1, v1, j1, s2, v2, j2), (a_id, b_id, true, a_id, b_id, true));
    assert_eq!(k1, k2);
    assert!(VoiceKey::from_b64(&k1).is_some());
    assert_eq!(c1, vec!["h264"]);
}

#[tokio::test]
async fn video_relayed_only_to_watcher() {
    let app = TestApp::spawn().await;
    let a = app.user("Dimas").await;
    let b = app.user("Ayu").await;
    let c = app.user("Raka").await;
    let v = app.create_channel(&a.token, "voice", "Mabar").await;
    let mut pa = Peer::join(&app, &a.token, v.id).await;
    let mut pb = Peer::join(&app, &b.token, v.id).await;
    let pc = Peer::join(&app, &c.token, v.id).await;
    // Not streaming yet: dropped.
    pa.video(&app, b.user.id, b"early").await;
    assert!(pb.recv_kind(DOWN_VIDEO, 300).await.is_none());
    pa.ws.send_json(start()).await;
    started(&mut pb.ws, a.user.id).await;
    // Streaming but B is not watching: dropped.
    pa.video(&app, b.user.id, b"nobody").await;
    assert!(pb.recv_kind(DOWN_VIDEO, 300).await.is_none());
    pb.ws.send_json(watch(a.user.id)).await;
    viewer_msg(&mut pa.ws).await;
    pa.video(&app, b.user.id, b"frame").await;
    let d = pb.recv_kind(DOWN_VIDEO, 1000).await.expect("watcher gets video");
    assert_eq!((d.from_user, d.payload.as_slice()), (a.user.id, &b"frame"[..]));
    // Aimed at C, who does not watch: dropped.
    pa.video(&app, c.user.id, b"frame").await;
    assert!(pc.recv_kind(DOWN_VIDEO, 300).await.is_none());
    // Viewer feedback goes back to the streamer.
    pb.video(&app, a.user.id, b"report").await;
    let d = pa.recv_kind(DOWN_VIDEO, 1000).await.expect("streamer gets viewer feedback");
    assert_eq!((d.from_user, d.payload.as_slice()), (b.user.id, &b"report"[..]));
    // Audio still flows as before.
    pa.audio(&app, b"opus").await;
    assert!(pb.recv_kind(DOWN_AUDIO, 1000).await.is_some());
    let _ = DOWN_KEEPALIVE_ACK;
}

#[tokio::test]
async fn stream_ends_when_streamer_leaves_voice() {
    let app = TestApp::spawn().await;
    let (mut pa, mut pb, a_id, b_id) = two_in_voice(&app).await;
    pa.ws.send_json(start()).await;
    started(&mut pb.ws, a_id).await;
    pb.ws.send_json(watch(a_id)).await;
    viewer_msg(&mut pb.ws).await;
    pa.ws.send_json(json!({"t":"voiceLeave"})).await;
    let ch = until(&mut pb.ws, |m| match m {
        WsServerMsg::StreamStopped { user_id, channel_id } if user_id == a_id => Some(channel_id),
        _ => None,
    })
    .await;
    assert!(ch > 0);
    // Relay bytes are written for the pair.
    let mut n: i64 = 0;
    for _ in 0..20 {
        n = sqlx::query_scalar("SELECT COUNT(*) FROM stream_usage WHERE streamer_id = ? AND viewer_id = ?")
            .bind(a_id)
            .bind(b_id)
            .fetch_one(&app.state.db)
            .await
            .unwrap();
        if n > 0 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert_eq!(n, 1);
}

#[tokio::test]
async fn viewer_leaving_voice_notifies_streamer() {
    let app = TestApp::spawn().await;
    let (mut pa, mut pb, a_id, b_id) = two_in_voice(&app).await;
    pa.ws.send_json(start()).await;
    started(&mut pb.ws, a_id).await;
    pb.ws.send_json(watch(a_id)).await;
    viewer_msg(&mut pa.ws).await;
    pb.ws.send_json(json!({"t":"voiceLeave"})).await;
    let (s, v, _, joined, _) = viewer_msg(&mut pa.ws).await;
    assert_eq!((s, v, joined), (a_id, b_id, false));
}

#[tokio::test]
async fn ready_lists_streams_and_signal_forwarded() {
    let app = TestApp::spawn().await;
    let (mut pa, mut pb, a_id, b_id) = two_in_voice(&app).await;
    pa.ws.send_json(start()).await;
    started(&mut pb.ws, a_id).await;
    let tok = app.user("Raka").await.token;
    let mut wc = app.ws(&tok).await;
    let streams = until(&mut wc, |m| match m {
        WsServerMsg::Ready { streams, .. } => Some(streams),
        _ => None,
    })
    .await;
    assert_eq!(streams.iter().map(|s| s.user_id).collect::<Vec<_>>(), vec![a_id]);
    // Signals pass only within a pair.
    pb.ws.send_json(json!({"t":"streamSignal","d":{"to":a_id,"data":{"x":1}}})).await;
    while let Some(m) = pa.ws.try_next(300).await {
        assert!(!matches!(m, WsServerMsg::StreamSignal { .. }), "signal outside a pair forwarded");
    }
    pb.ws.send_json(watch(a_id)).await;
    viewer_msg(&mut pa.ws).await;
    pb.ws.send_json(json!({"t":"streamSignal","d":{"to":a_id,"data":{"x":2}}})).await;
    let (from, data) = until(&mut pa.ws, |m| match m {
        WsServerMsg::StreamSignal { from, data } => Some((from, data)),
        _ => None,
    })
    .await;
    assert_eq!((from, data), (b_id, json!({"x":2})));
}

#[tokio::test]
async fn video_relay_stops_when_quota_used_up_but_voice_continues() {
    let app = TestApp::spawn().await;
    let a = app.user("Dimas").await;
    let b = app.user("Ayu").await;
    let v = app.create_channel(&a.token, "voice", "Mabar").await;
    let mut pa = Peer::join(&app, &a.token, v.id).await;
    let mut pb = Peer::join(&app, &b.token, v.id).await;
    pa.ws.send_json(start()).await;
    started(&mut pb.ws, a.user.id).await;
    pb.ws.send_json(watch(a.user.id)).await;
    viewer_msg(&mut pa.ws).await;
    pa.video(&app, b.user.id, b"frame").await;
    assert!(pb.recv_kind(DOWN_VIDEO, 1000).await.is_some());
    let q = &app.state.relay.quota;
    assert!(q.used() > 0, "relayed bytes are counted");
    q.add(q.limit());
    pa.video(&app, b.user.id, b"frame").await;
    assert!(pb.recv_kind(DOWN_VIDEO, 300).await.is_none(), "budget used up: no relayed screen share");
    pa.audio(&app, b"opus").await;
    assert!(pb.recv_kind(DOWN_AUDIO, 1000).await.is_some(), "voice is never cut");
}
