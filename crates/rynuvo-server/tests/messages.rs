use rynuvo_proto::{ApiError, Message};
use rynuvo_server::testutil::TestApp;
use serde_json::json;

async fn post(app: &TestApp, token: &str, ch: i64, content: &str) -> reqwest::Response {
    app.client.post(app.url(&format!("/api/v1/channels/{ch}/messages"))).bearer_auth(token).json(&json!({"content": content})).send().await.unwrap()
}

async fn list(app: &TestApp, token: &str, ch: i64, q: &str) -> Vec<Message> {
    app.client.get(app.url(&format!("/api/v1/channels/{ch}/messages{q}"))).bearer_auth(token).send().await.unwrap().json().await.unwrap()
}

#[tokio::test]
async fn post_and_list_in_order() {
    let app = TestApp::spawn().await;
    let a = app.user("Dimas").await;
    let c = app.create_channel(&a.token, "text", "umum").await;
    let m: Message = post(&app, &a.token, c.id, "  halo  ").await.json().await.unwrap();
    assert_eq!(m.content, "halo");
    assert_eq!(m.author_id, a.user.id);
    post(&app, &a.token, c.id, "dua").await;
    let l = list(&app, &a.token, c.id, "").await;
    assert_eq!(l.iter().map(|m| m.content.as_str()).collect::<Vec<_>>(), ["halo", "dua"]);
    assert_eq!(post(&app, &a.token, c.id, "   ").await.status(), 400);
}

#[tokio::test]
async fn pagination_before_and_after() {
    let app = TestApp::spawn().await;
    let a = app.user("Dimas").await;
    let c = app.create_channel(&a.token, "text", "umum").await;
    for i in 0..120 {
        post(&app, &a.token, c.id, &format!("m{i}")).await;
    }
    let last = list(&app, &a.token, c.id, "?limit=50").await;
    assert_eq!(last.len(), 50);
    assert_eq!(last[0].content, "m70");
    assert_eq!(last[49].content, "m119");
    let prev = list(&app, &a.token, c.id, &format!("?before={}&limit=50", last[0].id)).await;
    assert_eq!(prev.len(), 50);
    assert_eq!(prev[0].content, "m20");
    assert_eq!(prev[49].content, "m69");
    let all = list(&app, &a.token, c.id, "?limit=1000").await;
    assert_eq!(all.len(), 100, "limit capped at 100");
    let m100 = &all[79]; // m99 is 100th message overall (index 79 of last 100 = m99)
    assert_eq!(m100.content, "m99");
    let after = list(&app, &a.token, c.id, &format!("?after={}", m100.id)).await;
    assert_eq!(after.len(), 20);
    assert_eq!(after[0].content, "m100");
}

#[tokio::test]
async fn message_too_long_rejected() {
    let app = TestApp::spawn().await;
    let a = app.user("Dimas").await;
    let c = app.create_channel(&a.token, "text", "umum").await;
    let r = post(&app, &a.token, c.id, &"é".repeat(4001)).await;
    assert_eq!(r.status(), 400);
    assert_eq!(r.json::<ApiError>().await.unwrap().code, "too_long");
    assert_eq!(post(&app, &a.token, c.id, &"é".repeat(4000)).await.status(), 200);
}

#[tokio::test]
async fn only_author_edits() {
    let app = TestApp::spawn().await;
    let a = app.user("Dimas").await;
    let b = app.user("Ayu").await;
    let c = app.create_channel(&a.token, "text", "umum").await;
    let m: Message = post(&app, &b.token, c.id, "asli").await.json().await.unwrap();
    let r = app.client.patch(app.url(&format!("/api/v1/messages/{}", m.id))).bearer_auth(&a.token).json(&json!({"content":"ubah"})).send().await.unwrap();
    assert_eq!(r.status(), 403, "channel owner cannot edit others' text");
    let e: Message = app.client.patch(app.url(&format!("/api/v1/messages/{}", m.id))).bearer_auth(&b.token).json(&json!({"content":"ubah"})).send().await.unwrap().json().await.unwrap();
    assert_eq!(e.content, "ubah");
    assert!(e.edited_at.is_some());
}

#[tokio::test]
async fn channel_owner_can_delete_others() {
    let app = TestApp::spawn().await;
    let a = app.user("Dimas").await;
    let b = app.user("Ayu").await;
    let x = app.user("Bayu").await;
    let c = app.create_channel(&a.token, "text", "umum").await;
    let m: Message = post(&app, &b.token, c.id, "spam").await.json().await.unwrap();
    let r = app.client.delete(app.url(&format!("/api/v1/messages/{}", m.id))).bearer_auth(&x.token).send().await.unwrap();
    assert_eq!(r.status(), 403);
    let r = app.client.delete(app.url(&format!("/api/v1/messages/{}", m.id))).bearer_auth(&a.token).send().await.unwrap();
    assert_eq!(r.status(), 204);
    assert!(list(&app, &a.token, c.id, "").await.is_empty());
}

#[tokio::test]
async fn cannot_post_to_voice_channel() {
    let app = TestApp::spawn().await;
    let a = app.user("Dimas").await;
    let c = app.create_channel(&a.token, "voice", "Mabar").await;
    assert_eq!(post(&app, &a.token, c.id, "halo").await.status(), 400);
}
