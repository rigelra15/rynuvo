//! Windows capture: Windows.Graphics.Capture for screens and windows (works for borderless and
//! most exclusive-fullscreen games through monitor capture), WASAPI process loopback for sound.
use std::{
    collections::VecDeque,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::JoinHandle,
    time::Duration,
};

use wasapi::{AudioClient, Direction, SampleType, StreamMode, WaveFormat, initialize_mta};
use windows_capture::{
    capture::{CaptureControl, Context, GraphicsCaptureApiHandler},
    frame::Frame as WgcFrame,
    graphics_capture_api::InternalCaptureControl,
    monitor::Monitor,
    settings::{
        ColorFormat, CursorCaptureSettings, DirtyRegionSettings, DrawBorderSettings, MinimumUpdateIntervalSettings,
        SecondaryWindowSettings, Settings,
    },
    window::Window,
};

use super::{AudioSink, CaptureOpts, EndedSink, Source, SourceKind, VideoSink, fit, scale_bgra};
use crate::stream::encode::Frame;

pub struct Capture {
    video: CaptureControl<Handler, String>,
    audio_stop: Arc<AtomicBool>,
    audio: Option<JoinHandle<()>>,
}

impl Capture {
    pub fn stop(self) {
        self.audio_stop.store(true, Ordering::Relaxed);
        let _ = self.video.stop();
        if let Some(a) = self.audio {
            let _ = a.join();
        }
    }
}

pub fn sources() -> Result<Vec<Source>, String> {
    let mut out = Vec::new();
    let monitors = Monitor::enumerate().map_err(|e| format!("Could not list screens: {e}"))?;
    for (i, m) in monitors.iter().enumerate() {
        let name = m.name().unwrap_or_else(|_| format!("Screen {}", i + 1));
        let size = match (m.width(), m.height()) {
            (Ok(w), Ok(h)) => format!(" ({w}×{h})"),
            _ => String::new(),
        };
        out.push(Source { id: format!("screen:{}", i + 1), kind: SourceKind::Screen, name: format!("{name}{size}"), app: None });
    }
    let me = std::process::id();
    for w in Window::enumerate().map_err(|e| format!("Could not list windows: {e}"))? {
        if w.process_id().is_ok_and(|p| p == me) {
            continue;
        }
        let Ok(title) = w.title() else { continue };
        if title.trim().is_empty() {
            continue;
        }
        let app = w.process_name().ok().map(|n| n.trim_end_matches(".exe").to_string());
        out.push(Source { id: format!("window:{}", w.as_raw_hwnd() as usize), kind: SourceKind::Window, name: title, app });
    }
    Ok(out)
}

struct Flags {
    video: VideoSink,
    opts: CaptureOpts,
    ended: EndedSink,
}

struct Handler {
    video: VideoSink,
    opts: CaptureOpts,
    ended: EndedSink,
}

impl GraphicsCaptureApiHandler for Handler {
    type Flags = Flags;
    type Error = String;

    fn new(ctx: Context<Flags>) -> Result<Self, String> {
        Ok(Handler { video: ctx.flags.video, opts: ctx.flags.opts, ended: ctx.flags.ended })
    }

    fn on_frame_arrived(&mut self, frame: &mut WgcFrame, _c: InternalCaptureControl) -> Result<(), String> {
        let mut buf = match frame.buffer() {
            Ok(b) => b,
            Err(_) => return Ok(()), // a frame mid-resize; the next one will do
        };
        let (sw, sh, stride) = (buf.width(), buf.height(), buf.row_pitch());
        if sw < 2 || sh < 2 {
            return Ok(());
        }
        let (dw, dh) = fit(sw, sh, self.opts.max_width, self.opts.max_height);
        let data = scale_bgra(buf.as_raw_buffer(), sw, sh, stride, dw, dh);
        (self.video)(Frame::Bgra { width: dw, height: dh, data });
        Ok(())
    }

