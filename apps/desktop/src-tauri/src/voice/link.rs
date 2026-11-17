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
    e2ee::{KIND_VOICE, Scope},
    voice_wire::{DOWN_AUDIO, DOWN_KEEPALIVE_ACK, DOWN_VIDEO, UP_AUDIO, UP_KEEPALIVE, UP_VIDEO, VoiceKey, open_down, seal_up},
};
use serde::Serialize;
use tokio::{
    net::UdpSocket,
    sync::{mpsc, watch},
};

use crate::e2ee::SharedKeys;

/// No UDP acknowledgement for this long → switch to the WebSocket path.
pub const UDP_SILENCE: Duration = Duration::from_secs(3);
/// While on the WebSocket path, try UDP again this often.
pub const UDP_REPROBE: Duration = Duration::from_secs(10);
const KEEPALIVE: Duration = Duration::from_secs(1);
/// No acknowledgement on any path for this long → the network changed under us: rebind UDP and
/// ask the session to reconnect its WebSocket.
pub const LINK_DEAD: Duration = Duration::from_secs(5);

pub type OnDead = Arc<dyn Fn() + Send + Sync>;

pub type WsSender = Arc<dyn Fn(Vec<u8>) + Send + Sync>;
/// Relayed Go Live data: (other user, payload).
pub type VideoSink = Arc<dyn Fn(Id, &[u8]) + Send + Sync>;

#[derive(Clone)]
pub struct LinkCfg {
    pub host: String,
    pub udp_port: u16,
    pub session: u32,
    pub key: VoiceKey,
    /// Media keys for end-to-end encrypted audio in voice channel `channel`. None: nothing is sent or
    /// played, unless `unencrypted_for_tests`.
    pub keys: Option<SharedKeys>,
    pub channel: Id,
    /// Tests only (relay-level link checks): with no `keys`, audio is passed as is.
    #[doc(hidden)]
    pub unencrypted_for_tests: bool,
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
    keys: Option<SharedKeys>,
    unencrypted: bool,
    scope: Scope,
    seq: AtomicU32,
    audio_seq: super::jitter::AudioClock,
    /// 0 = UDP, 1 = WebSocket
    path: AtomicU8,
    udp: Mutex<Option<(Arc<UdpSocket>, SocketAddr)>>,
    ws: WsSender,
    down: mpsc::UnboundedSender<DownAudio>,
    last_udp_ack: Mutex<Instant>,
    last_any_ack: Mutex<Instant>,
    on_dead: Mutex<Option<OnDead>>,
    video: Mutex<Option<VideoSink>>,
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
            keys: cfg.keys.clone(),
            unencrypted: cfg.unencrypted_for_tests,
            scope: Scope::Voice(cfg.channel),
            seq: AtomicU32::new(0),
            audio_seq: Default::default(),
            path: AtomicU8::new(0),
            udp: Mutex::new(None),
            ws,
            down,
            last_udp_ack: Mutex::new(Instant::now()),
            last_any_ack: Mutex::new(Instant::now()),
            on_dead: Mutex::new(None),
            video: Mutex::new(None),
            stop,
        });
        tokio::spawn(run(inner.clone(), cfg.host, cfg.udp_port, stop_rx));
        VoiceLink { inner }
    }

    pub fn path(&self) -> LinkPath {
        if self.inner.path.load(Ordering::Relaxed) == 0 { LinkPath::Udp } else { LinkPath::Ws }
    }

    /// Send one Opus frame. The payload starts with this speaker's own audio frame counter, which
    /// listeners use for jitter ordering (the packet header seq is per listener and only feeds the nonce).
    pub fn send_audio(&self, opus: Vec<u8>) {
        let n = self.inner.audio_seq.send();
        let mut p = Vec::with_capacity(4 + opus.len());
        p.extend_from_slice(&n.to_be_bytes());
        p.extend_from_slice(&opus);
        self.inner.send(UP_AUDIO, &p, self.path());
    }

    /// Encrypt one Opus frame for the call. None: no key yet (send nothing, `skip_audio`).
    /// Without e2ee keys nothing is sent (tests may opt into plaintext).
    pub fn seal_audio(&self, opus: &[u8]) -> Option<Vec<u8>> {
        match &self.inner.keys {
            Some(k) => crate::e2ee::seal_shared(k, &self.inner.scope, KIND_VOICE, opus, Instant::now()),
            None if self.inner.unencrypted => Some(opus.to_vec()),
            None => None,
        }
    }

    /// A 20 ms frame passed without audio (voice gate closed or muted): keeps the frame counter on time.
    pub fn skip_audio(&self) {
        self.inner.audio_seq.skip();
    }

    /// Go Live data for `to` through the relay (the server forwards it only within a watch pair).
    pub fn send_video(&self, to: Id, data: &[u8]) {
        let mut p = Vec::with_capacity(8 + data.len());
        p.extend_from_slice(&to.to_be_bytes());
        p.extend_from_slice(data);
        self.inner.send(UP_VIDEO, &p, self.path());
    }

    pub fn set_video_sink(&self, f: VideoSink) {
        *self.inner.video.lock().unwrap() = Some(f);
    }

    /// Called when neither path has answered for `LINK_DEAD`.
    pub fn set_on_dead(&self, f: OnDead) {
        *self.inner.on_dead.lock().unwrap() = Some(f);
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
            DOWN_KEEPALIVE_ACK => {
                *self.last_any_ack.lock().unwrap() = Instant::now();
                if via_udp {
                    *self.last_udp_ack.lock().unwrap() = Instant::now();
                    self.set_path(LinkPath::Udp);
                }
            }
            DOWN_AUDIO if d.payload.len() >= 4 => {
                let seq = u32::from_be_bytes(d.payload[..4].try_into().unwrap());
                let body = &d.payload[4..];
                let payload = match &self.keys {
                    // A frame that does not open is dropped: the jitter buffer conceals it as lost.
                    Some(k) => match crate::e2ee::open_shared(k, &self.scope, d.from_user, KIND_VOICE, body, Instant::now()) {
                        Some(p) => p,
                        None => return,
                    },
                    None if self.unencrypted => body.to_vec(),
                    // No keys: never play audio that wasn't end-to-end encrypted.
                    None => return,
                };
                let _ = self.down.send(DownAudio { from: d.from_user, seq, payload });
            }
            DOWN_VIDEO => {
                let sink = self.video.lock().unwrap().clone();
                if let Some(f) = sink {
                    f(d.from_user, &d.payload);
                }
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
            // `try_send` on a new socket fails with WouldBlock until writability was observed once.
            if s.connect(to).await.is_ok() && s.writable().await.is_ok() {
                return Some((Arc::new(s), *to));
            }
        }
    }
    None
}

