use rynuvo_proto::{Message, ReadMark, WsServerMsg};
use rynuvo_server::testutil::{TestApp, WsClient};
use serde_json::json;

async fn post(app: &TestApp, token: &str, ch: i64, content: &str) -> Message {
    let r = app.client.post(app.url(&format!("/api/v1/channels/{ch}/messages"))).bearer_auth(token).json(&json!({"content": content})).send().await.unwrap();
    r.json().await.unwrap()
}

async fn reads(c: &mut WsClient) -> Vec<ReadMark> {
    match c.next().await {
        WsServerMsg::Ready { reads, .. } => reads,
        m => panic!("expected ready, got {m:?}"),
    }
}

/// The next ReadUpdated, skipping presence and the like.
async fn read_updated(c: &mut WsClient) -> Option<(i64, i64)> {
    while let Some(m) = c.try_next(800).await {
        if let WsServerMsg::ReadUpdated { channel_id, last_read } = m {
            return Some((channel_id, last_read));
        }
    }
    None
}

#[tokio::test]
async fn unread_until_read_and_only_forward() {
    let app = TestApp::spawn().await;
    let a = app.user("Dimas").await;
    let b = app.user("Budi").await;
    let c = app.create_channel(&a.token, "text", "umum").await;
    let v = app.create_channel(&a.token, "voice", "ngobrol").await;
    let m1 = post(&app, &a.token, c.id, "satu").await;
    let m2 = post(&app, &a.token, c.id, "dua").await;

    // Budi hasn't read anything; Dimas wrote both, so he's read them. Voice channels have no mark.
    let mut wb = app.ws(&b.token).await;
    assert_eq!(reads(&mut wb).await, vec![ReadMark { channel_id: c.id, last_message: Some(m2.id), last_read: 0 }]);
    let mut wa = app.ws(&a.token).await;
    assert_eq!(reads(&mut wa).await, vec![ReadMark { channel_id: c.id, last_message: Some(m2.id), last_read: m2.id }]);

    // Budi reads on one device: his other device hears it, the sender doesn't.
    let mut wb2 = app.ws(&b.token).await;
    reads(&mut wb2).await;
    wb.send_json(json!({"t":"read","d":{"channelId":c.id,"messageId":m2.id}})).await;
    assert_eq!(read_updated(&mut wb2).await, Some((c.id, m2.id)));
    assert_eq!(read_updated(&mut wb).await, None);

    // Never backwards, and not for voice channels.
    wb.send_json(json!({"t":"read","d":{"channelId":c.id,"messageId":m1.id}})).await;
    wb.send_json(json!({"t":"read","d":{"channelId":v.id,"messageId":m1.id}})).await;
    assert_eq!(read_updated(&mut wb2).await, None);
    let mut wb3 = app.ws(&b.token).await;
    assert_eq!(reads(&mut wb3).await[0].last_read, m2.id);

    // A mark past the newest message stops at it, so later messages still count as unread.
    let n = post(&app, &a.token, c.id, "tiga").await;
    wb.send_json(json!({"t":"read","d":{"channelId":c.id,"messageId":n.id + 1000}})).await;
    assert_eq!(read_updated(&mut wb2).await, Some((c.id, n.id)));
}
