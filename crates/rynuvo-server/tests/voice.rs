use rynuvo_proto::{VoiceMember, WsServerMsg, voice_wire::VoiceKey};
use rynuvo_server::testutil::{TestApp, WsClient};
use serde_json::json;

/// Skip messages until one matches.
async fn until<T>(ws: &mut WsClient, mut f: impl FnMut(WsServerMsg) -> Option<T>) -> T {
    loop {
        let m = ws.try_next(3000).await.expect("expected a matching message");
        if let Some(t) = f(m) {
            return t;
        }
    }
}

async fn ready(app: &TestApp, token: &str) -> WsClient {
    let mut ws = app.ws(token).await;
    until(&mut ws, |m| matches!(m, WsServerMsg::Ready { .. }).then_some(())).await;
    ws
}

fn join(ch: i64) -> serde_json::Value {
    json!({"t":"voiceJoin","d":{"channelId":ch}})
}

async fn member_update(ws: &mut WsClient, user: i64) -> VoiceMember {
    until(ws, |m| match m {
        WsServerMsg::VoiceMemberUpdated(v) if v.user_id == user => Some(v),
        _ => None,
    })
    .await
}

async fn member_left(ws: &mut WsClient, user: i64) -> i64 {
    until(ws, |m| match m {
        WsServerMsg::VoiceMemberLeft { user_id, channel_id } if user_id == user => Some(channel_id),
        _ => None,
    })
    .await
}

async fn voice_ended(ws: &mut WsClient) -> String {
    until(ws, |m| match m {
        WsServerMsg::VoiceEnded { reason } => Some(reason),
        _ => None,
    })
    .await
}

#[tokio::test]
async fn join_broadcasts_member_and_returns_key() {
    let app = TestApp::spawn().await;
    let a = app.user("Dimas").await;
    let b = app.user("Ayu").await;
    let v = app.create_channel(&a.token, "voice", "Mabar").await;
    let mut wa = ready(&app, &a.token).await;
    let mut wb = ready(&app, &b.token).await;
    wa.send_json(join(v.id)).await;
    let (ch, key, port) = until(&mut wa, |m| match m {
        WsServerMsg::VoiceJoined { channel_id, key, udp_port, .. } => Some((channel_id, key, udp_port)),
        _ => None,
    })
    .await;
    assert_eq!(ch, v.id);
    assert!(VoiceKey::from_b64(&key).is_some());
    assert_eq!(port, app.state.cfg.stun_ports[0]);
    let m = member_update(&mut wb, a.user.id).await;
    assert!(m.joined_at > 0, "members carry when they joined, for the call timer");
    assert_eq!(m, VoiceMember { user_id: a.user.id, channel_id: v.id, muted: false, deafened: false, joined_at: m.joined_at, call_since: m.joined_at, pubkey: String::new() });
}

#[tokio::test]
async fn join_text_channel_rejected() {
    let app = TestApp::spawn().await;
    let a = app.user("Dimas").await;
    let t = app.create_channel(&a.token, "text", "umum").await;
    let mut wa = ready(&app, &a.token).await;
    wa.send_json(join(t.id)).await;
    assert_eq!(voice_ended(&mut wa).await, "That voice channel is gone");
    wa.send_json(join(999)).await;
    assert_eq!(voice_ended(&mut wa).await, "That voice channel is gone");
    assert!(app.state.voice.members().is_empty());
}

#[tokio::test]
async fn second_join_moves_member() {
    let app = TestApp::spawn().await;
    let a = app.user("Dimas").await;
    let b = app.user("Ayu").await;
    let v1 = app.create_channel(&a.token, "voice", "One").await;
    let v2 = app.create_channel(&a.token, "voice", "Two").await;
    let mut wa = ready(&app, &a.token).await;
    let mut wb = ready(&app, &b.token).await;
    wa.send_json(join(v1.id)).await;
    member_update(&mut wb, a.user.id).await;
    // Join from a second device of the same user moves the membership.
    let mut wa2 = ready(&app, &a.token).await;
    wa2.send_json(join(v2.id)).await;
    assert_eq!(member_left(&mut wb, a.user.id).await, v1.id);
    assert_eq!(member_update(&mut wb, a.user.id).await.channel_id, v2.id);
    assert_eq!(voice_ended(&mut wa).await, "You joined voice from another device");
    assert_eq!(app.state.voice.members().len(), 1);
}

#[tokio::test]
async fn state_change_broadcast() {
    let app = TestApp::spawn().await;
    let a = app.user("Dimas").await;
    let b = app.user("Ayu").await;
    let v = app.create_channel(&a.token, "voice", "Mabar").await;
    let mut wa = ready(&app, &a.token).await;
    let mut wb = ready(&app, &b.token).await;
    wa.send_json(join(v.id)).await;
    member_update(&mut wb, a.user.id).await;
    wa.send_json(json!({"t":"voiceState","d":{"muted":true,"deafened":true}})).await;
    let m = member_update(&mut wb, a.user.id).await;
    assert!(m.muted && m.deafened);
}

#[tokio::test]
async fn disconnect_removes_member() {
    let app = TestApp::spawn().await;
    let a = app.user("Dimas").await;
    let b = app.user("Ayu").await;
    let v = app.create_channel(&a.token, "voice", "Mabar").await;
    let mut wa = ready(&app, &a.token).await;
    let mut wb = ready(&app, &b.token).await;
    wa.send_json(join(v.id)).await;
    member_update(&mut wb, a.user.id).await;
    wa.close().await;
    assert_eq!(member_left(&mut wb, a.user.id).await, v.id);
    assert!(app.state.voice.members().is_empty());
}