    fn on_closed(&mut self) -> Result<(), String> {
        (self.ended)("The shared window was closed".into());
        Ok(())
    }
}

struct ThumbFlags {
    max: (u32, u32),
    out: std::sync::mpsc::SyncSender<Result<Vec<u8>, String>>,
}

/// Takes the first frame, scales it down, and stops.
struct Thumb {
    max: (u32, u32),
    out: Option<std::sync::mpsc::SyncSender<Result<Vec<u8>, String>>>,
}

impl GraphicsCaptureApiHandler for Thumb {
    type Flags = ThumbFlags;
    type Error = String;

    fn new(ctx: Context<ThumbFlags>) -> Result<Self, String> {
        Ok(Thumb { max: ctx.flags.max, out: Some(ctx.flags.out) })
    }

    fn on_frame_arrived(&mut self, frame: &mut WgcFrame, control: InternalCaptureControl) -> Result<(), String> {
        let Ok(mut buf) = frame.buffer() else { return Ok(()) };
        let (sw, sh, stride) = (buf.width(), buf.height(), buf.row_pitch());
        if sw < 2 || sh < 2 {
            return Ok(());
        }
        let (dw, dh) = fit(sw, sh, self.max.0, self.max.1);
        let data = scale_bgra(buf.as_raw_buffer(), sw, sh, stride, dw, dh);
        if let Some(out) = self.out.take() {
            let _ = out.try_send(super::jpeg_from_bgra(dw, dh, &data));
        }
        control.stop();
        Ok(())
    }

    fn on_closed(&mut self) -> Result<(), String> {
        Ok(())
    }
}

pub fn thumbnail(source_id: &str, max_w: u32, max_h: u32) -> Result<Vec<u8>, String> {
    let (kind, id) = source_id.split_once(':').ok_or("Unknown source")?;
    let (tx, rx) = std::sync::mpsc::sync_channel(1);
    let flags = ThumbFlags { max: (max_w, max_h), out: tx };
    let interval = MinimumUpdateIntervalSettings::Default;
    let err = |e: String| format!("Could not take a preview: {e}");
    let control = match kind {
        "screen" => {
            let idx: usize = id.parse().map_err(|_| "Unknown screen")?;
            let m = Monitor::from_index(idx).map_err(|_| "That screen is no longer connected")?;
            let s = Settings::new(m, CursorCaptureSettings::WithoutCursor, DrawBorderSettings::WithoutBorder, SecondaryWindowSettings::Default, interval, DirtyRegionSettings::Default, ColorFormat::Bgra8, flags);
            Thumb::start_free_threaded(s).map_err(|e| err(e.to_string()))?
        }
        "window" => {
            let hwnd: usize = id.parse().map_err(|_| "Unknown window")?;
            let w = Window::from_raw_hwnd(hwnd as *mut std::ffi::c_void);
            if !w.is_valid() {
                return Err("That window was closed".into());
            }
            let s = Settings::new(w, CursorCaptureSettings::WithoutCursor, DrawBorderSettings::WithoutBorder, SecondaryWindowSettings::Include, interval, DirtyRegionSettings::Default, ColorFormat::Bgra8, flags);
            Thumb::start_free_threaded(s).map_err(|e| err(e.to_string()))?
        }
        _ => return Err("Unknown source".into()),
    };
    // A minimized window sends no frames: give up rather than wait forever.
    let got = rx.recv_timeout(Duration::from_millis(1500));
    let _ = control.stop();
    got.map_err(|_| "No preview: the window may be minimized".to_string())?
}

