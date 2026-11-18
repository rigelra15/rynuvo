//! ScreenCaptureKit: displays and windows, with system audio minus Rynuvo's own sound.
use screencapturekit::{prelude::*, screenshot_manager::SCScreenshotManager};

use super::{AudioSink, CaptureOpts, EndedSink, PERMISSION, Source, SourceKind, VideoSink, fit};
use crate::stream::encode::Frame;

pub struct Capture {
    stream: SCStream,
}

impl Capture {
    pub fn stop(self) {
        let _ = self.stream.stop_capture();
    }
}

fn content() -> Result<SCShareableContent, String> {
    SCShareableContent::get().map_err(|e| {
        tracing::warn!("screen capture content: {e:?}");
        PERMISSION.to_string()
    })
}

pub fn sources() -> Result<Vec<Source>, String> {
    let c = content()?;
    let snap = c.snapshot().ok_or("Could not list screens and windows")?;
    let me = std::process::id() as i32;
    let mut out: Vec<Source> = snap
        .displays
        .iter()
        .enumerate()
        .map(|(i, d)| Source {
            id: format!("screen:{}", d.display_id),
            kind: SourceKind::Screen,
            name: if snap.displays.len() > 1 { format!("Screen {}", i + 1) } else { "Entire screen".into() },
            app: None,
        })
        .collect();
    for w in snap.windows.iter() {
        let app = w.owning_app_index.and_then(|i| snap.applications.get(i));
        let title = w.title.clone().unwrap_or_default();
        if !w.is_on_screen || w.window_layer != 0 || w.frame.size.width < 120.0 || w.frame.size.height < 80.0 || title.is_empty() {
            continue;
        }
        if app.is_some_and(|a| a.process_id == me) {
            continue;
        }
        out.push(Source {
            id: format!("window:{}", w.window_id),
            kind: SourceKind::Window,
            name: title,
            app: app.map(|a| a.application_name.clone()),
        });
    }
    Ok(out)
}

struct Handler {
    video: VideoSink,
    audio: AudioSink,
}

impl SCStreamOutputTrait for Handler {
    fn did_output_sample_buffer(&self, sample: CMSampleBuffer, of_type: SCStreamOutputType) {
        match of_type {
            SCStreamOutputType::Screen => {
                let Some(pb) = sample.pixel_buffer() else { return }; // idle frames carry no image
                if let Some(s) = pb.io_surface() {
                    (self.video)(Frame::Surface(s));
                }
            }
            SCStreamOutputType::Audio => {
                let Ok(list) = sample.audio_buffer_list() else { return };
                let bufs: Vec<&[u8]> = (0..).map_while(|i| list.get(i)).map(|b| b.data()).collect();
                let as_f32 = |b: &[u8]| -> Vec<f32> { b.chunks_exact(4).map(|c| f32::from_ne_bytes([c[0], c[1], c[2], c[3]])).collect() };
                let inter: Vec<f32> = match bufs.as_slice() {
                    [l, r] => as_f32(l).into_iter().zip(as_f32(r)).flat_map(|(a, b)| [a, b]).collect(),
                    [one] => {
                        let ch = list.get(0).map_or(1, |b| b.number_channels);
                        let v = as_f32(one);
                        if ch >= 2 { v } else { v.into_iter().flat_map(|a| [a, a]).collect() }
                    }
                    _ => return,
                };
                (self.audio)(&inter);
            }
            _ => {}
        }
    }
}

