use rynuvo_desktop_lib::core::{http::Api, logs};
use rynuvo_server::testutil::TestApp;

#[tokio::test]
async fn log_tail_redacts_nothing_sensitive() {
    let dir = tempfile::tempdir().unwrap();
    logs::init(dir.path()).unwrap();
    let app = TestApp::spawn().await;
    app.set_invite(TestApp::INVITE).await;
    let base = format!("http://{}", app.addr);
    let resp = Api::new(&base, None).unwrap().join(TestApp::INVITE, "Dimas", "test").await.unwrap();
    let authed = Api::new(&base, Some(resp.token.clone())).unwrap();
    let code = authed.transfer_code().await.unwrap().code;
    authed.me().await.unwrap();

    let tail = logs::collect_tail(1 << 20);
    assert!(tail.contains("/join"), "join call should be logged: {tail}");
    assert!(!tail.contains(&resp.token));
    assert!(!tail.contains(&code));
    assert!(!tail.contains(TestApp::INVITE));
    assert!(!tail.to_lowercase().contains("bearer"));
}
