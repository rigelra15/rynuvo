//! Network link to the voice relay: UDP first, WebSocket binary frames when UDP is silent.
use std::{
    net::SocketAddr,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU8, AtomicU32, Ordering},
    },
    time::{Duration, Instant},
};

use rynuvo_proto::{
    Id,
    voice_wire::{DOWN_AUDIO, DOWN_KEEPALIVE_ACK, UP_AUDIO, UP_KEEPALIVE, VoiceKey, open_down, seal_up},
};
use serde::Serialize;
use tokio::{
    net::UdpSocket,
    sync::{mpsc, watch},
};

/// No UDP acknowledgement for this long → switch to the WebSocket path.
pub const UDP_SILENCE: Duration = Duration::from_secs(3);
/// While on the WebSocket path, try UDP again this often.
pub const UDP_REPROBE: Duration = Duration::from_secs(10);
const KEEPALIVE: Duration = Duration::from_secs(1);

pub type WsSender = Arc<dyn Fn(Vec<u8>) + Send + Sync>;

#[derive(Debug, Clone)]
pub struct LinkCfg {
    pub host: String,
    pub udp_port: u16,
    pub session: u32,
    pub key: VoiceKey,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownAudio {
    pub from: Id,
    pub seq: u32,
    pub payload: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum LinkPath {
    Udp,
    Ws,
}

struct Inner {
    session: u32,
    key: VoiceKey,
    seq: AtomicU32,
    /// 0 = UDP, 1 = WebSocket
    path: AtomicU8,
    udp: Mutex<Option<(Arc<UdpSocket>, SocketAddr)>>,
    ws: WsSender,
    down: mpsc::UnboundedSender<DownAudio>,
    last_udp_ack: Mutex<Instant>,
    stop: watch::Sender<bool>,
}

#[derive(Clone)]
pub struct VoiceLink {
    inner: Arc<Inner>,
}

impl VoiceLink {
    pub fn start(cfg: LinkCfg, ws: WsSender, down: mpsc::UnboundedSender<DownAudio>) -> VoiceLink {
        let (stop, stop_rx) = watch::channel(false);
        let inner = Arc::new(Inner {
            session: cfg.session,
            key: cfg.key.clone(),
            seq: AtomicU32::new(0),
            path: AtomicU8::new(0),
            udp: Mutex::new(None),
            ws,
            down,
            last_udp_ack: Mutex::new(Instant::now()),
            stop,
        });
        tokio::spawn(run(inner.clone(), cfg.host, cfg.udp_port, stop_rx));
        VoiceLink { inner }
    }

    pub fn path(&self) -> LinkPath {
        if self.inner.path.load(Ordering::Relaxed) == 0 { LinkPath::Udp } else { LinkPath::Ws }
    }

    pub fn send_audio(&self, opus: Vec<u8>) {
        self.inner.send(UP_AUDIO, &opus, self.path());
    }

    /// A binary WebSocket frame from the server (the WebSocket path's downlink).
    pub fn feed_ws(&self, pkt: &[u8]) {
        self.inner.receive(pkt, false);
    }

    pub fn stop(&self) {
        let _ = self.inner.stop.send(true);
    }
}

impl Inner {
    fn next_packet(&self, kind: u8, payload: &[u8]) -> Vec<u8> {
        let seq = self.seq.fetch_add(1, Ordering::Relaxed).wrapping_add(1);
        seal_up(&self.key, kind, self.session, seq, payload)
    }

    fn send(&self, kind: u8, payload: &[u8], path: LinkPath) {
        let pkt = self.next_packet(kind, payload);
        match path {
            LinkPath::Udp => {
                // The socket is connected: plain send (sendto on a connected socket fails on macOS).
                if let Some((s, _)) = self.udp.lock().unwrap().as_ref() {
                    let _ = s.try_send(&pkt);
                }
            }
            LinkPath::Ws => (self.ws)(pkt),
        }
    }

    fn set_path(&self, p: LinkPath) {
        let v = if p == LinkPath::Udp { 0 } else { 1 };
        if self.path.swap(v, Ordering::Relaxed) != v {
            tracing::info!("voice path is now {p:?}");
        }
    }

    fn receive(&self, pkt: &[u8], via_udp: bool) {
        let Ok(d) = open_down(&self.key, pkt) else { return };
        match d.kind {
            DOWN_KEEPALIVE_ACK if via_udp => {
                *self.last_udp_ack.lock().unwrap() = Instant::now();
                self.set_path(LinkPath::Udp);
            }
            DOWN_AUDIO => {
                let _ = self.down.send(DownAudio { from: d.from_user, seq: d.seq, payload: d.payload });
            }
            _ => {}
        }
    }
}

async fn bind_udp(host: &str, port: u16) -> Option<(Arc<UdpSocket>, SocketAddr)> {
    let addrs: Vec<SocketAddr> = tokio::net::lookup_host((host, port)).await.ok()?.collect();
    // Prefer IPv6 (IndiHome gives every subscriber IPv6; IPv4 is often CGNAT).
    for to in addrs.iter().filter(|a| a.is_ipv6()).chain(addrs.iter().filter(|a| a.is_ipv4())) {
        let local = if to.is_ipv6() { "[::]:0" } else { "0.0.0.0:0" };
        if let Ok(s) = UdpSocket::bind(local).await {
            if s.connect(to).await.is_ok() {
                return Some((Arc::new(s), *to));
            }
        }
    }
    None
}

async fn run(inner: Arc<Inner>, host: String, port: u16, mut stop: watch::Receiver<bool>) {
    let udp = bind_udp(&host, port).await;
    if udp.is_none() {
        tracing::warn!("voice: no UDP route to {host}:{port}, using WebSocket");
        inner.set_path(LinkPath::Ws);
    }
    *inner.udp.lock().unwrap() = udp.clone();
    let mut tick = tokio::time::interval(KEEPALIVE);
    let mut last_probe = Instant::now();
    let mut buf = vec![0u8; 1500];
    loop {
        let recv = async {
            match &udp {
                Some((s, _)) => s.recv(&mut buf).await.ok(),
                None => std::future::pending().await,
            }
        };
        tokio::select! {
            _ = stop.changed() => return,
            n = recv => if let Some(n) = n { inner.receive(&buf[..n], true) },
            _ = tick.tick() => {
                let since_ack = inner.last_udp_ack.lock().unwrap().elapsed();
                let path = if udp.is_some() && since_ack <= UDP_SILENCE { LinkPath::Udp } else { LinkPath::Ws };
                inner.set_path(path);
                match path {
                    LinkPath::Udp => inner.send(UP_KEEPALIVE, &[], LinkPath::Udp),
                    LinkPath::Ws => {
                        inner.send(UP_KEEPALIVE, &[], LinkPath::Ws);
                        if udp.is_some() && last_probe.elapsed() >= UDP_REPROBE {
                            last_probe = Instant::now();
                            inner.send(UP_KEEPALIVE, &[], LinkPath::Udp);
                        }
                    }
                }
            }
        }
    }
}
