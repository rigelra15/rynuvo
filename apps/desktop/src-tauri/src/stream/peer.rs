//! Direct UDP path between a streamer and one viewer (hole punching, IPv6 first), relay fallback.
//!
//! Both sides learn their candidates (local address + STUN-mapped address per IP family), swap them
//! through the server, and fire encrypted probes at every remote candidate. The first answered probe
//! becomes the direct path; IPv6 replaces IPv4 when both answer. Until then, and whenever the direct
//! path goes quiet, data goes through the relay (the voice link) instead.
use std::{
    net::{IpAddr, SocketAddr},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU32, Ordering},
    },
    time::{Duration, Instant},
};

use rynuvo_proto::voice_wire::{UP_KEEPALIVE, UP_VIDEO, VoiceKey, open_up, seal_up};
use serde_json::json;
use tokio::{
    net::UdpSocket,
    sync::{mpsc, watch},
};

/// Give up on a direct path this long after the remote candidates arrived.
pub const PUNCH_TIMEOUT: Duration = Duration::from_secs(3);
/// A direct path with no traffic for this long falls back to the relay.
pub const DIRECT_SILENCE: Duration = Duration::from_secs(3);
/// While on the relay, try the direct path again this often.
pub const REPROBE: Duration = Duration::from_secs(10);
const PROBE_EVERY: Duration = Duration::from_millis(100);
const KEEPALIVE: Duration = Duration::from_secs(1);
const STUN_TIMEOUT: Duration = Duration::from_millis(1500);
const PROBE: u8 = 0;
const ACK: u8 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Streamer,
    Viewer,
}

