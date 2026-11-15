//! Voice engine with a fake audio backend: gate, mute, deafen, device-error recovery.
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use ringbuf::traits::{Consumer, Producer};
use rynuvo_desktop_lib::voice::{
    backend::{AudioBackend, AudioIo},
    engine::{EngineOpts, Tone, VoiceEngine, VoiceEvent},
    link::{LinkCfg, WsSender},
};
use rynuvo_proto::voice_wire::{UP_AUDIO, VoiceKey};
use tokio::net::UdpSocket;

#[derive(Default)]
struct Shared {
    starts: Vec<Option<String>>,
    io: Option<AudioIo>,
}

struct FakeBackend(Arc<Mutex<Shared>>);

impl AudioBackend for FakeBackend {
    fn start(&mut self, input: Option<&str>, _output: Option<&str>, io: AudioIo) -> Result<(), String> {
        let mut s = self.0.lock().unwrap();
        s.starts.push(input.map(str::to_string));
        s.io = Some(io);
        Ok(())
    }
    fn stop(&mut self) {
        self.0.lock().unwrap().io = None;
    }
}

struct Rig {
    engine: VoiceEngine,
    shared: Arc<Mutex<Shared>>,
    events: Arc<Mutex<Vec<VoiceEvent>>>,
    udp: Option<Arc<UdpSocket>>,
}

async fn rig(threshold: f32, with_link: bool) -> Rig {
    let shared = Arc::new(Mutex::new(Shared::default()));
    let events = Arc::new(Mutex::new(Vec::new()));
    let ev2 = events.clone();
    let ws: WsSender = Arc::new(|_| {});
    let (udp, link) = if with_link {
        let s = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
        let cfg = LinkCfg { host: "127.0.0.1".into(), udp_port: s.local_addr().unwrap().port(), session: 9, key: VoiceKey::random() };
        (Some(s), Some(cfg))
    } else {
        (None, None)
    };
    let opts = EngineOpts { me: 1, input: Some("Mic A".into()), output: None, threshold, volumes: HashMap::new(), music_volume: 0.5 };
    let engine = VoiceEngine::start(
        Box::new(FakeBackend(shared.clone())),
        opts,
        link,
        ws,
        Arc::new(move |e| ev2.lock().unwrap().push(e)),
    );
    Rig { engine, shared, events, udp }
}

fn speech_like(n: usize) -> Vec<f32> {
    (0..n).map(|i| 0.4 * (2.0 * std::f32::consts::PI * 220.0 * i as f32 / 48_000.0).sin()).collect()
}

/// Push audio into the fake mic at real-time pace.
async fn speak(r: &Rig, secs: f32) {
    let total = (48_000.0 * secs) as usize;
    let pcm = speech_like(total);
    for chunk in pcm.chunks(480) {
        if let Some(io) = r.shared.lock().unwrap().io.as_mut() {
            io.capture.push_slice(chunk);
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

/// Count UP_AUDIO packets that reached the fake relay so far.
async fn audio_packets(udp: &UdpSocket) -> usize {
    let mut buf = [0u8; 1500];
    let mut n = 0;
    while let Ok(Ok(len)) = tokio::time::timeout(Duration::from_millis(50), udp.recv(&mut buf)).await {
        if len > 0 && buf[0] == UP_AUDIO {
            n += 1;
        }
    }
    n
}

#[tokio::test(flavor = "multi_thread")]
async fn engine_sends_only_when_gate_open() {
    let r = rig(1.0, true).await;
    let udp = r.udp.clone().unwrap();
    speak(&r, 1.0).await;
    assert_eq!(audio_packets(&udp).await, 0, "gate closed: nothing sent");
    r.engine.set_threshold(0.0);
    speak(&r, 1.0).await;
    let n = audio_packets(&udp).await;
    assert!(n >= 35, "gate open: about 50 packets per second, got {n}");
    r.engine.stop();
}

#[tokio::test(flavor = "multi_thread")]
async fn muted_engine_sends_nothing() {
    let r = rig(0.0, true).await;
    r.engine.set_muted(true);
    let udp = r.udp.clone().unwrap();
    speak(&r, 1.0).await;
    assert_eq!(audio_packets(&udp).await, 0);
    r.engine.stop();
}

#[tokio::test(flavor = "multi_thread")]
async fn deafened_engine_outputs_silence() {
    let r = rig(0.5, false).await;
    r.engine.set_deafened(true);
    r.engine.play_tone(Tone::Join);
    let mut got = 0usize;
    let mut peak = 0f32;
    let end = Instant::now() + Duration::from_millis(600);
    let mut buf = [0f32; 960];
    while Instant::now() < end {
        if let Some(io) = r.shared.lock().unwrap().io.as_mut() {
            let n = io.playback.pop_slice(&mut buf);
            got += n;
            peak = buf[..n].iter().fold(peak, |m, x| m.max(x.abs()));
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(got > 4_800, "engine keeps feeding the output ({got} samples)");
    assert_eq!(peak, 0.0, "deafened: silence only");
    r.engine.stop();
}

#[tokio::test(flavor = "multi_thread")]
async fn undeafened_engine_plays_tones() {
    let r = rig(0.5, false).await;
    r.engine.play_tone(Tone::Join);
    let mut peak = 0f32;
    let end = Instant::now() + Duration::from_millis(600);
    let mut buf = [0f32; 960];
    while Instant::now() < end {
        if let Some(io) = r.shared.lock().unwrap().io.as_mut() {
            let n = io.playback.pop_slice(&mut buf);
            peak = buf[..n].iter().fold(peak, |m, x| m.max(x.abs()));
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(peak > 0.05, "tone is audible ({peak})");
    r.engine.stop();
}

#[tokio::test(flavor = "multi_thread")]
async fn engine_survives_backend_error() {
    let r = rig(0.5, false).await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    let errors = r.shared.lock().unwrap().io.as_ref().unwrap().errors.clone();
    errors.send("device unplugged".into()).unwrap();
    let end = Instant::now() + Duration::from_secs(3);
    while Instant::now() < end && r.shared.lock().unwrap().starts.len() < 2 {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let starts = r.shared.lock().unwrap().starts.clone();
    assert_eq!(starts, vec![Some("Mic A".to_string()), None], "restarted on default devices");
    assert!(r.events.lock().unwrap().iter().any(|e| matches!(e, VoiceEvent::DeviceError { .. })));
    r.engine.stop();
}

#[tokio::test(flavor = "multi_thread")]
async fn engine_reports_level() {
    let r = rig(0.5, false).await;
    speak(&r, 0.5).await;
    let ev = r.events.lock().unwrap().clone();
    assert!(ev.iter().filter(|e| matches!(e, VoiceEvent::Level { .. })).count() >= 3, "level every 100 ms");
    r.engine.stop();
}

#[tokio::test(flavor = "multi_thread")]
async fn stop_returns_while_mic_floods() {
    let r = rig(0.5, false).await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    let shared = r.shared.clone();
    let flood = std::thread::spawn(move || {
        let chunk = speech_like(48_000);
        let end = Instant::now() + Duration::from_secs(4);
        while Instant::now() < end {
            // Keep the 1 s mic buffer full: far faster than any DSP.
            match shared.lock().unwrap().io.as_mut() {
                Some(io) => {
                    io.capture.push_slice(&chunk);
                }
                None => return,
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    });
    tokio::time::sleep(Duration::from_millis(300)).await;
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        r.engine.stop();
        let _ = tx.send(());
    });
    assert!(rx.recv_timeout(Duration::from_secs(1)).is_ok(), "stop() must return while the mic outpaces the DSP");
    flood.join().unwrap();
}
