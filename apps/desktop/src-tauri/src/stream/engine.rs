//! Go Live data plane. The streamer side (`Broadcast`) captures, encodes once and fans the
//! fragments out to every viewer's `PeerLink`; the viewer side (`Watch`) reassembles access units
//! for the webview decoder and plays the stream's sound through the voice mixer.
//!
//! Pair messages (inside the encrypted peer/relay packet), first byte is the type:
//! `VIDEO` + fragment · `AUDIO` + seq u32 + Opus (stereo) · `REPORT` + loss f32 · `KEYREQ`.
use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU32, Ordering},
        mpsc as smpsc,
    },
    time::{Duration, Instant},
};

use rynuvo_proto::{Id, voice_wire::VoiceKey};
use serde::Serialize;
use tokio::sync::mpsc;

use super::{
    capture::{AudioSink, CaptureOpts, EndedSink, VideoSink},
    encode::{self, Codec, Frame, VideoEncoder},
    frag::{self, Reassembler},
    peer::{PeerLink, PeerPath, Role},
    rate::Controller,
};

pub const T_VIDEO: u8 = 1;
pub const T_AUDIO: u8 = 2;
pub const T_REPORT: u8 = 3;
pub const T_KEYREQ: u8 = 4;

/// Relay streams are capped (server bandwidth): 1080p30, ~4 Mbps.
pub const RELAY_KBPS: u32 = 4000;
pub const RELAY_FPS: u32 = 30;
pub const RELAY_MAX: (u32, u32) = (1920, 1080);
const REPORT_EVERY: Duration = Duration::from_millis(500);
/// A static screen produces no new frames: re-encode the last one this often so joiners and
/// keyframe requests are answered and viewers know the stream is alive.
const IDLE_REPEAT: Duration = Duration::from_millis(500);
/// Viewer: no video for this long means the streamer is gone.
pub const VIEWER_TIMEOUT: Duration = Duration::from_secs(8);
const KEYREQ_EVERY: Duration = Duration::from_millis(400);
const AUDIO_FRAME: usize = 960;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Preset {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub max_kbps: u32,
}

