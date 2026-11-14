//! The voice engine: mic → DSP → gate → Opus → link, and link → jitter → mixer → speakers.
//! Runs on its own thread with a 10 ms tick; every control call is a message to that thread.
use std::{
    collections::HashMap,
    sync::{Arc, Mutex, mpsc},
    thread::JoinHandle,
    time::{Duration, Instant},
};

use ringbuf::{
    HeapCons, HeapProd, HeapRb,
    traits::{Consumer, Observer, Producer, Split},
};
use rynuvo_proto::Id;
use serde::Serialize;
use tokio::sync::mpsc as tmpsc;

use super::{
    FRAME,
    backend::{AudioBackend, AudioIo},
    codec::Encoder,
    dsp::{DSP_FRAME, Processor},
    gate::VadGate,
    link::{DownAudio, LinkCfg, LinkPath, VoiceLink, WsSender},
    mixer::Mixer,
    tones,
};
pub use super::tones::Tone;

const TICK: Duration = Duration::from_millis(10);
const LEVEL_EVERY: Duration = Duration::from_millis(100);
const FIRST_RETRY: Duration = Duration::from_secs(1);
const RETRY_EVERY: Duration = Duration::from_secs(3);
/// Output queued ahead of the sound card (40 ms).
const PLAYBACK_AHEAD: usize = 2 * FRAME;
/// Mic backlog beyond which old audio is dropped to keep latency low (200 ms).
const CAPTURE_MAX_BACKLOG: usize = 10 * FRAME;
/// DSP frames processed per tick at most (60 ms of audio per 10 ms tick), so commands are
/// always handled even when the machine can't keep up with the mic.
const CAPTURE_PER_TICK: usize = 6;

#[derive(Debug, Clone)]
pub struct EngineOpts {
    pub me: Id,
    pub input: Option<String>,
    pub output: Option<String>,
    /// Voice-activity threshold, 0–1.
    pub threshold: f32,
    pub volumes: HashMap<Id, f32>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum VoiceEvent {
    Speaking { user_ids: Vec<Id> },
    Level { db: f32, open: bool },
    Path { via: LinkPath },
    DeviceError { message: String },
}

pub type EventSink = Arc<dyn Fn(VoiceEvent) + Send + Sync>;

enum Cmd {
    Muted(bool),
    Deafened(bool),
    Threshold(f32),
    Volume(Id, f32),
    Devices(Option<String>, Option<String>),
    MicTest(bool),
    RemovePeer(Id),
    Tone(Tone),
    Stop,
}

pub struct VoiceEngine {
    tx: mpsc::Sender<Cmd>,
    link: Option<VoiceLink>,
    thread: Mutex<Option<JoinHandle<()>>>,
}

impl VoiceEngine {
    /// Start the engine. `link = None` runs audio only (the mic test in Settings).
    /// Must be called inside a Tokio runtime when `link` is set.
    pub fn start(backend: Box<dyn AudioBackend>, opts: EngineOpts, link: Option<LinkCfg>, ws: WsSender, events: EventSink) -> VoiceEngine {
        let (down_tx, down_rx) = tmpsc::unbounded_channel();
        let link = link.map(|cfg| VoiceLink::start(cfg, ws, down_tx));
        let (tx, rx) = mpsc::channel();
        let link2 = link.clone();
        let h = std::thread::Builder::new()
            .name("rynuvo-voice".into())
            .spawn(move || Worker::new(backend, opts, link2, down_rx, events).run(rx))
            .expect("spawn voice thread");
        VoiceEngine { tx, link, thread: Mutex::new(Some(h)) }
    }

    fn send(&self, c: Cmd) {
        let _ = self.tx.send(c);
    }
    pub fn set_muted(&self, v: bool) {
        self.send(Cmd::Muted(v));
    }
    pub fn set_deafened(&self, v: bool) {
        self.send(Cmd::Deafened(v));
    }
    pub fn set_threshold(&self, v: f32) {
        self.send(Cmd::Threshold(v));
    }
    pub fn set_volume(&self, user: Id, v: f32) {
        self.send(Cmd::Volume(user, v));
    }
    pub fn set_devices(&self, input: Option<String>, output: Option<String>) {
        self.send(Cmd::Devices(input, output));
    }
    /// Play the processed mic back to yourself (nothing is sent).
    pub fn mic_test(&self, on: bool) {
        self.send(Cmd::MicTest(on));
    }
    pub fn remove_peer(&self, user: Id) {
        self.send(Cmd::RemovePeer(user));
    }
    pub fn play_tone(&self, t: Tone) {
        self.send(Cmd::Tone(t));
    }
    /// A binary WebSocket frame from the server.
    pub fn feed_ws(&self, pkt: &[u8]) {
        if let Some(l) = &self.link {
            l.feed_ws(pkt);
        }
    }