/// The capture filter for `screen:<id>` / `window:<id>`, and the source's size in pixels.
fn filter_for(c: &SCShareableContent, source_id: &str) -> Result<(SCContentFilter, u32, u32, bool), String> {
    let (kind, id) = source_id.split_once(':').ok_or("Unknown source")?;
    let id: u32 = id.parse().map_err(|_| "Unknown source")?;
    match kind {
        "screen" => {
            let d = c.displays().into_iter().find(|d| d.display_id() == id).ok_or("That screen is gone")?;
            let me = std::process::id() as i32;
            // Rynuvo's own windows stay out of a full-screen share.
            let mine: Vec<SCWindow> = c.windows().into_iter().filter(|w| w.owning_application().is_some_and(|a| a.process_id() == me)).collect();
            let refs: Vec<&SCWindow> = mine.iter().collect();
            let (w, h) = (d.width() as u32 * 2, d.height() as u32 * 2);
            Ok((SCContentFilter::create().with_display(&d).with_excluding_windows(&refs).build().map_err(|e| format!("{e:?}"))?, w, h, false))
        }
        "window" => {
            let w = c.windows().into_iter().find(|w| w.window_id() == id).ok_or("That window was closed")?;
            let f = w.frame();
            let (sw, sh) = (f.size.width as u32 * 2, f.size.height as u32 * 2);
            Ok((SCContentFilter::create().with_window(&w).build().map_err(|e| format!("{e:?}"))?, sw, sh, true))
        }
        _ => Err("Unknown source".into()),
    }
}

pub fn thumbnail(source_id: &str, max_w: u32, max_h: u32) -> Result<Vec<u8>, String> {
    let c = content()?;
    let (filter, src_w, src_h, _) = filter_for(&c, source_id)?;
    let (w, h) = fit(src_w, src_h, max_w, max_h);
    let config = SCStreamConfiguration::new().with_width(w).with_height(h).with_scales_to_fit(true).with_shows_cursor(false);
    let img = SCScreenshotManager::capture_image(&filter, &config).map_err(|e| format!("{e:?}"))?;
    // The image only leaves CoreGraphics as a file: a private temp PNG, read back and removed.
    let path = std::env::temp_dir().join(format!("rynuvo-thumb-{}-{}.png", std::process::id(), next_thumb_id()));
    img.save_png(&path).map_err(|e| e.to_string())?;
    let png = std::fs::read(&path);
    let _ = std::fs::remove_file(&path);
    let decoded = image::load_from_memory(&png.map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    super::jpeg(&decoded.to_rgb8())
}

/// Unique enough for concurrent thumbnails in one process.
fn next_thumb_id() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    N.fetch_add(1, Ordering::Relaxed)
}

pub fn start(source_id: &str, opts: CaptureOpts, video: VideoSink, audio: AudioSink, ended: EndedSink) -> Result<Capture, String> {
    let c = content()?;
    let (filter, src_w, src_h, is_window) = filter_for(&c, source_id)?;
    let (w, h) = fit(src_w, src_h, opts.max_width, opts.max_height);
    let config = SCStreamConfiguration::new()
        .with_width(w)
        .with_height(h)
        .with_scales_to_fit(true)
        .with_shows_cursor(opts.cursor)
        .with_minimum_frame_interval(&CMTime::new(1, opts.fps as i32))
        .with_queue_depth(5)
        .with_captures_audio(true)
        .with_excludes_current_process_audio(true)
        .with_sample_rate(48_000)
        .with_channel_count(2);
    let on_error = screencapturekit::stream::ErrorHandler::new(move |e| {
        tracing::warn!("capture stopped: {e}");
        let why = e.to_string();
        ended(if is_window {
            "The shared window was closed".into()
        } else if why.contains("No capture source") || why.contains("displays or windows") {
            // macOS lost the display: it slept or locked, or a monitor was unplugged or rearranged.
            "Screen sharing stopped: the screen went to sleep, locked, or was disconnected. Share it again when it's back.".into()
        } else {
            format!("Screen capture stopped ({e})")
        });
    });
    let mut stream = SCStream::new_with_delegate(&filter, &config, on_error).map_err(|e| format!("{e:?}"))?;
    let h1 = Handler { video: video.clone(), audio: audio.clone() };
    let h2 = Handler { video, audio };
    stream.add_output_handler(h1, SCStreamOutputType::Screen).map_err(|e| format!("{e:?}"))?;
    stream.add_output_handler(h2, SCStreamOutputType::Audio).map_err(|e| format!("{e:?}"))?;
    stream.start_capture().map_err(|e| {
        tracing::warn!("start capture: {e:?}");
        PERMISSION.to_string()
    })?;
    tracing::info!("capture started: {source_id} at {w}×{h}@{}", opts.fps);
    Ok(Capture { stream })
}