impl Role {
    /// Distinct nonce space per direction: the pair shares one key.
    fn session(self) -> u32 {
        match self {
            Role::Streamer => 1,
            Role::Viewer => 2,
        }
    }
    fn other(self) -> Role {
        match self {
            Role::Streamer => Role::Viewer,
            Role::Viewer => Role::Streamer,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PeerPath {
    Connecting,
    Direct(SocketAddr),
    Relay,
}

impl PeerPath {
    pub fn is_direct(self) -> bool {
        matches!(self, PeerPath::Direct(_))
    }
}

pub type SignalFn = Arc<dyn Fn(serde_json::Value) + Send + Sync>;
pub type RelayFn = Arc<dyn Fn(Vec<u8>) + Send + Sync>;

/// IPv6 first (IndiHome subscribers usually reach each other directly over IPv6), then IPv4.
pub fn order_candidates(mut c: Vec<SocketAddr>) -> Vec<SocketAddr> {
    c.sort_by_key(|a| !a.is_ipv6());
    c.dedup();
    c
}

struct State {
    path: PeerPath,
    remotes: Vec<SocketAddr>,
    remotes_at: Option<Instant>,
    last_direct: Instant,
}

struct Inner {
    role: Role,
    key: VoiceKey,
    seq: AtomicU32,
    st: Mutex<State>,
    socks: Mutex<Vec<Arc<UdpSocket>>>,
    relay: RelayFn,
    rx: mpsc::UnboundedSender<Vec<u8>>,
    stop: watch::Sender<bool>,
}

#[derive(Clone)]
pub struct PeerLink {
    inner: Arc<Inner>,
}

impl PeerLink {
    /// `stun`: the server's STUN addresses (also used to find the outgoing interface per IP family).
    /// Must be called inside a Tokio runtime.
    pub fn start(role: Role, key: VoiceKey, stun: Vec<SocketAddr>, signal: SignalFn, relay: RelayFn, rx: mpsc::UnboundedSender<Vec<u8>>) -> PeerLink {
        let (stop, stop_rx) = watch::channel(false);
        let inner = Arc::new(Inner {
            role,
            key,
            seq: AtomicU32::new(0),
            st: Mutex::new(State { path: PeerPath::Connecting, remotes: Vec::new(), remotes_at: None, last_direct: Instant::now() }),
            socks: Mutex::new(Vec::new()),
            relay,
            rx,
            stop,
        });
        tokio::spawn(run(inner.clone(), stun, signal, stop_rx));
        PeerLink { inner }
    }

    /// The other side's candidates, from `StreamSignal`.
    pub fn on_signal(&self, v: &serde_json::Value) {
        let Some(list) = v.get("cands").and_then(|c| c.as_array()) else { return };
        let addrs: Vec<SocketAddr> = list.iter().filter_map(|s| s.as_str()?.parse().ok()).take(8).collect();
        let mut st = self.inner.st.lock().unwrap();
        let mut all = std::mem::take(&mut st.remotes);
        all.extend(addrs);
        st.remotes = order_candidates(all);
        st.remotes_at.get_or_insert_with(Instant::now);
    }

    /// Data that came through the relay for this pair.
    pub fn on_relay(&self, b: &[u8]) {
        let _ = self.inner.rx.send(b.to_vec());
    }

    pub fn send(&self, b: &[u8]) {
        match self.path() {
            PeerPath::Direct(a) => self.inner.send_to(UP_VIDEO, b, a),
            _ => (self.inner.relay)(b.to_vec()),
        }
    }

    pub fn path(&self) -> PeerPath {
        self.inner.st.lock().unwrap().path
    }

    pub fn stop(&self) {
        let _ = self.inner.stop.send(true);
    }
}

impl Inner {
    fn send_to(&self, kind: u8, payload: &[u8], to: SocketAddr) {
        let seq = self.seq.fetch_add(1, Ordering::Relaxed).wrapping_add(1);
        let pkt = seal_up(&self.key, kind, self.role.session(), seq, payload);
        for s in self.socks.lock().unwrap().iter() {
            if s.local_addr().is_ok_and(|l| l.is_ipv6() == to.is_ipv6()) {
                let _ = s.try_send_to(&pkt, to);
                return;
            }
        }
    }

    fn set_path(&self, p: PeerPath) {
        let mut st = self.st.lock().unwrap();
        if st.path != p {
            match p {
                PeerPath::Direct(a) => tracing::info!("stream: direct path to {a}"),
                PeerPath::Relay => tracing::info!("stream: pair via relay"),
                PeerPath::Connecting => {}
            }
            st.path = p;
        }
    }

    fn receive(&self, pkt: &[u8], from: SocketAddr) {
        let Ok(up) = open_up(&self.key, pkt) else { return };
        if up.session != self.role.other().session() {
            return; // our own packet reflected back, or a stranger
        }
        match up.kind {
            UP_KEEPALIVE if up.payload.first() == Some(&PROBE) => {
                self.send_to(UP_KEEPALIVE, &[ACK], from);
                let mut st = self.st.lock().unwrap();
                if !st.remotes.contains(&from) {
                    // Peer-reflexive: the NAT mapping we were actually reached from.
                    st.remotes.insert(0, from);
                    st.remotes = order_candidates(std::mem::take(&mut st.remotes));
                }
                if st.path == PeerPath::Direct(from) {
                    st.last_direct = Instant::now();
                }
            }
            UP_KEEPALIVE if up.payload.first() == Some(&ACK) => {
                let current = self.st.lock().unwrap().path;
                let better = match current {
                    PeerPath::Direct(a) => a == from || (from.is_ipv6() && !a.is_ipv6()),
                    _ => true,
                };
                if better {
                    self.st.lock().unwrap().last_direct = Instant::now();
                    self.set_path(PeerPath::Direct(from));
                }
            }
            UP_VIDEO => {
                let mut st = self.st.lock().unwrap();
                if st.path == PeerPath::Direct(from) {
                    st.last_direct = Instant::now();
                }
                drop(st);
                let _ = self.rx.send(up.payload);
            }
            _ => {}
        }
    }
}

async fn bind(addr: &str) -> Option<Arc<UdpSocket>> {
    let s = UdpSocket::bind(addr).await.ok()?;
    s.writable().await.ok()?;
    Some(Arc::new(s))
}

/// The local address the OS would use to reach `to` (no packet is sent).
async fn local_ip_towards(to: SocketAddr) -> Option<IpAddr> {
    let c = UdpSocket::bind(if to.is_ipv6() { "[::]:0" } else { "0.0.0.0:0" }).await.ok()?;
    c.connect(to).await.ok()?;
    Some(c.local_addr().ok()?.ip())
}

async fn gather(socks: &[Arc<UdpSocket>], stun: &[SocketAddr]) -> Vec<SocketAddr> {
    let mut out = Vec::new();
    for s in socks {
        let Ok(local) = s.local_addr() else { continue };
        for server in stun.iter().filter(|a| a.is_ipv6() == local.is_ipv6()) {
            if let Some(ip) = local_ip_towards(*server).await {
                out.push(SocketAddr::new(ip, local.port()));
            }
            if !server.ip().is_loopback() {
                if let Ok(Some(m)) = tokio::time::timeout(STUN_TIMEOUT, crate::core::nettest::query(s, *server)).await {
                    out.push(m);
                }
            }
        }
    }
    order_candidates(out)
}

async fn run(inner: Arc<Inner>, stun: Vec<SocketAddr>, signal: SignalFn, mut stop: watch::Receiver<bool>) {
    let socks: Vec<Arc<UdpSocket>> = [bind("[::]:0").await, bind("0.0.0.0:0").await].into_iter().flatten().collect();
    let cands = gather(&socks, &stun).await;
    *inner.socks.lock().unwrap() = socks.clone();
    signal(json!({ "cands": cands.iter().map(|a| a.to_string()).collect::<Vec<_>>() }));

    let (pkt_tx, mut pkt_rx) = mpsc::unbounded_channel::<(Vec<u8>, SocketAddr)>();
    let mut readers = Vec::new();
    for s in socks {
        let tx = pkt_tx.clone();
        readers.push(tokio::spawn(async move {
            let mut buf = vec![0u8; 1500];
            while let Ok((n, from)) = s.recv_from(&mut buf).await {
                if tx.send((buf[..n].to_vec(), from)).is_err() {
                    return;
                }
            }
        }));
    }

    let mut tick = tokio::time::interval(PROBE_EVERY);
    let started = Instant::now();
    let mut last_keepalive = Instant::now();
    let mut last_reprobe = Instant::now();
    loop {
        tokio::select! {
            _ = stop.changed() => break,
            Some((p, from)) = pkt_rx.recv() => inner.receive(&p, from),
            _ = tick.tick() => {
                let (path, remotes, remotes_at, last_direct) = {
                    let st = inner.st.lock().unwrap();
                    (st.path, st.remotes.clone(), st.remotes_at, st.last_direct)
                };
                match path {
                    PeerPath::Connecting => {
                        for r in &remotes {
                            inner.send_to(UP_KEEPALIVE, &[PROBE], *r);
                        }
                        let gave_up = match remotes_at {
                            Some(t) => t.elapsed() > PUNCH_TIMEOUT,
                            None => started.elapsed() > PUNCH_TIMEOUT * 3,
                        };
                        if gave_up {
                            inner.set_path(PeerPath::Relay);
                            last_reprobe = Instant::now();
                        }
                    }
                    PeerPath::Direct(a) => {
                        if last_direct.elapsed() > DIRECT_SILENCE {
                            inner.set_path(PeerPath::Relay);
                            last_reprobe = Instant::now();
                        } else if last_keepalive.elapsed() >= KEEPALIVE {
                            last_keepalive = Instant::now();
                            inner.send_to(UP_KEEPALIVE, &[PROBE], a);
                        }
                    }
                    PeerPath::Relay => {
                        if last_reprobe.elapsed() >= REPROBE {
                            last_reprobe = Instant::now();
                            for r in &remotes {
                                inner.send_to(UP_KEEPALIVE, &[PROBE], *r);
                            }
                        }
                    }
                }
            }
        }
    }
    for r in readers {
        r.abort();
    }
}