    pub fn stop(&self) {
        self.send(Cmd::Stop);
        if let Some(l) = &self.link {
            l.stop();
        }
        if let Some(h) = self.thread.lock().unwrap().take() {
            let _ = h.join();
        }
    }
}

impl Drop for VoiceEngine {
    fn drop(&mut self) {
        self.stop();
    }
}

struct Worker {
    me: Id,
    backend: Box<dyn AudioBackend>,
    devices: (Option<String>, Option<String>),
    capture: Option<HeapCons<f32>>,
    playback: Option<HeapProd<f32>>,
    errors: Option<mpsc::Receiver<String>>,
    retry_at: Option<Instant>,
    link: Option<VoiceLink>,
    down: tmpsc::UnboundedReceiver<DownAudio>,
    events: EventSink,
    dsp: Processor,
    gate: VadGate,
    enc: Option<Encoder>,
    mixer: Mixer,
    muted: bool,
    deafened: bool,
    mic_test: bool,
    frame: [f32; FRAME],
    filled: usize,
    vad: f32,
    open: bool,
    level_sum: f64,
    level_n: usize,
    last_level: Instant,
    last_speaking: Vec<Id>,
    last_path: Option<LinkPath>,
}

impl Worker {
    fn new(backend: Box<dyn AudioBackend>, o: EngineOpts, link: Option<VoiceLink>, down: tmpsc::UnboundedReceiver<DownAudio>, events: EventSink) -> Self {
        let mut mixer = Mixer::new();
        for (u, v) in &o.volumes {
            mixer.set_volume(*u, *v);
        }
        let enc = match Encoder::new() {
            Ok(e) => Some(e),
            Err(e) => {
                tracing::error!("opus encoder: {e}");
                None
            }
        };
        Worker {
            me: o.me,
            backend,
            devices: (o.input, o.output),
            capture: None,
            playback: None,
            errors: None,
            retry_at: None,
            link,
            down,
            events,
            dsp: Processor::new(),
            gate: VadGate::new(o.threshold),
            enc,
            mixer,
            muted: false,
            deafened: false,
            mic_test: false,
            frame: [0.0; FRAME],
            filled: 0,
            vad: 0.0,
            open: false,
            level_sum: 0.0,
            level_n: 0,
            last_level: Instant::now(),
            last_speaking: Vec::new(),
            last_path: None,
        }
    }

    fn emit(&self, e: VoiceEvent) {
        (self.events)(e);
    }

    fn close_audio(&mut self) {
        self.backend.stop();
        self.capture = None;
        self.playback = None;
        self.errors = None;
    }

    fn open_audio(&mut self, input: Option<String>, output: Option<String>) -> Result<(), String> {
        self.close_audio();
        let (cap_p, cap_c) = HeapRb::<f32>::new(48_000).split();
        let (play_p, play_c) = HeapRb::<f32>::new(10 * FRAME).split();
        let (etx, erx) = mpsc::channel();
        self.backend.start(input.as_deref(), output.as_deref(), AudioIo { capture: cap_p, playback: play_c, errors: etx })?;
        self.capture = Some(cap_c);
        self.playback = Some(play_p);
        self.errors = Some(erx);
        Ok(())
    }

    /// Open the chosen devices; on failure report it and fall back to the defaults shortly.
    fn open_or_schedule(&mut self, input: Option<String>, output: Option<String>) {
        self.devices = (input.clone(), output.clone());
        if let Err(e) = self.open_audio(input, output) {
            tracing::warn!("audio start: {e}");
            self.emit(VoiceEvent::DeviceError { message: e });
            self.retry_at = Some(Instant::now() + FIRST_RETRY);
        } else {
            self.retry_at = None;
        }
    }

    fn check_devices(&mut self, now: Instant) {
        let failed = self.errors.as_ref().and_then(|r| r.try_recv().ok());
        if let Some(msg) = failed {
            tracing::warn!("audio device error: {msg}");
            self.close_audio();
            self.emit(VoiceEvent::DeviceError { message: format!("Audio device problem ({msg}). Switching to the default device.") });
            self.retry_at = Some(now + FIRST_RETRY);
        }
        if self.retry_at.is_some_and(|t| now >= t) {
            self.devices = (None, None);
            match self.open_audio(None, None) {
                Ok(()) => self.retry_at = None,
                Err(e) => {
                    self.emit(VoiceEvent::DeviceError { message: e });
                    self.retry_at = Some(now + RETRY_EVERY);
                }
            }
        }
    }