#[tokio::test]
async fn owner_can_disconnect_member() {
    let app = TestApp::spawn().await;
    let owner = app.user("Dimas").await;
    let b = app.user("Ayu").await;
    let c = app.user("Bayu").await;
    let v = app.create_channel(&owner.token, "voice", "Mabar").await;
    let mut wo = ready(&app, &owner.token).await;
    let mut wb = ready(&app, &b.token).await;
    let mut wc = ready(&app, &c.token).await;
    wb.send_json(join(v.id)).await;
    member_update(&mut wo, b.user.id).await;
    // Not the owner: ignored.
    wc.send_json(json!({"t":"voiceKick","d":{"userId":b.user.id}})).await;
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    assert_eq!(app.state.voice.members().len(), 1);
    wo.send_json(json!({"t":"voiceKick","d":{"userId":b.user.id}})).await;
    assert_eq!(voice_ended(&mut wb).await, "You were disconnected from the voice channel");
    assert_eq!(member_left(&mut wo, b.user.id).await, v.id);
}

#[tokio::test]
async fn channel_delete_ends_voice() {
    let app = TestApp::spawn().await;
    let a = app.user("Dimas").await;
    let v = app.create_channel(&a.token, "voice", "Mabar").await;
    let mut wa = ready(&app, &a.token).await;
    wa.send_json(join(v.id)).await;
    member_update(&mut wa, a.user.id).await;
    let r = app.client.delete(app.url(&format!("/api/v1/channels/{}", v.id))).bearer_auth(&a.token).send().await.unwrap();
    assert!(r.status().is_success());
    assert_eq!(voice_ended(&mut wa).await, "The voice channel was deleted");
    assert!(app.state.voice.members().is_empty());
}

#[tokio::test]
async fn ready_lists_voice_members() {
    let app = TestApp::spawn().await;
    let a = app.user("Dimas").await;
    let b = app.user("Ayu").await;
    let v = app.create_channel(&a.token, "voice", "Mabar").await;
    let mut wa = ready(&app, &a.token).await;
    wa.send_json(join(v.id)).await;
    member_update(&mut wa, a.user.id).await;
    let mut wb = app.ws(&b.token).await;
    let voice = until(&mut wb, |m| match m {
        WsServerMsg::Ready { voice, .. } => Some(voice),
        _ => None,
    })
    .await;
    assert_eq!(voice.len(), 1);
    assert_eq!(voice[0].user_id, a.user.id);
}

#[tokio::test]
async fn usage_row_written_on_leave() {
    let app = TestApp::spawn().await;
    let a = app.user("Dimas").await;
    let v = app.create_channel(&a.token, "voice", "Mabar").await;
    let mut wa = ready(&app, &a.token).await;
    wa.send_json(join(v.id)).await;
    member_update(&mut wa, a.user.id).await;
    wa.send_json(json!({"t":"voiceLeave"})).await;
    member_left(&mut wa, a.user.id).await;
    let mut n = 0i64;
    for _ in 0..20 {
        n = sqlx::query_scalar("SELECT COUNT(*) FROM voice_usage WHERE user_id = ? AND channel_id = ?")
            .bind(a.user.id)
            .bind(v.id)
            .fetch_one(&app.state.db)
            .await
            .unwrap();
        if n > 0 {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert_eq!(n, 1);
}

#[tokio::test]
async fn channel_full_rejected() {
    let app = TestApp::spawn().await;
    let a = app.user("Owner").await;
    let v = app.create_channel(&a.token, "voice", "Mabar").await;
    let mut socks = Vec::new();
    for i in 0..10 {
        let u = app.user(&format!("user{i}")).await;
        let mut w = ready(&app, &u.token).await;
        w.send_json(join(v.id)).await;
        member_update(&mut w, u.user.id).await;
        socks.push(w);
    }
    let mut wa = ready(&app, &a.token).await;
    wa.send_json(join(v.id)).await;
    assert_eq!(voice_ended(&mut wa).await, "This voice channel is full");
}

/// A reconnect re-joins the same channel before the server noticed the old connection died:
/// nobody (including the new connection) may see the user leave.
#[tokio::test]
async fn rejoin_same_channel_does_not_announce_leave() {
    let app = TestApp::spawn().await;
    let a = app.user("Dimas").await;
    let b = app.user("Ayu").await;
    let v = app.create_channel(&a.token, "voice", "One").await;
    let mut wa = ready(&app, &a.token).await;
    let mut wb = ready(&app, &b.token).await;
    wa.send_json(join(v.id)).await;
    member_update(&mut wb, a.user.id).await;
    let mut wa2 = ready(&app, &a.token).await;
    wa2.send_json(join(v.id)).await;
    let mut saw_left = false;
    loop {
        match wa2.try_next(1500).await {
            Some(WsServerMsg::VoiceMemberLeft { user_id, .. }) if user_id == a.user.id => saw_left = true,
            Some(WsServerMsg::VoiceJoined { .. }) => break,
            Some(_) => {}
            None => panic!("no VoiceJoined"),
        }
    }
    assert!(!saw_left, "new connection was told it left");
    assert_eq!(member_update(&mut wb, a.user.id).await.channel_id, v.id);
    while let Some(m) = wb.try_next(300).await {
        assert!(!matches!(m, WsServerMsg::VoiceMemberLeft { .. }), "peer saw a leave");
    }
    assert_eq!(app.state.voice.members().len(), 1);
}