async fn run(inner: Arc<Inner>, host: String, port: u16, mut stop: watch::Receiver<bool>) {
    let mut udp = bind_udp(&host, port).await;
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
                if inner.last_any_ack.lock().unwrap().elapsed() > LINK_DEAD {
                    tracing::info!("voice: no answer on any path for {LINK_DEAD:?}; rebinding UDP and reconnecting");
                    *inner.last_any_ack.lock().unwrap() = Instant::now();
                    udp = bind_udp(&host, port).await;
                    *inner.udp.lock().unwrap() = udp.clone();
                    last_probe = Instant::now();
                    inner.send(UP_KEEPALIVE, &[], LinkPath::Udp);
                    let cb = inner.on_dead.lock().unwrap().clone();
                    if let Some(cb) = cb {
                        cb();
                    }
                }
                let since_ack = inner.last_udp_ack.lock().unwrap().elapsed();
                let path = if udp.is_some() && since_ack <= UDP_SILENCE { LinkPath::Udp } else { LinkPath::Ws };
                inner.set_path(path);
                match path {
                    LinkPath::Udp => inner.send(UP_KEEPALIVE, &[], LinkPath::Udp),
                    LinkPath::Ws => {
                        inner.send(UP_KEEPALIVE, &[], LinkPath::Ws);
                        if last_probe.elapsed() >= UDP_REPROBE {
                            last_probe = Instant::now();
                            // A fresh socket: the old one may be bound to an address that no longer exists.
                            udp = bind_udp(&host, port).await;
                            *inner.udp.lock().unwrap() = udp.clone();
                            inner.send(UP_KEEPALIVE, &[], LinkPath::Udp);
                        }
                    }
                }
            }
        }
    }
}
