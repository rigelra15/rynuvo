//! Connection test: STUN mapping behaviour (P2P likelihood) and upload speed.
use std::{
    net::{IpAddr, SocketAddr},
    time::{Duration, Instant},
};

use serde::Serialize;
use tokio::net::UdpSocket;

use super::http::Api;

const COOKIE: u32 = 0x2112_A442;
const UPLOAD_BYTES: usize = 4 * 1024 * 1024;

#[derive(Serialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Mapping {
    EndpointIndependent,
    Dependent,
    Unknown,
}

#[derive(Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct NetReport {
    pub ipv4_mapped: [Option<SocketAddr>; 2],
    pub ipv6_mapped: [Option<SocketAddr>; 2],
    pub local_ips: Vec<IpAddr>,
    pub mapping: Mapping,
    pub udp_ok: bool,
    pub ipv6_ok: bool,
    pub upload_mbps: Option<f64>,
}

/// Same public mapping seen from two server ports means the NAT is endpoint-independent.
pub fn classify(a: Option<SocketAddr>, b: Option<SocketAddr>) -> Mapping {
    match (a, b) {
        (Some(a), Some(b)) if a == b => Mapping::EndpointIndependent,
        (Some(_), Some(_)) => Mapping::Dependent,
        _ => Mapping::Unknown,
    }
}

fn binding_request() -> ([u8; 20], [u8; 12]) {
    let mut txid = [0u8; 12];
    let _ = getrandom::fill(&mut txid);
    let mut req = [0u8; 20];
    req[0..2].copy_from_slice(&0x0001u16.to_be_bytes());
    req[4..8].copy_from_slice(&COOKIE.to_be_bytes());
    req[8..20].copy_from_slice(&txid);
    (req, txid)
}

/// Extract XOR-MAPPED-ADDRESS from a Binding Success Response matching `txid`.
fn parse_response(buf: &[u8], txid: &[u8; 12]) -> Option<SocketAddr> {
    if buf.len() < 20 || buf[0..2] != [0x01, 0x01] || buf[4..8] != COOKIE.to_be_bytes() || &buf[8..20] != txid {
        return None;
    }
    let mut key = COOKIE.to_be_bytes().to_vec();
    key.extend_from_slice(txid);
    let mut i = 20;
    while i + 4 <= buf.len() {
        let t = u16::from_be_bytes([buf[i], buf[i + 1]]);
        let l = u16::from_be_bytes([buf[i + 2], buf[i + 3]]) as usize;
        let v = buf.get(i + 4..i + 4 + l)?;
        if t == 0x0020 && l >= 8 {
            let port = u16::from_be_bytes([v[2], v[3]]) ^ (COOKIE >> 16) as u16;
            let ip: IpAddr = match v[1] {
                0x01 => {
                    let o: [u8; 4] = std::array::from_fn(|k| v[4 + k] ^ key[k]);
                    o.into()
                }
                0x02 if l >= 20 => {
                    let o: [u8; 16] = std::array::from_fn(|k| v[4 + k] ^ key[k]);
                    o.into()
                }
                _ => return None,
            };
            return Some(SocketAddr::new(ip, port));
        }
        i += 4 + l.div_ceil(4) * 4;
    }
    None
}

async fn query(sock: &UdpSocket, server: SocketAddr) -> Option<SocketAddr> {
    let (req, txid) = binding_request();
    let mut buf = [0u8; 1500];
    for _ in 0..3 {
        sock.send_to(&req, server).await.ok()?;
        let deadline = Instant::now() + Duration::from_secs(2);
        while let Ok(Ok((n, from))) = tokio::time::timeout_at(deadline.into(), sock.recv_from(&mut buf)).await {
            if from == server {
                if let Some(a) = parse_response(&buf[..n], &txid) {
                    return Some(a);
                }
            }
        }
    }
    None
}

async fn probe(bind: &str, servers: [Option<SocketAddr>; 2]) -> ([Option<SocketAddr>; 2], Option<IpAddr>) {
    let Ok(sock) = UdpSocket::bind(bind).await else { return ([None, None], None) };
    let mut out = [None, None];
    let mut local = None;
    for (i, s) in servers.into_iter().enumerate() {
        if let Some(s) = s {
            out[i] = query(&sock, s).await;
            if local.is_none() {
                // The OS picks the outgoing interface on connect; a second socket avoids disturbing `sock`.
                if let Ok(c) = UdpSocket::bind(bind).await {
                    if c.connect(s).await.is_ok() {
                        local = c.local_addr().ok().map(|a| a.ip());
                    }
                }
            }
        }
    }
    (out, local)
}

async fn resolve(host: &str, ports: [u16; 2]) -> ([Option<SocketAddr>; 2], [Option<SocketAddr>; 2]) {
    let mut v4 = [None, None];
    let mut v6 = [None, None];
    for (i, port) in ports.into_iter().enumerate() {
        if let Ok(addrs) = tokio::net::lookup_host((host, port)).await {
            for a in addrs {
                match a {
                    SocketAddr::V4(_) if v4[i].is_none() => v4[i] = Some(a),
                    SocketAddr::V6(_) if v6[i].is_none() => v6[i] = Some(a),
                    _ => {}
                }
            }
        }
    }
    (v4, v6)
}

async fn upload(api: &Api) -> Option<f64> {
    let start = Instant::now();
    api.upload_test(vec![0u8; UPLOAD_BYTES]).await.ok()?;
    let secs = start.elapsed().as_secs_f64().max(1e-3);
    Some((UPLOAD_BYTES as f64 * 8.0 / secs / 1e6 * 10.0).round() / 10.0)
}

pub async fn run(api: &Api, stun_host: &str, ports: [u16; 2]) -> NetReport {
    let (v4s, v6s) = resolve(stun_host, ports).await;
    let ((ipv4_mapped, l4), (ipv6_mapped, l6), upload_mbps) =
        tokio::join!(probe("0.0.0.0:0", v4s), probe("[::]:0", v6s), upload(api));
    let mapping = match classify(ipv4_mapped[0], ipv4_mapped[1]) {
        Mapping::Unknown => classify(ipv6_mapped[0], ipv6_mapped[1]),
        m => m,
    };
    let ipv6_ok = ipv6_mapped.iter().any(Option::is_some);
    NetReport {
        udp_ok: ipv6_ok || ipv4_mapped.iter().any(Option::is_some),
        ipv4_mapped,
        ipv6_mapped,
        local_ips: [l4, l6].into_iter().flatten().collect(),
        mapping,
        ipv6_ok,
        upload_mbps,
    }
}
