use rynuvo_proto::{VoiceMember, e2ee::{DeviceKey, Scope}, WsServerMsg, voice_wire::VoiceKey};
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

fn key() -> String {
    DeviceKey::generate().public().to_b64()
}

fn join(ch: i64) -> serde_json::Value {
    join_as(ch, &key())
}

fn join_as(ch: i64, pubkey: &str) -> serde_json::Value {
    json!({"t":"voiceJoin","d":{"channelId":ch,"pubkey":pubkey}})
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
    assert_eq!(m, VoiceMember { user_id: a.user.id, channel_id: v.id, muted: false, deafened: false, joined_at: m.joined_at, call_since: m.joined_at, pubkey: m.pubkey.clone() });
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
    let k = key();
    wa.send_json(join_as(v.id, &k)).await;
    member_update(&mut wb, a.user.id).await;
    let mut wa2 = ready(&app, &a.token).await;
    wa2.send_json(join_as(v.id, &k)).await;
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

async fn a_and_b() -> (TestApp, rynuvo_proto::JoinResp, rynuvo_proto::JoinResp) {
    let app = TestApp::spawn().await;
    let a = app.user("Dimas").await;
    let b = app.user("Ayu").await;
    (app, a, b)
}

#[tokio::test]
async fn members_carry_their_pubkey() {
    let (app, a, b) = a_and_b().await;
    let v = app.create_channel(&a.token, "voice", "Mabar").await;
    let mut wa = ready(&app, &a.token).await;
    let mut wb = ready(&app, &b.token).await;
    let k = key();
    wa.send_json(join_as(v.id, &k)).await;
    assert_eq!(member_update(&mut wb, a.user.id).await.pubkey, k);
}

#[tokio::test]
async fn bad_pubkey_cannot_join() {
    let (app, a, _) = a_and_b().await;
    let v = app.create_channel(&a.token, "voice", "Mabar").await;
    let mut wa = ready(&app, &a.token).await;
    wa.send_json(join_as(v.id, "x")).await;
    assert_eq!(voice_ended(&mut wa).await, "Update Rynuvo to join voice");
    assert!(app.state.voice.members().is_empty());
}

#[tokio::test]
async fn envelopes_only_flow_within_a_scope() {
    let app = TestApp::spawn().await;
    let a = app.user("Dimas").await;
    let b = app.user("Ayu").await;
    let c = app.user("Budi").await;
    let v1 = app.create_channel(&a.token, "voice", "One").await;
    let v2 = app.create_channel(&a.token, "voice", "Two").await;
    let (mut wa, mut wb, mut wc) = (ready(&app, &a.token).await, ready(&app, &b.token).await, ready(&app, &c.token).await);
    wa.send_json(join(v1.id)).await;
    wb.send_json(join(v1.id)).await;
    wc.send_json(join(v2.id)).await;
    member_update(&mut wc, b.user.id).await;
    let send = |to: i64, envelope: String| json!({"t":"e2eeKey","d":{"to":to,"scope":{"kind":"voice","id":v1.id},"generation":1,"envelope":envelope}});
    // Other channel and oversized envelopes are dropped; the in-scope one must still arrive.
    wa.send_json(send(c.user.id, "AAAA".into())).await;
    wa.send_json(send(b.user.id, "A".repeat(rynuvo_proto::MAX_ENVELOPE_B64 + 1))).await;
    wa.send_json(send(b.user.id, "AAAA".into())).await;
    let mut got = Vec::new();
    while let Some(m) = wb.try_next(400).await {
        if let WsServerMsg::E2eeKey { from, generation, envelope, .. } = m {
            got.push((from, generation, envelope));
        }
    }
    assert_eq!(got, vec![(a.user.id, 1, "AAAA".to_string())], "only the in-size envelope arrives");
    while let Some(m) = wc.try_next(300).await {
        assert!(!matches!(m, WsServerMsg::E2eeKey { .. }), "key crossed channels");
    }
}

#[tokio::test]
async fn key_requests_are_forwarded_to_the_sender_asked() {
    let (app, a, b) = a_and_b().await;
    let v = app.create_channel(&a.token, "voice", "Mabar").await;
    let (mut wa, mut wb) = (ready(&app, &a.token).await, ready(&app, &b.token).await);
    wa.send_json(join(v.id)).await;
    wb.send_json(join(v.id)).await;
    member_update(&mut wa, b.user.id).await;
    wb.send_json(json!({"t":"e2eeKeyRequest","d":{"from":a.user.id,"scope":{"kind":"voice","id":v.id}}})).await;
    let (by, scope) = until(&mut wa, |m| match m {
        WsServerMsg::E2eeKeyRequest { by, scope } => Some((by, scope)),
        _ => None,
    })
    .await;
    assert_eq!((by, scope), (b.user.id, Scope::Voice(v.id)));
}

#[tokio::test]
async fn takeover_from_another_device_is_a_leave() {
    let (app, a, b) = a_and_b().await;
    let v = app.create_channel(&a.token, "voice", "Mabar").await;
    let mut wa = ready(&app, &a.token).await;
    let mut wb = ready(&app, &b.token).await;
    wa.send_json(join(v.id)).await;
    member_update(&mut wb, a.user.id).await;
    let mut wa2 = ready(&app, &a.token).await;
    let k2 = key();
    wa2.send_json(join_as(v.id, &k2)).await;
    // The leave must come before the new member's update.
    let mut saw_left = false;
    let m = until(&mut wb, |m| match m {
        WsServerMsg::VoiceMemberLeft { user_id, .. } if user_id == a.user.id => {
            saw_left = true;
            None
        }
        WsServerMsg::VoiceMemberUpdated(v) if v.user_id == a.user.id => Some(v),
        _ => None,
    })
    .await;
    assert!(saw_left, "takeover must be announced as a leave first");
    assert_eq!(m.pubkey, k2);
    assert_eq!(voice_ended(&mut wa).await, "You joined voice from another device");
}

#[tokio::test]
async fn reconnect_same_key_is_not_a_leave() {
    let (app, a, b) = a_and_b().await;
    let v = app.create_channel(&a.token, "voice", "Mabar").await;
    let mut wa = ready(&app, &a.token).await;
    let mut wb = ready(&app, &b.token).await;
    let k = key();
    wa.send_json(join_as(v.id, &k)).await;
    member_update(&mut wb, a.user.id).await;
    let mut wa2 = ready(&app, &a.token).await;
    wa2.send_json(join_as(v.id, &k)).await;
    let mut left = false;
    until(&mut wb, |m| match m {
        WsServerMsg::VoiceMemberLeft { .. } => {
            left = true;
            None
        }
        WsServerMsg::VoiceMemberUpdated(v) if v.user_id == a.user.id => Some(()),
        _ => None,
    })
    .await;
    assert!(!left, "a same-key reconnect is not a leave");
}

fn key_msg(to: i64, scope: serde_json::Value, envelope: &str) -> serde_json::Value {
    json!({"t":"e2eeKey","d":{"to":to,"scope":scope,"generation":1,"envelope":envelope}})
}

/// Every envelope this socket receives until it goes quiet.
async fn envelopes(ws: &mut WsClient) -> Vec<(i64, String)> {
    let mut out = Vec::new();
    while let Some(m) = ws.try_next(400).await {
        if let WsServerMsg::E2eeKey { from, envelope, .. } = m {
            out.push((from, envelope));
        }
    }
    out
}

#[tokio::test]
async fn voice_scope_needs_both_in_that_channel() {
    let app = TestApp::spawn().await;
    let a = app.user("Dimas").await;
    let c = app.user("Budi").await;
    let v1 = app.create_channel(&a.token, "voice", "One").await;
    let v2 = app.create_channel(&a.token, "voice", "Two").await;
    let (mut wa, mut wc) = (ready(&app, &a.token).await, ready(&app, &c.token).await);
    wa.send_json(join(v1.id)).await;
    wc.send_json(join(v2.id)).await;
    member_update(&mut wc, a.user.id).await;
    // A is in v1, C in v2, and the scope names v2: only "both are members" rejects this.
    wa.send_json(key_msg(c.user.id, json!({"kind":"voice","id":v2.id}), "AAAA")).await;
    assert!(envelopes(&mut wc).await.is_empty());
}

#[tokio::test]
async fn stream_scope_flows_between_streamer_and_viewer_only() {
    let app = TestApp::spawn().await;
    let a = app.user("Dimas").await; // streamer
    let b = app.user("Ayu").await; // viewer
    let c = app.user("Budi").await; // in the channel, not watching
    let v = app.create_channel(&a.token, "voice", "Mabar").await;
    let (mut wa, mut wb, mut wc) = (ready(&app, &a.token).await, ready(&app, &b.token).await, ready(&app, &c.token).await);
    for w in [&mut wa, &mut wb, &mut wc] {
        w.send_json(join(v.id)).await;
    }
    member_update(&mut wc, a.user.id).await;
    member_update(&mut wc, b.user.id).await;
    wa.send_json(json!({"t":"streamStart","d":{"codecs":["h264"],"preset":"1080p60"}})).await;
    until(&mut wb, |m| matches!(m, WsServerMsg::StreamStarted(_)).then_some(())).await;
    wb.send_json(json!({"t":"streamWatch","d":{"userId":a.user.id,"codecs":["h264"]}})).await;
    until(&mut wa, |m| matches!(m, WsServerMsg::StreamViewer { joined: true, .. }).then_some(())).await;
    let scope = json!({"kind":"stream","id":a.user.id});

    // (a) both ways between streamer and viewer
    wa.send_json(key_msg(b.user.id, scope.clone(), "from-a")).await;
    wb.send_json(key_msg(a.user.id, scope.clone(), "from-b")).await;
    assert_eq!(envelopes(&mut wb).await, vec![(a.user.id, "from-a".to_string())]);
    assert_eq!(envelopes(&mut wa).await, vec![(b.user.id, "from-b".to_string())]);

    // (b) a channel member who is not a viewer neither sends nor receives
    wa.send_json(key_msg(c.user.id, scope.clone(), "to-c")).await;
    wc.send_json(key_msg(a.user.id, scope.clone(), "from-c")).await;
    wb.send_json(key_msg(c.user.id, scope.clone(), "b-to-c")).await;
    assert!(envelopes(&mut wc).await.is_empty(), "non-viewer received a stream key");
    assert!(envelopes(&mut wa).await.is_empty(), "non-viewer sent a stream key");

    // (c) after unwatching, nothing flows
    wb.send_json(json!({"t":"streamUnwatch","d":{"userId":a.user.id}})).await;
    until(&mut wa, |m| matches!(m, WsServerMsg::StreamViewer { joined: false, .. }).then_some(())).await;
    wa.send_json(key_msg(b.user.id, scope.clone(), "late-a")).await;
    wb.send_json(key_msg(a.user.id, scope.clone(), "late-b")).await;
    assert!(envelopes(&mut wb).await.is_empty());
    assert!(envelopes(&mut wa).await.is_empty());
}

#[tokio::test]
async fn old_device_cannot_send_envelopes_after_takeover() {
    let (app, a, b) = a_and_b().await;
    let v = app.create_channel(&a.token, "voice", "Mabar").await;
    let mut wa = ready(&app, &a.token).await;
    let mut wb = ready(&app, &b.token).await;
    wa.send_json(join(v.id)).await;
    wb.send_json(join(v.id)).await;
    member_update(&mut wa, b.user.id).await;
    let mut wa2 = ready(&app, &a.token).await;
    wa2.send_json(join(v.id)).await;
    assert_eq!(voice_ended(&mut wa).await, "You joined voice from another device");
    let scope = json!({"kind":"voice","id":v.id});
    wa.send_json(key_msg(b.user.id, scope.clone(), "old")).await;
    wa2.send_json(key_msg(b.user.id, scope, "new")).await;
    assert_eq!(envelopes(&mut wb).await, vec![(a.user.id, "new".to_string())]);
}
