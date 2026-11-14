//! Minimal STUN (RFC 5389) Binding responder used for client connectivity tests.
use std::net::{IpAddr, SocketAddr};

use tokio::{net::UdpSocket, task::JoinHandle};

const COOKIE: u32 = 0x2112_A442;

/// Build a Binding Success Response for `req` received from `from`. None if `req` is not a Binding Request.
pub fn binding_response(req: &[u8], from: SocketAddr) -> Option<Vec<u8>> {
    if req.len() < 20 || req[0] & 0xC0 != 0 {
        return None;
    }
    let msg_type = u16::from_be_bytes([req[0], req[1]]);
    let len = u16::from_be_bytes([req[2], req[3]]) as usize;
    let cookie = u32::from_be_bytes([req[4], req[5], req[6], req[7]]);
    if msg_type != 0x0001 || cookie != COOKIE || len + 20 != req.len() {
        return None;
    }
    let txid = &req[8..20];

    let ip = match from.ip() {
        IpAddr::V6(v6) => v6.to_canonical(),
        v4 => v4,
    };
    let mut key = COOKIE.to_be_bytes().to_vec();
    key.extend_from_slice(txid);
    let xport = from.port() ^ (COOKIE >> 16) as u16;
    let (family, addr): (u8, Vec<u8>) = match ip {
        IpAddr::V4(a) => (0x01, a.octets().iter().zip(&key).map(|(b, k)| b ^ k).collect()),
        IpAddr::V6(a) => (0x02, a.octets().iter().zip(&key).map(|(b, k)| b ^ k).collect()),
    };
    let mut attr = vec![0x00, family];
    attr.extend_from_slice(&xport.to_be_bytes());
    attr.extend_from_slice(&addr);

    let mut out = Vec::with_capacity(20 + 4 + attr.len());
    out.extend_from_slice(&0x0101u16.to_be_bytes());
    out.extend_from_slice(&((4 + attr.len()) as u16).to_be_bytes());
    out.extend_from_slice(&COOKIE.to_be_bytes());
    out.extend_from_slice(txid);
    out.extend_from_slice(&0x0020u16.to_be_bytes());
    out.extend_from_slice(&(attr.len() as u16).to_be_bytes());
    out.extend_from_slice(&attr);
    Some(out)
}

/// Serve STUN Binding on an already-bound socket; voice packets on it go to `relay`.
pub fn spawn_on(sock: UdpSocket, relay: Option<crate::voice::relay::Relay>) -> JoinHandle<()> {
    let sock = std::sync::Arc::new(sock);
    if let Some(r) = &relay {
        r.set_udp(sock.clone());
    }
    tokio::spawn(async move {
        let mut buf = [0u8; 1500];
        loop {
            let Ok((n, from)) = sock.recv_from(&mut buf).await else { continue };
            if let Some(resp) = binding_response(&buf[..n], from) {
                let _ = sock.send_to(&resp, from).await;
            } else if let Some(r) = &relay {
                r.from_udp(&buf[..n], from).await;
            }
        }
    })
}

/// Bind dual-stack UDP sockets on both ports and serve STUN. Voice media shares the first port.
pub fn spawn(ports: [u16; 2], relay: Option<crate::voice::relay::Relay>) -> anyhow::Result<()> {
    for (i, port) in ports.into_iter().enumerate() {
        let s = socket2::Socket::new(socket2::Domain::IPV6, socket2::Type::DGRAM, Some(socket2::Protocol::UDP))?;
        s.set_only_v6(false)?;
        s.set_nonblocking(true)?;
        s.set_recv_buffer_size(1 << 20)?;
        s.bind(&SocketAddr::from((std::net::Ipv6Addr::UNSPECIFIED, port)).into())?;
        let sock = UdpSocket::from_std(s.into())?;
        tracing::info!("STUN listening on udp/{port}{}", if i == 0 { " (and voice)" } else { "" });
        spawn_on(sock, if i == 0 { relay.clone() } else { None });
    }
    Ok(())
}
