use rynuvo_proto::{CLOSE_AUTH_REVOKED, CLOSE_PROTOCOL_OLD, WsServerMsg};
use rynuvo_server::testutil::TestApp;
use serde_json::json;

#[tokio::test]
async fn hello_gets_ready_with_snapshot() {
    let app = TestApp::spawn().await;
    let a = app.user("Dimas").await;
    let c = app.create_channel(&a.token, "text", "umum").await;
    let mut ws = app.ws(&a.token).await;
    match ws.next().await {
        WsServerMsg::Ready { me, users, channels, online, alerts } => {
            assert_eq!(me.id, a.user.id);
            assert_eq!(users.len(), 1);
            assert_eq!(channels, vec![c]);
            assert_eq!(online, vec![a.user.id]);
            assert!(alerts.is_empty());
        }
        m => panic!("expected Ready, got {m:?}"),
    }
}

#[tokio::test]
async fn bad_token_closed_4001() {
    let app = TestApp::spawn().await;
    let mut ws = app.ws_raw().await;
    ws.send_json(json!({"t":"hello","d":{"token":"nope","protocol":1}})).await;
    assert_eq!(ws.expect_close().await, CLOSE_AUTH_REVOKED);
}

#[tokio::test]
async fn old_protocol_closed_4002() {
    let app = TestApp::spawn().await;
    let a = app.user("Dimas").await;
    let mut ws = app.ws_raw().await;
    ws.send_json(json!({"t":"hello","d":{"token":a.token,"protocol":0}})).await;
    assert_eq!(ws.expect_close().await, CLOSE_PROTOCOL_OLD);
}

#[tokio::test]
async fn message_post_broadcasts_to_other_client() {
    let app = TestApp::spawn().await;
    let a = app.user("Dimas").await;
    let b = app.user("Ayu").await;
    let c = app.create_channel(&a.token, "text", "umum").await;
    let mut wb = app.ws(&b.token).await;
    wb.next().await; // Ready
    app.client.post(app.url(&format!("/api/v1/channels/{}/messages", c.id))).bearer_auth(&a.token).json(&json!({"content":"halo"})).send().await.unwrap();
    match wb.next().await {
        WsServerMsg::MessageCreated(m) => assert_eq!(m.content, "halo"),
        m => panic!("got {m:?}"),
    }
}

#[tokio::test]
async fn presence_online_offline() {
    let app = TestApp::spawn().await;
    let a = app.user("Dimas").await;
    let b = app.user("Ayu").await;
    let mut wa = app.ws(&a.token).await;
    wa.next().await;
    let wb = app.ws(&b.token).await;
    assert_eq!(wa.next().await, WsServerMsg::Presence { user_id: b.user.id, online: true });
    // second connection of same user: no extra presence
    let mut wb2 = app.ws(&b.token).await;
    wb2.next().await;
    drop(wb);
    wb2.close().await;
    assert_eq!(wa.next().await, WsServerMsg::Presence { user_id: b.user.id, online: false });
}

#[tokio::test]
async fn typing_relayed_not_echoed() {
    let app = TestApp::spawn().await;
    let a = app.user("Dimas").await;
    let b = app.user("Ayu").await;
    let c = app.create_channel(&a.token, "text", "umum").await;
    let mut wa = app.ws(&a.token).await;
    wa.next().await;
    let mut wb = app.ws(&b.token).await;
    wb.next().await;
    wa.next().await; // presence of b
    wa.send_json(json!({"t":"typing","d":{"channelId":c.id}})).await;
    wa.send_json(json!({"t":"typing","d":{"channelId":c.id}})).await; // rate limited
    wa.send_json(json!({"t":"ping"})).await;
    assert_eq!(wb.next().await, WsServerMsg::Typing { channel_id: c.id, user_id: a.user.id });
    assert_eq!(wa.next().await, WsServerMsg::Pong, "no typing echo to sender");
    assert!(wb.try_next(300).await.is_none(), "second typing within 3s is dropped");
}

#[tokio::test]
async fn oversized_message_drops_connection() {
    let app = TestApp::spawn().await;
    let a = app.user("Dimas").await;
    let mut ws = app.ws_raw().await;
    // A valid hello padded past the 64 KiB message limit must not be accepted.
    let pad = "x".repeat(200 * 1024);
    ws.send_json(json!({"t":"hello","d":{"token":a.token,"protocol":rynuvo_proto::PROTOCOL_VERSION,"pad":pad}})).await;
    assert!(ws.try_next(2000).await.is_none(), "oversized hello must not produce Ready");
    // Server keeps working for normal clients.
    let mut ok = app.ws(&a.token).await;
    assert!(matches!(ok.next().await, WsServerMsg::Ready { .. }));
}