/// Quality presets the streamer picks; unknown names mean the default, 1080p60.
pub fn preset(name: &str) -> Preset {
    match name {
        "720p30" => Preset { width: 1280, height: 720, fps: 30, max_kbps: 4_000 },
        "1440p60" => Preset { width: 2560, height: 1440, fps: 60, max_kbps: 16_000 },
        "native" => Preset { width: 7680, height: 4320, fps: 60, max_kbps: 25_000 },
        _ => Preset { width: 1920, height: 1080, fps: 60, max_kbps: 10_000 },
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum StreamEvent {
    /// I'm live: who is watching and how the encoder is doing.
    Live { viewers: Vec<Id>, codec: String, kbps: u32, hardware: bool, relay: bool },
    /// A stream I watch.
    Stats { user_id: Id, fps: f32, codec: String, kbps: u32, path: String },
    Ended { user_id: Id, reason: String },
    Error { message: String },
}

pub type EventSink = Arc<dyn Fn(StreamEvent) + Send + Sync>;
/// Access units to the webview: [codec u8][key u8][ts u32 BE][Annex B AU].
pub type FrameOut = Arc<dyn Fn(Vec<u8>) + Send + Sync>;
/// Decoded stream sound (mono 48 kHz) from a streamer, into the voice mixer.
pub type AudioOut = Arc<dyn Fn(Id, Vec<f32>) + Send + Sync>;
pub type Stopper = Box<dyn FnOnce() + Send>;
pub type CaptureFn = Arc<dyn Fn(CaptureOpts, VideoSink, AudioSink, EndedSink) -> Result<Stopper, String> + Send + Sync>;

/// How pair data leaves this machine apart from the direct path.
#[derive(Clone)]
pub struct Net {
    pub signal: Arc<dyn Fn(Id, serde_json::Value) + Send + Sync>,
    pub relay: Arc<dyn Fn(Id, Vec<u8>) + Send + Sync>,
    pub stun: Vec<SocketAddr>,
    pub rt: tokio::runtime::Handle,
}

impl Net {
    fn link(&self, role: Role, other: Id, key: VoiceKey) -> (PeerLink, mpsc::UnboundedReceiver<Vec<u8>>) {
        let (tx, rx) = mpsc::unbounded_channel();
        let sig = self.signal.clone();
        let rel = self.relay.clone();
        let _g = self.rt.enter();
        let l = PeerLink::start(role, key, self.stun.clone(), Arc::new(move |v| sig(other, v)), Arc::new(move |b| rel(other, b)), tx);
        (l, rx)
    }
}

fn path_name(p: PeerPath) -> &'static str {
    match p {
        PeerPath::Direct(_) => "direct",
        PeerPath::Relay => "relay",
        PeerPath::Connecting => "connecting",
    }
}

/// Any viewer actually on the relay (a path still hole punching does not count: it would toggle the
/// cap on every join).
pub fn any_relay(paths: &[PeerPath]) -> bool {
    paths.iter().any(|p| *p == PeerPath::Relay)
}

/// The relay cap needs a capture restart only when it lowers the resolution; fps is throttled in
/// the encoder.
pub fn needs_restart_for_cap(p: Preset) -> bool {
    p.width > RELAY_MAX.0 || p.height > RELAY_MAX.1
}

/// A new frame came sooner than `max_fps` allows, counting from when the last sent frame was taken
/// (not from when it finished sending: that would drop every other frame of a 60 fps source).
pub fn too_soon(since_last_taken: Option<Duration>, max_fps: u32) -> bool {
    since_last_taken.is_some_and(|d| d < Duration::from_micros(900_000 / max_fps.max(1) as u64))
}

/// The relay state must hold this long before the cap switches.
const CAP_SETTLE: Duration = Duration::from_secs(2);

/// Token bucket: spreads a key frame's fragments instead of one burst that overflows socket
/// buffers and home routers.
pub struct Pacer {
    rate: f64,
    burst: f64,
    tokens: f64,
    at: Option<Instant>,
}

impl Pacer {
    /// `rate` bytes per second, `burst` bytes sent without waiting.
    pub fn new(rate: u64, burst: u64) -> Pacer {
        Pacer { rate: rate.max(1) as f64, burst: burst as f64, tokens: burst as f64, at: None }
    }

    pub fn set_rate(&mut self, rate: u64) {
        self.rate = rate.max(1) as f64;
    }

    /// How long to wait before sending `bytes` at `now` (the bytes are counted as sent).
    pub fn wait(&mut self, bytes: u64, now: Instant) -> Duration {
        if let Some(t) = self.at {
            self.tokens = (self.tokens + now.saturating_duration_since(t).as_secs_f64() * self.rate).min(self.burst);
        }
        self.at = Some(now);
        self.tokens -= bytes as f64;
        if self.tokens >= 0.0 { Duration::ZERO } else { Duration::from_secs_f64(-self.tokens / self.rate) }
    }
}

fn with_type(t: u8, body: &[u8]) -> Vec<u8> {
    let mut v = Vec::with_capacity(1 + body.len());
    v.push(t);
    v.extend_from_slice(body);
    v
}

// ---------------------------------------------------------------------------------------------
// Streamer

struct ViewerConn {
    link: PeerLink,
    codecs: Vec<Codec>,
    loss: f32,
    last_keyreq: Option<Instant>,
}

struct BInner {
    net: Net,
    preset: Preset,
    viewers: Mutex<HashMap<Id, ViewerConn>>,
    force_key: AtomicBool,
    stop: AtomicBool,
    audio_seq: AtomicU32,
}

impl BInner {
    fn fan_out(&self, msgs: &[Vec<u8>]) {
        let links: Vec<PeerLink> = self.viewers.lock().unwrap().values().map(|v| v.link.clone()).collect();
        for l in links {
            for m in msgs {
                l.send(m);
            }
        }
    }

    fn on_viewer_msg(&self, from: Id, m: &[u8]) {
        let mut vs = self.viewers.lock().unwrap();
        let Some(v) = vs.get_mut(&from) else { return };
        match m.first() {
            Some(&T_REPORT) if m.len() >= 5 => {
                let loss = f32::from_be_bytes(m[1..5].try_into().unwrap());
                if loss.is_finite() {
                    v.loss = v.loss.max(loss.clamp(0.0, 1.0));
                }
            }
            Some(&T_KEYREQ) => {
                if v.last_keyreq.is_none_or(|t| t.elapsed() >= KEYREQ_EVERY) {
                    v.last_keyreq = Some(Instant::now());
                    self.force_key.store(true, Ordering::Relaxed);
                }
            }
            _ => {}
        }
    }
}

pub struct Broadcast {
    inner: Arc<BInner>,
    capture: Arc<Mutex<Option<Stopper>>>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Broadcast {
    pub fn start(start_capture: CaptureFn, preset: Preset, net: Net, events: EventSink) -> Result<Broadcast, String> {
        let inner = Arc::new(BInner {
            net,
            preset,
            viewers: Mutex::new(HashMap::new()),
            force_key: AtomicBool::new(true),
            stop: AtomicBool::new(false),
            audio_seq: AtomicU32::new(0),
        });
        let (ftx, frx) = smpsc::sync_channel::<Frame>(1);
        let video: VideoSink = Arc::new(move |f| {
            let _ = ftx.try_send(f); // encoder busy: drop this frame, keep latency low
        });
        let audio = audio_sink(inner.clone());
        let (i3, ev3) = (Arc::downgrade(&inner), events.clone());
        let ended: EndedSink = Arc::new(move |why: String| {
            // Reported once; the controller then ends the stream.
            if let Some(i) = i3.upgrade() {
                if !i.stop.swap(true, Ordering::Relaxed) {
                    (ev3)(StreamEvent::Error { message: why });
                }
            }
        });
        let opts = move |capped: bool| {
            let (w, h) = if capped { (preset.width.min(RELAY_MAX.0), preset.height.min(RELAY_MAX.1)) } else { (preset.width, preset.height) };
            CaptureOpts { max_width: w, max_height: h, fps: if capped { preset.fps.min(RELAY_FPS) } else { preset.fps } }
        };
        let stopper = start_capture(opts(false), video.clone(), audio.clone(), ended.clone())?;
        let capture = Arc::new(Mutex::new(Some(stopper)));
        let (i2, c2, i4) = (inner.clone(), capture.clone(), inner.clone());
        let thread = std::thread::Builder::new()
            .name("rynuvo-encode".into())
            .spawn(move || {
                let restart = move |capped: bool| {
                    if let Some(s) = c2.lock().unwrap().take() {
                        s();
                    }
                    match start_capture(opts(capped), video.clone(), audio.clone(), ended.clone()) {
                        // Stopped meanwhile: don't leave the new capture running.
                        Ok(s) if i4.stop.load(Ordering::Relaxed) => s(),
                        Ok(s) => *c2.lock().unwrap() = Some(s),
                        Err(e) => tracing::warn!("stream: capture restart failed: {e}"),
                    }
                };
                encode_loop(i2, frx, events, restart)
            })
            .map_err(|e| e.to_string())?;
        Ok(Broadcast { inner, capture, thread: Some(thread) })
    }

    /// The server paired a new viewer with me.
    pub fn add_viewer(&self, viewer: Id, codecs: &[String], key: VoiceKey) {
        let (link, mut rx) = self.inner.net.link(Role::Streamer, viewer, key);
        let codecs: Vec<Codec> = codecs.iter().filter_map(|c| Codec::from_name(c)).collect();
        let old = self.inner.viewers.lock().unwrap().insert(viewer, ViewerConn { link, codecs, loss: 0.0, last_keyreq: None });
        if let Some(o) = old {
            o.link.stop();
        }
        self.inner.force_key.store(true, Ordering::Relaxed);
        let inner = Arc::downgrade(&self.inner);
        self.inner.net.rt.spawn(async move {
            while let Some(m) = rx.recv().await {
                match inner.upgrade() {
                    Some(i) => i.on_viewer_msg(viewer, &m),
                    None => return,
                }
            }
        });
    }

    pub fn remove_viewer(&self, viewer: Id) {
        if let Some(v) = self.inner.viewers.lock().unwrap().remove(&viewer) {
            v.link.stop();
        }
    }

    pub fn has_viewer(&self, viewer: Id) -> bool {
        self.inner.viewers.lock().unwrap().contains_key(&viewer)
    }

    pub fn on_signal(&self, from: Id, v: &serde_json::Value) {
        if let Some(c) = self.inner.viewers.lock().unwrap().get(&from) {
            c.link.on_signal(v);
        }
    }

    pub fn on_relay(&self, from: Id, b: &[u8]) {
        if let Some(c) = self.inner.viewers.lock().unwrap().get(&from) {
            c.link.on_relay(b);
        }
    }

    pub fn stop(mut self) {
        self.shutdown();
    }

    fn shutdown(&mut self) {
        self.inner.stop.store(true, Ordering::Relaxed);
        if let Some(s) = self.capture.lock().unwrap().take() {
            s();
        }
        for (_, v) in self.inner.viewers.lock().unwrap().drain() {
            v.link.stop();
        }
        if let Some(t) = self.thread.take() {
            // The encoder notices `stop` within 100 ms; don't hold the caller (often async) on it.
            std::thread::spawn(move || {
                let _ = t.join();
            });
        }
    }
}

impl Drop for Broadcast {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// Stream sound: 20 ms stereo Opus frames at ~128 kbps to every viewer.
fn audio_sink(inner: Arc<BInner>) -> AudioSink {
    let enc = opus::Encoder::new(48_000, opus::Channels::Stereo, opus::Application::Audio).ok().map(|mut e| {
        let _ = e.set_bitrate(opus::Bitrate::Bits(128_000));
        e
    });
    let state = Mutex::new((enc, Vec::<f32>::with_capacity(AUDIO_FRAME * 4)));
    let weak = Arc::downgrade(&inner);
    drop(inner);
    Arc::new(move |pcm: &[f32]| {
        let Some(inner) = weak.upgrade() else { return };
        if inner.viewers.lock().unwrap().is_empty() {
            return;
        }
        let mut st = state.lock().unwrap();
        let (enc, buf) = &mut *st;
        let Some(enc) = enc.as_mut() else { return };
        buf.extend_from_slice(pcm);
        let mut out = [0u8; 1500];
        let mut msgs = Vec::new();
        while buf.len() >= AUDIO_FRAME * 2 {
            if let Ok(n) = enc.encode_float(&buf[..AUDIO_FRAME * 2], &mut out) {
                let seq = inner.audio_seq.fetch_add(1, Ordering::Relaxed).wrapping_add(1);
                let mut m = vec![T_AUDIO];
                m.extend_from_slice(&seq.to_be_bytes());
                m.extend_from_slice(&out[..n]);
                msgs.push(m);
            }
            buf.drain(..AUDIO_FRAME * 2);
        }
        if buf.len() > 48_000 {
            buf.clear();
        }
        drop(st);
        inner.fan_out(&msgs);
    })
}

fn encode_loop(inner: Arc<BInner>, frames: smpsc::Receiver<Frame>, events: EventSink, restart: impl Fn(bool)) {
    let mut enc: Option<Box<dyn VideoEncoder>> = None;
    let mut enc_size = (0, 0);
    let mut rate = Controller::new(inner.preset.max_kbps);
    let mut kbps = 0;
    let mut frame_no: u32 = 0;
    let started = Instant::now();
    let mut last: Option<Frame> = None;
    let mut last_new = Instant::now();
    let mut last_sent: Option<Instant> = None;
    let mut last_taken: Option<Instant> = None;
    let mut last_report = Instant::now();
    let mut capped = false;
    let mut last_live: Option<StreamEvent> = None;
    let mut relay_state = false;
    let mut relay_change: Option<Instant> = None;
    let mut pacer = Pacer::new(1, 64_000);
    while !inner.stop.load(Ordering::Relaxed) {
        let fresh = match frames.recv_timeout(Duration::from_millis(50)) {
            Ok(f) => {
                last = Some(f);
                last_new = Instant::now();
                true
            }
            Err(smpsc::RecvTimeoutError::Timeout) => false,
            Err(smpsc::RecvTimeoutError::Disconnected) => break,
        };
        let (ids, codecs, relay, worst) = {
            let mut vs = inner.viewers.lock().unwrap();
            let ids: Vec<Id> = vs.keys().copied().collect();
            let codecs: Vec<Vec<Codec>> = vs.values().map(|v| v.codecs.clone()).collect();
            let paths: Vec<PeerPath> = vs.values().map(|v| v.link.path()).collect();
            let relay = any_relay(&paths);
            let due = last_report.elapsed() >= REPORT_EVERY;
            let worst = if due { vs.values().map(|v| v.loss).fold(0.0f32, f32::max) } else { -1.0 };
            if due {
                vs.values_mut().for_each(|v| v.loss = 0.0);
            }
            (ids, codecs, relay, worst)
        };
        if worst >= 0.0 {
            last_report = Instant::now();
            rate.on_report(worst);
        }
        // Debounce: a viewer flapping between paths must not restart capture over and over.
        if relay != relay_state {
            let since = *relay_change.get_or_insert_with(Instant::now);
            if since.elapsed() >= CAP_SETTLE {
                relay_state = relay;
                relay_change = None;
            }
        } else {
            relay_change = None;
        }
        let relay = relay_state;
        let want_cap = relay && needs_restart_for_cap(inner.preset);
        if want_cap != capped {
            capped = want_cap;
            tracing::info!("stream: relay resolution cap {}", if capped { "on" } else { "off" });
            restart(capped);
        }
        rate.set_max(if relay { inner.preset.max_kbps.min(RELAY_KBPS) } else { inner.preset.max_kbps });
        if ids.is_empty() {
            let idle = StreamEvent::Live { viewers: Vec::new(), codec: e_name(&enc), kbps, hardware: enc.as_ref().is_some_and(|e| e.hardware()), relay: false };
            if last_live.as_ref() != Some(&idle) {
                (events)(idle.clone());
                last_live = Some(idle);
            }
            continue; // nobody watching: capture keeps running, nothing is encoded or sent
        }
        let force = inner.force_key.load(Ordering::Relaxed);
        let idle = !fresh && last_new.elapsed() >= IDLE_REPEAT && last_sent.is_none_or(|t| t.elapsed() >= IDLE_REPEAT);
        if !fresh && !idle && !(force && last.is_some()) {
            continue;
        }
        let max_fps = if relay { inner.preset.fps.min(RELAY_FPS) } else { inner.preset.fps };
        if fresh && !force && too_soon(last_taken.map(|t| t.elapsed()), max_fps) {
            continue;
        }
        let taken = Instant::now();
        let Some(frame) = last.as_ref() else { continue };
        let refs: Vec<&[Codec]> = codecs.iter().map(|c| c.as_slice()).collect();
        let codec = encode::negotiate(&encode::available(), &refs);
        let size = frame.size();
        if enc.as_ref().is_none_or(|e| e.codec() != codec) || enc_size != size {
            kbps = rate.target_kbps();
            enc = match encode::open(codec, size.0, size.1, inner.preset.fps, kbps).or_else(|e| {
                tracing::warn!("stream: {e}; using H.264");
                encode::open(Codec::H264, size.0, size.1, inner.preset.fps, kbps)
            }) {
                Ok(e) => Some(e),
                Err(e) => {
                    if !inner.stop.swap(true, Ordering::Relaxed) {
                        (events)(StreamEvent::Error { message: format!("Could not start the video encoder: {e}") });
                    }
                    break;
                }
            };
            enc_size = size;
            inner.force_key.store(true, Ordering::Relaxed);
        }
        let e = enc.as_mut().unwrap();
        if rate.target_kbps() != kbps {
            kbps = rate.target_kbps();
            e.set_bitrate(kbps);
        }
        let force = inner.force_key.swap(false, Ordering::Relaxed);
        match e.encode(frame, force) {
            Ok(Some(out)) => {
                frame_no = frame_no.wrapping_add(1);
                let ts = started.elapsed().as_millis() as u16;
                let flags = if out.key { frag::KEY } else { 0 };
                let msgs: Vec<Vec<u8>> = frag::split(frame_no, &out.data, flags, e.codec() as u8, ts).iter().map(|f| with_type(T_VIDEO, f)).collect();
                // 1.5× the target bitrate per viewer, so frames still leave well within their slot.
                let n = ids.len() as u64;
                pacer.set_rate(kbps as u64 * 1000 / 8 * 3 / 2 * n);
                for chunk in msgs.chunks(8) {
                    let bytes: u64 = chunk.iter().map(|m| m.len() as u64).sum::<u64>() * n;
                    let w = pacer.wait(bytes, Instant::now());
                    if !w.is_zero() {
                        std::thread::sleep(w.min(Duration::from_millis(200)));
                    }
                    inner.fan_out(chunk);
                }
                last_sent = Some(Instant::now());
                last_taken = Some(taken);
            }
            Ok(None) => {}
            Err(err) => {
                tracing::warn!("stream: encode failed: {err}");
                enc = None;
            }
        }
        let live = StreamEvent::Live { viewers: ids, codec: e_name(&enc), kbps, hardware: enc.as_ref().is_some_and(|e| e.hardware()), relay };
        if last_live.as_ref() != Some(&live) {
            (events)(live.clone());
            last_live = Some(live);
        }
    }
}

fn e_name(e: &Option<Box<dyn VideoEncoder>>) -> String {
    e.as_ref().map_or("", |e| e.codec().name()).to_string()
}

// ---------------------------------------------------------------------------------------------
// Viewer

pub struct Watch {
    streamer: Id,
    link: Arc<Mutex<Option<PeerLink>>>,
    stop: Arc<AtomicBool>,
}

impl Watch {
    pub fn new(streamer: Id) -> Watch {
        Watch { streamer, link: Arc::default(), stop: Arc::default() }
    }

    /// The server paired me with the streamer: open the path and start receiving.
    pub fn connect(&self, key: VoiceKey, net: &Net, out: FrameOut, audio: AudioOut, events: EventSink) {
        let (link, rx) = net.link(Role::Viewer, self.streamer, key);
        if let Some(old) = self.link.lock().unwrap().replace(link.clone()) {
            old.stop();
        }
        net.rt.spawn(receive(self.streamer, link, rx, out, audio, events, self.stop.clone()));
    }

    pub fn on_signal(&self, v: &serde_json::Value) {
        if let Some(l) = self.link.lock().unwrap().as_ref() {
            l.on_signal(v);
        }
    }

    pub fn request_key(&self) {
        if let Some(l) = self.link.lock().unwrap().as_ref() {
            l.send(&[T_KEYREQ]);
        }
    }

    pub fn on_relay(&self, b: &[u8]) {
        if let Some(l) = self.link.lock().unwrap().as_ref() {
            l.on_relay(b);
        }
    }

    pub fn stop(&self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(l) = self.link.lock().unwrap().take() {
            l.stop();
        }
    }
}

impl Drop for Watch {
    fn drop(&mut self) {
        self.stop();
    }
}

async fn receive(
    streamer: Id,
    link: PeerLink,
    mut rx: mpsc::UnboundedReceiver<Vec<u8>>,
    out: FrameOut,
    audio: AudioOut,
    events: EventSink,
    stop: Arc<AtomicBool>,
) {
    let mut reasm = Reassembler::new();
    let mut dec = opus::Decoder::new(48_000, opus::Channels::Stereo).ok();
    let mut pcm = vec![0f32; AUDIO_FRAME * 2 * 6];
    let mut tick = tokio::time::interval(REPORT_EVERY);
    let mut last_video = Instant::now();
    let (mut frames, mut bytes, mut codec) = (0u32, 0usize, 0u8);
    let mut window = Instant::now();
    loop {
        tokio::select! {
            m = rx.recv() => {
                let Some(m) = m else { break };
                match m.first() {
                    Some(&T_VIDEO) => {
                        bytes += m.len();
                        for a in reasm.push(&m[1..]) {
                            last_video = Instant::now();
                            frames += 1;
                            codec = a.codec;
                            let mut b = Vec::with_capacity(6 + a.data.len());
                            b.push(a.codec);
                            b.push(a.key as u8);
                            b.extend_from_slice(&(a.ts_ms as u32).to_be_bytes());
                            b.extend_from_slice(&a.data);
                            out(b);
                        }
                    }
                    Some(&T_AUDIO) if m.len() > 5 => {
                        if let Some(d) = dec.as_mut() {
                            if let Ok(n) = d.decode_float(&m[5..], &mut pcm, false) {
                                let mono: Vec<f32> = pcm[..n * 2].chunks_exact(2).map(|s| (s[0] + s[1]) * 0.5).collect();
                                audio(streamer, mono);
                            }
                        }
                    }
                    _ => {}
                }
            }
            _ = tick.tick() => {
                if stop.load(Ordering::Relaxed) {
                    break;
                }
                let mut r = vec![T_REPORT];
                r.extend_from_slice(&reasm.take_loss().to_be_bytes());
                link.send(&r);
                if reasm.needs_keyframe() {
                    link.send(&[T_KEYREQ]);
                }
                let secs = window.elapsed().as_secs_f32().max(0.001);
                if secs >= 1.0 {
                    let name = Codec::from_u8(codec).map_or("", |c| c.name()).to_string();
                    (events)(StreamEvent::Stats {
                        user_id: streamer,
                        fps: frames as f32 / secs,
                        codec: name,
                        kbps: (bytes as f32 * 8.0 / 1000.0 / secs) as u32,
                        path: path_name(link.path()).into(),
                    });
                    frames = 0;
                    bytes = 0;
                    window = Instant::now();
                }
                if last_video.elapsed() > VIEWER_TIMEOUT {
                    (events)(StreamEvent::Ended { user_id: streamer, reason: "The stream stopped responding".into() });
                    break;
                }
            }
        }
    }
    link.stop();
}

#[cfg(test)]
mod tests {

    #[test]
    fn a_60fps_source_is_not_halved_by_the_frame_gate() {
        let at = |ms: f64| Duration::from_micros((ms * 1000.0) as u64);
        // Frames every 16.7 ms; the previous one took 4 ms to encode and send.
        assert!(!too_soon(Some(at(16.7)), 60), "measured from when the last frame was taken");
        assert!(!too_soon(None, 60));
        // A 120 fps source is thinned to about 60.
        assert!(too_soon(Some(at(8.3)), 60));
        assert!(!too_soon(Some(at(16.6)), 60));
        // Relay: 30 fps.
        assert!(too_soon(Some(at(16.7)), 30));
        assert!(!too_soon(Some(at(33.3)), 30));
    }

    use super::*;

    #[test]
    fn pacer_spreads_a_burst() {
        // 1 MB/s with a 64 KB burst: the first 64 KB go at once, the next 100 KB need ~100 ms.
        let mut p = Pacer::new(1_000_000, 64_000);
        let t0 = Instant::now();
        assert_eq!(p.wait(64_000, t0), Duration::ZERO);
        let w = p.wait(100_000, t0);
        assert!(w >= Duration::from_millis(95) && w <= Duration::from_millis(105), "{w:?}");
        // After a quiet second the bucket is full again.
        assert_eq!(p.wait(64_000, t0 + Duration::from_secs(2)), Duration::ZERO);
    }

    #[test]
    fn only_a_real_relay_path_caps_the_stream() {
        use std::net::SocketAddr;
        let d = PeerPath::Direct("127.0.0.1:1".parse::<SocketAddr>().unwrap());
        assert!(!any_relay(&[PeerPath::Connecting, d]), "hole punching in progress is not relay");
        assert!(any_relay(&[d, PeerPath::Relay]));
        // fps alone is throttled in the encoder; only a resolution change needs a capture restart.
        assert!(!needs_restart_for_cap(preset("1080p60")));
        assert!(needs_restart_for_cap(preset("1440p60")));
        assert!(!needs_restart_for_cap(preset("720p30")));
    }

    #[test]
    fn presets() {
        assert_eq!(preset("720p30"), Preset { width: 1280, height: 720, fps: 30, max_kbps: 4_000 });
        assert_eq!(preset("1080p60").fps, 60);
        assert_eq!(preset("whatever"), preset("1080p60"));
        assert!(preset("native").width >= 3840);
    }
}
