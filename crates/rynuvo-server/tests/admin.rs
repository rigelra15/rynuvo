use rynuvo_proto::{ApiError, JoinResp, RotateInviteResp, TransferCodeResp, User};
use rynuvo_server::testutil::TestApp;
use serde_json::json;

#[tokio::test]
async fn rotate_invite_requires_admin() {
    let app = TestApp::spawn().await;
    let _admin = app.admin().await;
    let u = app.user("Dimas").await;
    let r = app.client.post(app.url("/api/v1/admin/invite")).bearer_auth(&u.token).send().await.unwrap();
    assert_eq!(r.status(), 403);
}

#[tokio::test]
async fn rotate_invite_invalidates_old() {
    let app = TestApp::spawn().await;
    let admin = app.admin().await;
    app.user("Dimas").await; // uses TestApp::INVITE
    let r: RotateInviteResp = app.client.post(app.url("/api/v1/admin/invite")).bearer_auth(&admin.token).send().await.unwrap().json().await.unwrap();
    let old = app.join_raw(TestApp::INVITE, "Ayu").await;
    assert_eq!(old.status(), 403);
    let new = app.join_raw(&r.code, "Ayu").await;
    assert_eq!(new.status(), 200);
}

#[tokio::test]
async fn kick_user_revokes_tokens_and_reassigns_channels() {
    let app = TestApp::spawn().await;
    let admin = app.admin().await;
    let b = app.user("Bayu").await;
    sqlx::query("INSERT INTO channels(kind, name, owner_id, position) VALUES('text','bayu-room',?,1)")
        .bind(b.user.id).execute(&app.state.db).await.unwrap();
    let r = app.client.delete(app.url(&format!("/api/v1/admin/users/{}", b.user.id))).bearer_auth(&admin.token).send().await.unwrap();
    assert_eq!(r.status(), 204);
    let r = app.client.get(app.url("/api/v1/me")).bearer_auth(&b.token).send().await.unwrap();
    assert_eq!(r.status(), 401);
    let owner: i64 = sqlx::query_scalar("SELECT owner_id FROM channels WHERE name='bayu-room'").fetch_one(&app.state.db).await.unwrap();
    assert_eq!(owner, admin.user.id);
    // admin cannot kick self
    let r = app.client.delete(app.url(&format!("/api/v1/admin/users/{}", admin.user.id))).bearer_auth(&admin.token).send().await.unwrap();
    assert_eq!(r.status(), 400);
}

#[tokio::test]
async fn kicked_user_ws_closed_with_4001() {
    let app = TestApp::spawn().await;
    let admin = app.admin().await;
    let b = app.user("Bayu").await;
    let mut ws = app.ws(&b.token).await;
    ws.next().await; // Ready
    app.client.delete(app.url(&format!("/api/v1/admin/users/{}", b.user.id))).bearer_auth(&admin.token).send().await.unwrap();
    assert_eq!(ws.expect_close().await, rynuvo_proto::CLOSE_AUTH_REVOKED);
}

async fn claim(app: &TestApp, code: &str) -> reqwest::Response {
    app.client.post(app.url("/api/v1/claim-transfer")).json(&json!({"code": code, "deviceName": "laptop baru"})).send().await.unwrap()
}

#[tokio::test]
async fn transfer_code_once_and_expires() {
    let app = TestApp::spawn().await;
    let admin = app.admin().await;
    let u = app.user("Dimas").await;
    let t: TransferCodeResp = app.client.post(app.url("/api/v1/me/transfer-code")).bearer_auth(&u.token).send().await.unwrap().json().await.unwrap();
    let r = claim(&app, &t.code).await;
    assert_eq!(r.status(), 200);
    let j: JoinResp = r.json().await.unwrap();
    assert_eq!(j.user.id, u.user.id);
    assert_ne!(j.token, u.token);
    // old device still works
    let me: User = app.client.get(app.url("/api/v1/me")).bearer_auth(&u.token).send().await.unwrap().json().await.unwrap();
    assert_eq!(me.id, u.user.id);
    let r = claim(&app, &t.code).await;
    assert_eq!(r.status(), 403);
    assert_eq!(r.json::<ApiError>().await.unwrap().code, "invalid_code");

    // admin-issued code, then expired
    let t2: TransferCodeResp = app.client.post(app.url(&format!("/api/v1/admin/users/{}/transfer-code", u.user.id))).bearer_auth(&admin.token).send().await.unwrap().json().await.unwrap();
    sqlx::query("UPDATE transfer_codes SET expires_at = 0").execute(&app.state.db).await.unwrap();
    assert_eq!(claim(&app, &t2.code).await.status(), 403);
}

#[tokio::test]
async fn removed_member_can_rejoin_with_same_nickname() {
    let app = TestApp::spawn().await;
    let admin = app.admin().await;
    let u = app.user("Dimas").await;
    let r = app.client.delete(app.url(&format!("/api/v1/admin/users/{}", u.user.id))).bearer_auth(&admin.token).send().await.unwrap();
    assert!(r.status().is_success(), "{}", r.status());
    app.set_invite(TestApp::INVITE).await;
    let r = app.join_raw(TestApp::INVITE, "Dimas").await;
    assert_eq!(r.status(), 200, "nickname of a removed member is free again");
}
