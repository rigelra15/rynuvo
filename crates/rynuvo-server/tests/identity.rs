use rynuvo_proto::{ApiError, JoinResp, User};
use rynuvo_server::{routes::identity::ensure_owner_code, testutil::TestApp};
use serde_json::json;

async fn join_raw(app: &TestApp, code: &str, nick: &str) -> reqwest::Response {
    app.client
        .post(app.url("/api/v1/join"))
        .json(&json!({"inviteCode": code, "nickname": nick, "deviceName": "test"}))
        .send()
        .await
        .unwrap()
}

#[tokio::test]
async fn owner_code_makes_admin() {
    let app = TestApp::spawn().await;
    let code = ensure_owner_code(&app.state.db).await.unwrap().expect("first run prints code");
    assert!(ensure_owner_code(&app.state.db).await.unwrap().is_none(), "only once");
    let r = join_raw(&app, &code, "Rigel").await;
    assert_eq!(r.status(), 200);
    let j: JoinResp = r.json().await.unwrap();
    assert!(j.user.is_admin);
    let r2 = join_raw(&app, &code, "Other").await;
    assert_eq!(r2.status(), 403);
    assert_eq!(r2.json::<ApiError>().await.unwrap().code, "invalid_invite");
}

#[tokio::test]
async fn join_with_invite() {
    let app = TestApp::spawn().await;
    app.set_invite("ABCD-EFGH-JKLM").await;
    let r = join_raw(&app, "abcd efgh jklm", "Dimas").await;
    assert_eq!(r.status(), 200);
    let j: JoinResp = r.json().await.unwrap();
    assert!(j.token.len() >= 43);
    assert!(!j.user.is_admin);
}

#[tokio::test]
async fn wrong_invite_rejected() {
    let app = TestApp::spawn().await;
    app.set_invite("ABCD-EFGH-JKLM").await;
    let r = join_raw(&app, "ZZZZ-ZZZZ-ZZZZ", "Dimas").await;
    assert_eq!(r.status(), 403);
    assert_eq!(r.json::<ApiError>().await.unwrap().code, "invalid_invite");
}

#[tokio::test]
async fn nickname_rules() {
    let app = TestApp::spawn().await;
    app.set_invite("ABCD-EFGH-JKLM").await;
    let j: JoinResp = join_raw(&app, "ABCD-EFGH-JKLM", "  Dimas ").await.json().await.unwrap();
    assert_eq!(j.user.nickname, "Dimas");
    let r = join_raw(&app, "ABCD-EFGH-JKLM", "dimas").await;
    assert_eq!(r.status(), 409);
    assert_eq!(r.json::<ApiError>().await.unwrap().code, "nickname_taken");
    for bad in ["a", &"x".repeat(33), "\u{0007}x", "   "] {
        let r = join_raw(&app, "ABCD-EFGH-JKLM", bad).await;
        assert_eq!(r.status(), 400, "nickname {bad:?}");
    }
    let r = join_raw(&app, "ABCD-EFGH-JKLM", &"y".repeat(32)).await;
    assert_eq!(r.status(), 200);
}

#[tokio::test]
async fn me_requires_token() {
    let app = TestApp::spawn().await;
    app.set_invite("ABCD-EFGH-JKLM").await;
    let j: JoinResp = join_raw(&app, "ABCD-EFGH-JKLM", "Dimas").await.json().await.unwrap();
    let r = app.client.get(app.url("/api/v1/me")).send().await.unwrap();
    assert_eq!(r.status(), 401);
    let r = app.client.get(app.url("/api/v1/me")).bearer_auth("nope").send().await.unwrap();
    assert_eq!(r.status(), 401);
    let me: User = app.client.get(app.url("/api/v1/me")).bearer_auth(&j.token).send().await.unwrap().json().await.unwrap();
    assert_eq!(me.nickname, "Dimas");
}

#[tokio::test]
async fn rename_me() {
    let app = TestApp::spawn().await;
    app.set_invite("ABCD-EFGH-JKLM").await;
    let a: JoinResp = join_raw(&app, "ABCD-EFGH-JKLM", "Dimas").await.json().await.unwrap();
    join_raw(&app, "ABCD-EFGH-JKLM", "Ayu").await;
    let r = app.client.patch(app.url("/api/v1/me")).bearer_auth(&a.token).json(&json!({"nickname":"AYU"})).send().await.unwrap();
    assert_eq!(r.status(), 409);
    let r = app.client.patch(app.url("/api/v1/me")).bearer_auth(&a.token).json(&json!({"nickname":"Bayu"})).send().await.unwrap();
    assert_eq!(r.status(), 200);
    assert_eq!(r.json::<User>().await.unwrap().nickname, "Bayu");
}