    fn handle(&mut self, c: Cmd) -> bool {
        match c {
            Cmd::Muted(v) => self.muted = v,
            Cmd::Deafened(v) => self.deafened = v,
            Cmd::Threshold(t) => self.gate.set_threshold(t),
            Cmd::Volume(u, v) => self.mixer.set_volume(u, v),
            Cmd::Devices(i, o) => self.open_or_schedule(i, o),
            Cmd::MicTest(v) => self.mic_test = v,
            Cmd::RemovePeer(u) => self.mixer.remove(u),
            Cmd::Tone(t) => self.mixer.play_local(tones::samples(t)),
            Cmd::Stop => return false,
        }
        true
    }

    fn run(mut self, rx: mpsc::Receiver<Cmd>) {
        let (i, o) = self.devices.clone();
        self.open_or_schedule(i, o);
        let mut next = Instant::now();
        'outer: loop {
            while let Ok(c) = rx.try_recv() {
                if !self.handle(c) {
                    break 'outer;
                }
            }
            let now = Instant::now();
            self.check_devices(now);
            while let Ok(d) = self.down.try_recv() {
                self.mixer.push(d.from, d.seq, d.payload);
            }
            self.pump_capture();
            self.pump_playback();
            self.report(now);
            next += TICK;
            let now = Instant::now();
            if next > now {
                std::thread::sleep(next - now);
            } else {
                next = now;
            }
        }
        self.close_audio();
        tracing::info!("voice engine stopped");
    }

    fn pump_capture(&mut self) {
        let Some(mut cap) = self.capture.take() else { return };
        let backlog = cap.occupied_len();
        if backlog > CAPTURE_MAX_BACKLOG {
            cap.skip(backlog - CAPTURE_MAX_BACKLOG / 2);
        }
        let mut chunk = [0f32; DSP_FRAME];
        let mut done = 0;
        while done < CAPTURE_PER_TICK && cap.occupied_len() >= DSP_FRAME {
            done += 1;
            cap.pop_slice(&mut chunk);
            let vad = self.dsp.capture(&mut chunk);
            self.level_sum += chunk.iter().map(|x| (*x as f64) * (*x as f64)).sum::<f64>();
            self.level_n += DSP_FRAME;
            self.frame[self.filled..self.filled + DSP_FRAME].copy_from_slice(&chunk);
            self.filled += DSP_FRAME;
            self.vad = self.vad.max(vad);
            if self.filled == FRAME {
                self.finish_frame();
            }
        }
        self.capture = Some(cap);
    }

    fn finish_frame(&mut self) {
        self.open = self.gate.update(self.vad);
        if self.mic_test {
            self.mixer.play_local(self.frame.to_vec());
        }
        if self.open && !self.muted {
            if let (Some(link), Some(enc)) = (&self.link, self.enc.as_mut()) {
                link.send_audio(enc.encode(&self.frame));
            }
        }
        self.filled = 0;
        self.vad = 0.0;
    }

    fn pump_playback(&mut self) {
        let Some(play) = self.playback.as_mut() else { return };
        let mut out = [0f32; FRAME];
        while play.occupied_len() < PLAYBACK_AHEAD && play.vacant_len() >= FRAME {
            self.mixer.mix(&mut out);
            if self.deafened {
                out.fill(0.0);
            }
            for half in out.chunks_exact(DSP_FRAME) {
                self.dsp.render(half.try_into().unwrap());
            }
            play.push_slice(&out);
        }
    }

    fn report(&mut self, now: Instant) {
        let mut speaking = self.mixer.speaking();
        if self.open && !self.muted {
            speaking.push(self.me);
            speaking.sort_unstable();
            speaking.dedup();
        }
        if speaking != self.last_speaking {
            self.last_speaking = speaking.clone();
            self.emit(VoiceEvent::Speaking { user_ids: speaking });
        }
        if now.duration_since(self.last_level) >= LEVEL_EVERY {
            self.last_level = now;
            let ms = if self.level_n > 0 { self.level_sum / self.level_n as f64 } else { 0.0 };
            let db = if ms > 1e-10 { (10.0 * ms.log10()) as f32 } else { -100.0 };
            self.level_sum = 0.0;
            self.level_n = 0;
            self.emit(VoiceEvent::Level { db: db.max(-100.0), open: self.open && !self.muted });
        }
        if let Some(l) = &self.link {
            let p = l.path();
            if self.last_path != Some(p) {
                self.last_path = Some(p);
                self.emit(VoiceEvent::Path { via: p });
            }
        }
    }
}
