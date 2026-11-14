use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use rynuvo_desktop_lib::core::{
    http::Api,
    nettest::{self, Mapping},
};
use rynuvo_server::{stun, testutil::TestApp};
use tokio::net::UdpSocket;

fn sa(s: &str) -> Option<SocketAddr> {
    Some(s.parse().unwrap())
}

#[test]
fn classify_eim() {
    assert_eq!(nettest::classify(sa("1.2.3.4:5000"), sa("1.2.3.4:5000")), Mapping::EndpointIndependent);
}

#[test]
fn classify_dependent() {
    assert_eq!(nettest::classify(sa("1.2.3.4:5000"), sa("1.2.3.4:5001")), Mapping::Dependent);
}

#[test]
fn classify_unknown_on_missing() {
    assert_eq!(nettest::classify(sa("1.2.3.4:5000"), None), Mapping::Unknown);
    assert_eq!(nettest::classify(None, None), Mapping::Unknown);
}

#[tokio::test]
async fn nettest_against_local_server() {
    let app = TestApp::spawn().await;
    let admin = app.admin().await;
    let api = Api::new(&format!("http://{}", app.addr), Some(admin.token)).unwrap();
    let mut ports = [0u16; 2];
    for p in &mut ports {
        let s = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        *p = s.local_addr().unwrap().port();
        stun::spawn_on(s);
    }
    let r = nettest::run(&api, "127.0.0.1", ports).await;
    assert!(r.udp_ok, "{r:?}");
    assert_eq!(r.ipv4_mapped[0].unwrap().ip(), IpAddr::V4(Ipv4Addr::LOCALHOST));
    assert_eq!(r.mapping, Mapping::EndpointIndependent);
    assert!(r.upload_mbps.is_some_and(|m| m > 0.0), "{r:?}");
}
