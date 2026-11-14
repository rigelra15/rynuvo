use rynuvo_server::{config::Config, testutil::TestApp};
use std::path::Path;

#[tokio::test]
async fn health_ok() {
    let app = TestApp::spawn().await;
    let v: serde_json::Value = app.client.get(app.url("/api/v1/health")).send().await.unwrap().json().await.unwrap();
    assert_eq!(v, serde_json::json!({"ok": true, "protocol": rynuvo_proto::PROTOCOL_VERSION}));
}

#[test]
fn config_parses_example() {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../deploy/config.example.toml");
    let c = Config::load(&p).unwrap();
    assert_eq!(c.bind.port(), 8110);
    assert_eq!(c.stun_ports, [3478, 3479]);
}
