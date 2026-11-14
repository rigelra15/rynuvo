//! Screen and window capture through the OS capture APIs only (no hooks into other processes).
use std::sync::Arc;

use serde::Serialize;

use super::encode::Frame;

#[cfg(target_os = "macos")]
mod mac;
#[cfg(target_os = "macos")]
use mac as os;
#[cfg(windows)]
mod win;
#[cfg(windows)]
use win as os;

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Source {
    /// `screen:<id>` or `window:<id>`.
    pub id: String,
    pub kind: SourceKind,
    pub name: String,
    pub app: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SourceKind {
    Screen,
    Window,
}

pub type VideoSink = Arc<dyn Fn(Frame) + Send + Sync>;
/// Interleaved stereo f32 at 48 kHz.
pub type AudioSink = Arc<dyn Fn(&[f32]) + Send + Sync>;

#[derive(Debug, Clone, Copy)]
pub struct CaptureOpts {
    pub max_width: u32,
    pub max_height: u32,
    pub fps: u32,
}

pub const PERMISSION: &str = "Allow Rynuvo in System Settings → Privacy & Security → Screen & System Audio Recording, then try again";

/// Largest even size with the source's aspect ratio that fits in `max`.
pub fn fit(src_w: u32, src_h: u32, max_w: u32, max_h: u32) -> (u32, u32) {
    if src_w == 0 || src_h == 0 {
        return (max_w & !1, max_h & !1);
    }
    let scale = (max_w as f64 / src_w as f64).min(max_h as f64 / src_h as f64).min(1.0);
    let w = ((src_w as f64 * scale) as u32).max(2) & !1;
    let h = ((src_h as f64 * scale) as u32).max(2) & !1;
    (w, h)
}

pub struct Capture {
    #[cfg(any(target_os = "macos", windows))]
    inner: os::Capture,
}

impl Capture {
    pub fn stop(self) {
        #[cfg(any(target_os = "macos", windows))]
        self.inner.stop();
    }
}

pub fn sources() -> Result<Vec<Source>, String> {
    #[cfg(any(target_os = "macos", windows))]
    return os::sources();
    #[cfg(not(any(target_os = "macos", windows)))]
    Err("Screen share is not supported on this system".into())
}

pub fn start(source_id: &str, opts: CaptureOpts, video: VideoSink, audio: AudioSink) -> Result<Capture, String> {
    #[cfg(any(target_os = "macos", windows))]
    return os::start(source_id, opts, video, audio).map(|inner| Capture { inner });
    #[cfg(not(any(target_os = "macos", windows)))]
    {
        let _ = (source_id, opts, video, audio);
        Err("Screen share is not supported on this system".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fit_keeps_aspect_and_even() {
        assert_eq!(fit(3024, 1964, 1920, 1080), (1662, 1080));
        assert_eq!(fit(1280, 720, 1920, 1080), (1280, 720));
        assert_eq!(fit(2560, 1440, 1920, 1080), (1920, 1080));
        let (w, h) = fit(1001, 777, 1920, 1080);
        assert!(w % 2 == 0 && h % 2 == 0);
    }
}
