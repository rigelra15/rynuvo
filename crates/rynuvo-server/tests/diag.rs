use rynuvo_server::{stun, testutil::TestApp};
use serde_json::json;
use std::net::SocketAddr;

const COOKIE: u32 = 0x2112A442;

fn binding_request(txid: [u8; 12]) -> Vec<u8> {
    let mut v = vec![0x00, 0x01, 0x00, 0x00];
    v.extend_from_slice(&COOKIE.to_be_bytes());
    v.extend_from_slice(&txid);
    v
}

/// Decode XOR-MAPPED-ADDRESS from a Binding Success Response.
fn decode_xor_mapped(resp: &[u8], txid: [u8; 12]) -> SocketAddr {
    assert_eq!(&resp[0..2], &[0x01, 0x01], "binding success");
    assert_eq!(&resp[8..20], &txid);
    let len = u16::from_be_bytes([resp[2], resp[3]]) as usize;
    let mut i = 20;
    while i < 20 + len {
        let t = u16::from_be_bytes([resp[i], resp[i + 1]]);
        let l = u16::from_be_bytes([resp[i + 2], resp[i + 3]]) as usize;
        let val = &resp[i + 4..i + 4 + l];
        if t == 0x0020 {
            let port = u16::from_be_bytes([val[2], val[3]]) ^ (COOKIE >> 16) as u16;
            let mut key = COOKIE.to_be_bytes().to_vec();
            key.extend_from_slice(&txid);
            let ip: std::net::IpAddr = match val[1] {
                0x01 => {
                    let b: Vec<u8> = val[4..8].iter().zip(&key).map(|(a, k)| a ^ k).collect();
                    std::net::Ipv4Addr::new(b[0], b[1], b[2], b[3]).into()
                }
                0x02 => {
                    let b: Vec<u8> = val[4..20].iter().zip(&key).map(|(a, k)| a ^ k).collect();
                    let arr: [u8; 16] = b.try_into().unwrap();
                    std::net::Ipv6Addr::from(arr).into()
                }
                f => panic!("family {f}"),
            };
            return SocketAddr::new(ip, port);
        }
        i += 4 + l.div_ceil(4) * 4;
    }
    panic!("no XOR-MAPPED-ADDRESS");
}

#[test]
fn stun_xor_mapped_ipv4() {
    let tx = [7u8; 12];
    let from: SocketAddr = "203.0.113.5:54321".parse().unwrap();
    let r = stun::binding_response(&binding_request(tx), from).unwrap();
    assert_eq!(decode_xor_mapped(&r, tx), from);
}

#[test]
fn stun_xor_mapped_ipv6() {
    let tx = [9u8; 12];
    let from: SocketAddr = "[2001:db8::1234]:40000".parse().unwrap();
    let r = stun::binding_response(&binding_request(tx), from).unwrap();
    assert_eq!(decode_xor_mapped(&r, tx), from);
}

#[test]
fn stun_ignores_garbage() {
    let from: SocketAddr = "1.2.3.4:5".parse().unwrap();
    assert!(stun::binding_response(b"hello", from).is_none());
    let mut bad = binding_request([1; 12]);
    bad[4] = 0; // wrong cookie
    assert!(stun::binding_response(&bad, from).is_none());
    let mut not_req = binding_request([1; 12]);
    not_req[1] = 0x11; // indication
    assert!(stun::binding_response(&not_req, from).is_none());
}

#[tokio::test]
async fn stun_udp_roundtrip() {
    let server = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let addr = server.local_addr().unwrap();
    stun::spawn_on(server, None);
    let client = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let tx = [3u8; 12];
    client.send_to(&binding_request(tx), addr).await.unwrap();
    let mut buf = [0u8; 512];
    let (n, _) = tokio::time::timeout(std::time::Duration::from_secs(2), client.recv_from(&mut buf)).await.unwrap().unwrap();
    assert_eq!(decode_xor_mapped(&buf[..n], tx), client.local_addr().unwrap());
}

