//! Sound devices through cpal: listing, and the real `AudioBackend`.
use std::{
    collections::VecDeque,
    sync::mpsc,
    thread::JoinHandle,
};

use cpal::{
    Device, ErrorKind, SampleFormat, SizedSample, StreamConfig,
    traits::{DeviceTrait, HostTrait, StreamTrait},
};
use ringbuf::traits::{Consumer, Producer};
use serde::Serialize;

use super::backend::{AudioBackend, AudioIo, Resampler};

#[derive(Debug, Clone, Default, Serialize)]
pub struct DeviceList {
    pub inputs: Vec<String>,
    pub outputs: Vec<String>,
}

pub fn list() -> DeviceList {
    let host = cpal::default_host();
    let names = |it: Result<_, _>| -> Vec<String> {
        let it: Box<dyn Iterator<Item = Device>> = match it {
            Ok(i) => i,
            Err(e) => {
                tracing::warn!("listing devices: {e:?}");
                return Vec::new();
            }
        };
        let mut v: Vec<String> = it.map(|d| d.to_string()).collect();
        v.dedup();
        v
    };
    DeviceList {
        inputs: names(host.input_devices().map(|i| Box::new(i) as Box<dyn Iterator<Item = Device>>)),
        outputs: names(host.output_devices().map(|i| Box::new(i) as Box<dyn Iterator<Item = Device>>)),
    }
}

fn find(input: bool, name: Option<&str>) -> Result<Device, String> {
    let host = cpal::default_host();
    let default = || if input { host.default_input_device() } else { host.default_output_device() };
    let what = if input { "microphone" } else { "speaker or headphones" };
    match name {
        None => default().ok_or_else(|| format!("No {what} found")),
        Some(n) => {
            let found = if input { host.input_devices().ok().and_then(|mut i| i.find(|d| d.to_string() == n)) } else { host.output_devices().ok().and_then(|mut i| i.find(|d| d.to_string() == n)) };
            found.ok_or_else(|| format!("{n} is not connected"))
        }
    }
}

/// Pick 48 kHz when the device offers it, otherwise its default configuration.
fn pick_config(dev: &Device, input: bool) -> Result<(StreamConfig, SampleFormat), String> {
    let ranges: Vec<_> = if input { dev.supported_input_configs().map(|i| i.collect()) } else { dev.supported_output_configs().map(|i| i.collect()) }
        .map_err(|e| format!("Can't open the device: {e}"))?;
    let rank = |f: SampleFormat| match f {
        SampleFormat::F32 => 0,
        SampleFormat::I16 => 1,
        SampleFormat::I32 => 2,
        SampleFormat::U16 => 3,
        _ => 9,
    };
    let best = ranges
        .into_iter()
        .filter(|r| rank(r.sample_format()) < 9 && r.channels() <= 8)
        .filter_map(|r| r.try_with_sample_rate(48_000))
        .min_by_key(|c| (rank(c.sample_format()), c.channels()));
    let c = match best {
        Some(c) => c,
        None => if input { dev.default_input_config() } else { dev.default_output_config() }.map_err(|e| format!("Can't open the device: {e}"))?,
    };
    Ok((c.config(), c.sample_format()))
}

fn report(errors: &mpsc::Sender<String>) -> impl FnMut(cpal::Error) + Send + 'static {
    let errors = errors.clone();
    move |e: cpal::Error| match e.kind() {
        ErrorKind::Xrun | ErrorKind::DeviceChanged => {}
        _ => {
            let _ = errors.send(e.to_string());
        }
    }
}

trait ToF32: SizedSample + Send + 'static {
    fn to_f32(self) -> f32;
    fn from_f32(x: f32) -> Self;
}
impl ToF32 for f32 {
    fn to_f32(self) -> f32 {
        self
    }
    fn from_f32(x: f32) -> Self {
        x
    }
}
impl ToF32 for i16 {
    fn to_f32(self) -> f32 {
        self as f32 / 32768.0
    }
    fn from_f32(x: f32) -> Self {
        (x.clamp(-1.0, 1.0) * 32767.0) as i16
    }
}
impl ToF32 for i32 {
    fn to_f32(self) -> f32 {
        self as f32 / 2_147_483_648.0
    }
    fn from_f32(x: f32) -> Self {
        (x.clamp(-1.0, 1.0) as f64 * 2_147_483_647.0) as i32
    }
}
impl ToF32 for u16 {
    fn to_f32(self) -> f32 {
        (self as f32 - 32768.0) / 32768.0
    }
    fn from_f32(x: f32) -> Self {
        (x.clamp(-1.0, 1.0) * 32767.0 + 32768.0) as u16
    }
}

