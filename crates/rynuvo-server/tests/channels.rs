use rynuvo_proto::{Channel, ChannelKind};
use rynuvo_server::testutil::TestApp;
use serde_json::json;

#[tokio::test]
async fn create_sets_owner() {
    let app = TestApp::spawn().await;
    let u = app.user("Dimas").await;
    let c = app.create_channel(&u.token, "text", "umum").await;
    assert_eq!(c.owner_id, u.user.id);
    assert_eq!(c.kind, ChannelKind::Text);
    assert_eq!(c.position, 1);
}

#[tokio::test]
async fn text_name_normalized() {
    let app = TestApp::spawn().await;
    let u = app.user("Dimas").await;
    assert_eq!(app.create_channel(&u.token, "text", " Clip Game ").await.name, "clip-game");
    assert_eq!(app.create_channel(&u.token, "voice", " Mabar Malam ").await.name, "Mabar Malam");
    let r = app.client.post(app.url("/api/v1/channels")).bearer_auth(&u.token).json(&json!({"kind":"text","name":"   "})).send().await.unwrap();
    assert_eq!(r.status(), 400);
    let r = app.client.post(app.url("/api/v1/channels")).bearer_auth(&u.token).json(&json!({"kind":"text","name":"x".repeat(41)})).send().await.unwrap();
    assert_eq!(r.status(), 400);
}

#[tokio::test]
async fn non_owner_cannot_rename_or_delete() {
    let app = TestApp::spawn().await;
    let a = app.user("Dimas").await;
    let b = app.user("Ayu").await;
    let c = app.create_channel(&a.token, "text", "umum").await;
    let r = app.client.patch(app.url(&format!("/api/v1/channels/{}", c.id))).bearer_auth(&b.token).json(&json!({"name":"hack"})).send().await.unwrap();
    assert_eq!(r.status(), 403);
    let r = app.client.delete(app.url(&format!("/api/v1/channels/{}", c.id))).bearer_auth(&b.token).send().await.unwrap();
    assert_eq!(r.status(), 403);
    let r = app.client.patch(app.url(&format!("/api/v1/channels/{}", c.id))).bearer_auth(&a.token).json(&json!({"name":"Ngobrol Santai","topic":"bebas"})).send().await.unwrap();
    assert_eq!(r.status(), 200);
    let c2: Channel = r.json().await.unwrap();
    assert_eq!(c2.name, "ngobrol-santai");
    assert_eq!(c2.topic.as_deref(), Some("bebas"));
    let c3: Channel = app.client.patch(app.url(&format!("/api/v1/channels/{}", c.id))).bearer_auth(&a.token).json(&json!({"topic":null})).send().await.unwrap().json().await.unwrap();
    assert_eq!(c3.topic, None);
}

#[tokio::test]
async fn admin_can_delete_any() {
    let app = TestApp::spawn().await;
    let admin = app.admin().await;
    let a = app.user("Dimas").await;
    let c = app.create_channel(&a.token, "text", "umum").await;
    let r = app.client.delete(app.url(&format!("/api/v1/channels/{}", c.id))).bearer_auth(&admin.token).send().await.unwrap();
    assert_eq!(r.status(), 204);
    let r = app.client.delete(app.url(&format!("/api/v1/channels/{}", c.id))).bearer_auth(&admin.token).send().await.unwrap();
    assert_eq!(r.status(), 404);
}

#[tokio::test]
async fn delete_cascades_messages() {
    let app = TestApp::spawn().await;
    let a = app.user("Dimas").await;
    let c = app.create_channel(&a.token, "text", "umum").await;
    sqlx::query("INSERT INTO messages(channel_id, author_id, content, created_at) VALUES(?, ?, 'hi', 0)")
        .bind(c.id).bind(a.user.id).execute(&app.state.db).await.unwrap();
    app.client.delete(app.url(&format!("/api/v1/channels/{}", c.id))).bearer_auth(&a.token).send().await.unwrap();
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM messages").fetch_one(&app.state.db).await.unwrap();
    assert_eq!(n, 0);
}

#[tokio::test]
async fn list_sorted() {
    let app = TestApp::spawn().await;
    let a = app.user("Dimas").await;
    app.create_channel(&a.token, "voice", "Mabar").await;
    app.create_channel(&a.token, "text", "umum").await;
    app.create_channel(&a.token, "text", "clip").await;
    let list: Vec<Channel> = app.client.get(app.url("/api/v1/channels")).bearer_auth(&a.token).send().await.unwrap().json().await.unwrap();
    let names: Vec<_> = list.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, ["umum", "clip", "Mabar"]);
}