#[tokio::test]
async fn report_roundtrip_admin_only() {
    let app = TestApp::spawn().await;
    let admin = app.admin().await;
    let u = app.user("Dimas").await;
    let r = app.client.post(app.url("/api/v1/diag/report")).bearer_auth(&u.token).json(&json!({"kind":"nettest","body":"{\"udpOk\":true}"})).send().await.unwrap();
    assert_eq!(r.status(), 204);
    let r = app.client.post(app.url("/api/v1/diag/report")).bearer_auth(&u.token).json(&json!({"kind":"other","body":"x"})).send().await.unwrap();
    assert_eq!(r.status(), 400);
    let r = app.client.get(app.url("/api/v1/diag/reports?kind=nettest")).bearer_auth(&u.token).send().await.unwrap();
    assert_eq!(r.status(), 403);
    let v: serde_json::Value = app.client.get(app.url("/api/v1/diag/reports?kind=nettest")).bearer_auth(&admin.token).send().await.unwrap().json().await.unwrap();
    assert_eq!(v[0]["nickname"], "Dimas");
    assert_eq!(v[0]["body"], "{\"udpOk\":true}");
}

#[tokio::test]
async fn upload_test_counts_bytes() {
    let app = TestApp::spawn().await;
    let u = app.user("Dimas").await;
    let v: serde_json::Value = app.client.post(app.url("/api/v1/diag/upload-test")).bearer_auth(&u.token).body(vec![0u8; 1_000_000]).send().await.unwrap().json().await.unwrap();
    assert_eq!(v["bytes"], 1_000_000);
}

#[tokio::test]
async fn downloads_served_without_auth() {
    let app = TestApp::spawn().await;
    std::fs::create_dir_all(app.state.cfg.downloads_dir()).unwrap();
    std::fs::write(app.state.cfg.downloads_dir().join("hello.txt"), "hi").unwrap();
    let r = app.client.get(app.url("/downloads/hello.txt")).send().await.unwrap();
    assert_eq!(r.status(), 200);
    assert_eq!(r.text().await.unwrap(), "hi");
}

#[tokio::test]
async fn reports_list_truncates_large_bodies() {
    let app = TestApp::spawn().await;
    let admin = app.admin().await;
    let u = app.user("Dimas").await;
    let big = "L".repeat(1_000_000);
    let r = app.client.post(app.url("/api/v1/diag/report")).bearer_auth(&u.token).json(&json!({"kind":"log","body":big})).send().await.unwrap();
    assert_eq!(r.status(), 204);
    let v: serde_json::Value = app.client.get(app.url("/api/v1/diag/reports")).bearer_auth(&admin.token).send().await.unwrap().json().await.unwrap();
    assert!(v[0]["body"].as_str().unwrap().len() <= 4096);
    assert_eq!(v[0]["size"], 1_000_000);
    let id = v[0]["id"].as_i64().unwrap();
    let full: serde_json::Value = app.client.get(app.url(&format!("/api/v1/diag/reports/{id}"))).bearer_auth(&admin.token).send().await.unwrap().json().await.unwrap();
    assert_eq!(full["body"].as_str().unwrap().len(), 1_000_000);
    let r = app.client.get(app.url(&format!("/api/v1/diag/reports/{id}"))).bearer_auth(&u.token).send().await.unwrap();
    assert_eq!(r.status(), 403);
}

#[tokio::test]
async fn report_retention_keeps_last_20_per_user_and_kind() {
    let app = TestApp::spawn().await;
    let admin = app.admin().await;
    let u = app.user("Dimas").await;
    for i in 0..25 {
        app.client.post(app.url("/api/v1/diag/report")).bearer_auth(&u.token).json(&json!({"kind":"log","body":format!("log {i}")})).send().await.unwrap();
    }
    let v: serde_json::Value = app.client.get(app.url("/api/v1/diag/reports?kind=log")).bearer_auth(&admin.token).send().await.unwrap().json().await.unwrap();
    let bodies: Vec<&str> = v.as_array().unwrap().iter().map(|r| r["body"].as_str().unwrap()).collect();
    assert_eq!(bodies.len(), 20);
    assert_eq!(bodies[0], "log 24");
    assert_eq!(bodies[19], "log 5");
}

#[tokio::test]
async fn report_rejected_when_disk_blocked() {
    let app = TestApp::spawn().await;
    let u = app.user("Dimas").await;
    rynuvo_server::disk::check_now(&app.state.disk, 95);
    let r = app.client.post(app.url("/api/v1/diag/report")).bearer_auth(&u.token).json(&json!({"kind":"log","body":"x"})).send().await.unwrap();
    assert_eq!(r.status(), 507);
}