pub fn start(source_id: &str, opts: CaptureOpts, video: VideoSink, audio: AudioSink, ended: EndedSink) -> Result<Capture, String> {
    let (kind, id) = source_id.split_once(':').ok_or("Unknown source")?;
    let interval = MinimumUpdateIntervalSettings::Custom(Duration::from_micros(1_000_000 / opts.fps.max(1) as u64));
    let flags = Flags { video, opts, ended };
    let gone = |e: String| format!("Could not start the capture: {e}");
    let (control, loopback) = match kind {
        "screen" => {
            let idx: usize = id.parse().map_err(|_| "Unknown screen")?;
            let m = Monitor::from_index(idx).map_err(|_| "That screen is no longer connected")?;
            let s = Settings::new(
                m,
                if opts.cursor { CursorCaptureSettings::WithCursor } else { CursorCaptureSettings::WithoutCursor },
                DrawBorderSettings::WithoutBorder,
                SecondaryWindowSettings::Default,
                interval,
                DirtyRegionSettings::Default,
                ColorFormat::Bgra8,
                flags,
            );
            // Everything the PC plays except Rynuvo itself (no voice chat echo).
            (Handler::start_free_threaded(s).map_err(|e| gone(e.to_string()))?, (std::process::id(), false))
        }
        "window" => {
            let hwnd: usize = id.parse().map_err(|_| "Unknown window")?;
            let w = Window::from_raw_hwnd(hwnd as *mut std::ffi::c_void);
            if !w.is_valid() {
                return Err("That window was closed".into());
            }
            let pid = w.process_id().map_err(|_| "That window was closed")?;
            let s = Settings::new(
                w,
                if opts.cursor { CursorCaptureSettings::WithCursor } else { CursorCaptureSettings::WithoutCursor },
                DrawBorderSettings::WithoutBorder,
                SecondaryWindowSettings::Include,
                interval,
                DirtyRegionSettings::Default,
                ColorFormat::Bgra8,
                flags,
            );
            // Only that app's sound (and its child processes, as browsers play from a child).
            (Handler::start_free_threaded(s).map_err(|e| gone(e.to_string()))?, (pid, true))
        }
        _ => return Err("Unknown source".into()),
    };
    let audio_stop = Arc::new(AtomicBool::new(false));
    let stop = audio_stop.clone();
    let audio = std::thread::Builder::new()
        .name("rynuvo-loopback".into())
        .spawn(move || {
            if let Err(e) = loopback_loop(loopback.0, loopback.1, audio, &stop) {
                tracing::warn!("stream: app sound capture unavailable: {e}");
            }
        })
        .ok();
    Ok(Capture { video: control, audio_stop, audio })
}

fn loopback_loop(pid: u32, include_tree: bool, sink: AudioSink, stop: &AtomicBool) -> Result<(), String> {
    let _ = initialize_mta();
    let format = WaveFormat::new(32, 32, SampleType::Float, 48_000, 2, None);
    let mut client = AudioClient::new_application_loopback_client(pid, include_tree).map_err(|e| e.to_string())?;
    client
        .initialize_client(&format, Direction::Capture, StreamMode::EventsShared { autoconvert: true, buffer_duration_hns: 0 }, None)
        .map_err(|e| e.to_string())?;
    let event = client.set_get_eventhandle().map_err(|e| e.to_string())?;
    let capture = client.get_audiocaptureclient().map_err(|e| e.to_string())?;
    client.start_stream().map_err(|e| e.to_string())?;
    let mut q: VecDeque<u8> = VecDeque::new();
    let mut samples: Vec<f32> = Vec::new();
    while !stop.load(Ordering::Relaxed) {
        if event.wait_for_event(200).is_err() {
            continue; // silence: process loopback delivers nothing while the app is quiet
        }
        if capture.read_from_device_to_deque(&mut q).is_err() {
            break;
        }
        let whole = q.len() / 8 * 8;
        if whole == 0 {
            continue;
        }
        samples.clear();
        let bytes: Vec<u8> = q.drain(..whole).collect();
        samples.extend(bytes.chunks_exact(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])));
        sink(&samples);
    }
    let _ = client.stop_stream();
    Ok(())
}
