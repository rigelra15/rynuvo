//! Media relay: decrypts a member's packet with their key and re-encrypts it for every listener.
use std::{
    net::SocketAddr,
    sync::{
        Arc, OnceLock,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

use rynuvo_proto::voice_wire::{DOWN_AUDIO, DOWN_KEEPALIVE_ACK, UP_AUDIO, UP_KEEPALIVE, open_up, peek_up, seal_down};
use tokio::net::UdpSocket;

use super::{VoiceRooms, rooms::Route};
use crate::ws::{Hub, hub::Outgoing};

/// Largest Opus payload relayed (a 20 ms frame at 64 kbps is ~160 bytes).
pub const MAX_AUDIO: usize = 1000;
/// A member whose route is older than this is not sent to.
pub const ROUTE_TTL: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Udp(SocketAddr),
    Ws(u64),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dest {
    Udp(SocketAddr),
    Ws(u64),
}

static DROPPED: AtomicU64 = AtomicU64::new(0);

fn drop_packet() -> Vec<(Dest, Vec<u8>)> {
    DROPPED.fetch_add(1, Ordering::Relaxed);
    Vec::new()
}

fn dest_of(route: Route, now: Instant) -> Option<Dest> {
    match route {
        Route::Udp(a, t) if now.duration_since(t) <= ROUTE_TTL => Some(Dest::Udp(a)),
        // The conn id is filled in by the caller, which knows the member.
        Route::Ws(t) if now.duration_since(t) <= ROUTE_TTL => Some(Dest::Ws(0)),
        _ => None,
    }
}

/// Process one packet from a client and return what to send where. Pure with respect to I/O.
pub fn handle_packet(rooms: &VoiceRooms, pkt: &[u8], from: Source, now: Instant) -> Vec<(Dest, Vec<u8>)> {
    let Some((_, session)) = peek_up(pkt) else { return drop_packet() };
    rooms.with(|members| {
        let Some(i) = members.iter().position(|m| m.session == session) else { return drop_packet() };
        let Ok(up) = open_up(&members[i].key, pkt) else { return drop_packet() };
        if let Source::Ws(conn) = from {
            if conn != members[i].conn_id {
                return drop_packet();
            }
        }
        let reply_to = match from {
            Source::Udp(a) => {
                members[i].route = Route::Udp(a, now);
                Dest::Udp(a)
            }
            Source::Ws(c) => {
                members[i].route = Route::Ws(now);
                Dest::Ws(c)
            }
        };
        members[i].bytes_in += pkt.len() as u64;
        let mut out = Vec::new();
        match up.kind {
            UP_KEEPALIVE => {
                let m = &mut members[i];
                m.down_seq = m.down_seq.wrapping_add(1);
                let ack = seal_down(&m.key, DOWN_KEEPALIVE_ACK, m.session, m.down_seq, 0, &[]);
                m.bytes_out += ack.len() as u64;
                out.push((reply_to, ack));
            }
            UP_AUDIO => {
                if members[i].muted || up.payload.len() > MAX_AUDIO {
                    return out;
                }
                let (from_user, channel) = (members[i].user_id, members[i].channel_id);
                for m in members.iter_mut().filter(|m| m.channel_id == channel && m.user_id != from_user && !m.deafened) {
                    let dest = match dest_of(m.route, now) {
                        Some(Dest::Ws(_)) => Dest::Ws(m.conn_id),
                        Some(d) => d,
                        None => continue,
                    };
                    m.down_seq = m.down_seq.wrapping_add(1);
                    let p = seal_down(&m.key, DOWN_AUDIO, m.session, m.down_seq, from_user, &up.payload);
                    m.bytes_out += p.len() as u64;
                    out.push((dest, p));
                }
            }
            _ => {}
        }
        out
    })
}

/// Shared relay handle: owns the UDP socket once the STUN port binds, and the hub for WS delivery.
#[derive(Clone)]
pub struct Relay {
    pub rooms: VoiceRooms,
    hub: Hub,
    udp: Arc<OnceLock<Arc<UdpSocket>>>,
}

impl Relay {
    pub fn new(rooms: VoiceRooms, hub: Hub) -> Self {
        Relay { rooms, hub, udp: Arc::default() }
    }

    pub fn set_udp(&self, sock: Arc<UdpSocket>) {
        let _ = self.udp.set(sock);
    }

    pub async fn deliver(&self, out: Vec<(Dest, Vec<u8>)>) {
        for (d, p) in out {
            match d {
                Dest::Udp(a) => {
                    if let Some(s) = self.udp.get() {
                        let _ = s.send_to(&p, a).await;
                    }
                }
                Dest::Ws(c) => self.hub.send_to_conn(c, Outgoing::Bin(p)),
            }
        }
    }

    pub async fn from_udp(&self, pkt: &[u8], from: SocketAddr) {
        let out = handle_packet(&self.rooms, pkt, Source::Udp(from), Instant::now());
        self.deliver(out).await;
    }

    pub async fn from_ws(&self, conn_id: u64, pkt: &[u8]) {
        let out = handle_packet(&self.rooms, pkt, Source::Ws(conn_id), Instant::now());
        self.deliver(out).await;
    }

    /// Log the count of dropped packets once a minute (never per packet).
    pub fn spawn_drop_logger() {
        tokio::spawn(async {
            let mut t = tokio::time::interval(Duration::from_secs(60));
            loop {
                t.tick().await;
                let n = DROPPED.swap(0, Ordering::Relaxed);
                if n > 0 {
                    tracing::info!(dropped = n, "voice relay dropped invalid packets in the last minute");
                }
            }
        });
    }
}