fn input_stream<T: ToF32>(dev: &Device, cfg: StreamConfig, mut prod: ringbuf::HeapProd<f32>, errors: &mpsc::Sender<String>) -> Result<cpal::Stream, String> {
    let ch = cfg.channels.max(1) as usize;
    let mut rs = Resampler::new(cfg.sample_rate, 48_000);
    let mut mono = Vec::with_capacity(4096);
    let mut out = Vec::with_capacity(4096);
    dev.build_input_stream::<T, _, _>(
        cfg,
        move |data: &[T], _| {
            mono.clear();
            mono.extend(data.chunks(ch).map(|f| f.iter().map(|s| s.to_f32()).sum::<f32>() / ch as f32));
            if rs.is_identity() {
                prod.push_slice(&mono);
            } else {
                out.clear();
                rs.process(&mono, &mut out);
                prod.push_slice(&out);
            }
        },
        report(errors),
        None,
    )
    .map_err(|e| format!("Can't open the microphone: {e}"))
}

fn output_stream<T: ToF32>(dev: &Device, cfg: StreamConfig, mut cons: ringbuf::HeapCons<f32>, errors: &mpsc::Sender<String>) -> Result<cpal::Stream, String> {
    let ch = cfg.channels.max(1) as usize;
    let mut rs = Resampler::new(48_000, cfg.sample_rate);
    let mut pending: VecDeque<f32> = VecDeque::with_capacity(8192);
    let mut chunk = [0f32; 480];
    let mut res = Vec::with_capacity(1024);
    dev.build_output_stream::<T, _, _>(
        cfg,
        move |data: &mut [T], _| {
            let frames = data.len() / ch;
            while pending.len() < frames {
                let n = cons.pop_slice(&mut chunk);
                if n == 0 {
                    break;
                }
                res.clear();
                rs.process(&chunk[..n], &mut res);
                pending.extend(res.iter());
            }
            for f in data.chunks_mut(ch) {
                let s = T::from_f32(pending.pop_front().unwrap_or(0.0));
                f.fill(s);
            }
        },
        report(errors),
        None,
    )
    .map_err(|e| format!("Can't open the speaker: {e}"))
}

fn open(input: Option<&str>, output: Option<&str>, io: AudioIo) -> Result<(cpal::Stream, cpal::Stream), String> {
    let AudioIo { capture, playback, errors } = io;
    let idev = find(true, input)?;
    let odev = find(false, output)?;
    let (icfg, ifmt) = pick_config(&idev, true)?;
    let (ocfg, ofmt) = pick_config(&odev, false)?;
    tracing::info!("audio in: {idev} {icfg:?} {ifmt:?}; out: {odev} {ocfg:?} {ofmt:?}");
    let i = match ifmt {
        SampleFormat::I16 => input_stream::<i16>(&idev, icfg, capture, &errors)?,
        SampleFormat::I32 => input_stream::<i32>(&idev, icfg, capture, &errors)?,
        SampleFormat::U16 => input_stream::<u16>(&idev, icfg, capture, &errors)?,
        _ => input_stream::<f32>(&idev, icfg, capture, &errors)?,
    };
    let o = match ofmt {
        SampleFormat::I16 => output_stream::<i16>(&odev, ocfg, playback, &errors)?,
        SampleFormat::I32 => output_stream::<i32>(&odev, ocfg, playback, &errors)?,
        SampleFormat::U16 => output_stream::<u16>(&odev, ocfg, playback, &errors)?,
        _ => output_stream::<f32>(&odev, ocfg, playback, &errors)?,
    };
    i.play().map_err(|e| format!("Can't start the microphone: {e}"))?;
    o.play().map_err(|e| format!("Can't start the speaker: {e}"))?;
    Ok((i, o))
}

/// Real sound card. Streams live on their own thread, which owns them until `stop`.
#[derive(Default)]
pub struct CpalBackend {
    running: Option<(mpsc::Sender<()>, JoinHandle<()>)>,
}

impl AudioBackend for CpalBackend {
    fn start(&mut self, input: Option<&str>, output: Option<&str>, io: AudioIo) -> Result<(), String> {
        self.stop();
        let (input, output) = (input.map(str::to_string), output.map(str::to_string));
        let (ready_tx, ready_rx) = mpsc::channel();
        let (stop_tx, stop_rx) = mpsc::channel::<()>();
        let h = std::thread::Builder::new()
            .name("rynuvo-audio".into())
            .spawn(move || match open(input.as_deref(), output.as_deref(), io) {
                Ok(streams) => {
                    let _ = ready_tx.send(Ok(()));
                    let _ = stop_rx.recv();
                    drop(streams);
                }
                Err(e) => {
                    let _ = ready_tx.send(Err(e));
                }
            })
            .map_err(|e| e.to_string())?;
        match ready_rx.recv().unwrap_or_else(|_| Err("Audio thread stopped".into())) {
            Ok(()) => {
                self.running = Some((stop_tx, h));
                Ok(())
            }
            Err(e) => {
                let _ = h.join();
                Err(e)
            }
        }
    }

    fn stop(&mut self) {
        if let Some((tx, h)) = self.running.take() {
            let _ = tx.send(());
            let _ = h.join();
        }
    }
}

impl Drop for CpalBackend {
    fn drop(&mut self) {
        self.stop();
    }
}
