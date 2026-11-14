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
/// The source went away on its own (window closed, display unplugged, capture error).
pub type EndedSink = Arc<dyn Fn(String) + Send + Sync>;

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

/// BGRA (`stride` bytes per source row) to tightly packed `dw`×`dh` BGRA. Same size or smaller
/// only: an exact copy (cropping the right/bottom edge) when no scaling is needed, otherwise a
/// bilinear sample, which keeps downscaled text readable.
#[allow(dead_code)] // used by the Windows capture
pub fn scale_bgra(src: &[u8], sw: u32, sh: u32, stride: u32, dw: u32, dh: u32) -> Vec<u8> {
    let (sw, sh, stride, dw, dh) = (sw as usize, sh as usize, stride as usize, dw as usize, dh as usize);
    let mut out = vec![0u8; dw * dh * 4];
    if dw <= sw && dh <= sh && (sw - dw) <= 1 && (sh - dh) <= 1 {
        for y in 0..dh {
            out[y * dw * 4..(y + 1) * dw * 4].copy_from_slice(&src[y * stride..y * stride + dw * 4]);
        }
        return out;
    }
    let fx = sw as f32 / dw as f32;
    let fy = sh as f32 / dh as f32;
    for y in 0..dh {
        let sy = ((y as f32 + 0.5) * fy - 0.5).clamp(0.0, (sh - 1) as f32);
        let (y0, wy) = (sy as usize, sy.fract());
        let y1 = (y0 + 1).min(sh - 1);
        for x in 0..dw {
            let sx = ((x as f32 + 0.5) * fx - 0.5).clamp(0.0, (sw - 1) as f32);
            let (x0, wx) = (sx as usize, sx.fract());
            let x1 = (x0 + 1).min(sw - 1);
            let o = (y * dw + x) * 4;
            for c in 0..4 {
                let p = |xx: usize, yy: usize| src[yy * stride + xx * 4 + c] as f32;
                let top = p(x0, y0) * (1.0 - wx) + p(x1, y0) * wx;
                let bot = p(x0, y1) * (1.0 - wx) + p(x1, y1) * wx;
                out[o + c] = (top * (1.0 - wy) + bot * wy + 0.5) as u8;
            }
        }
    }
    out
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

pub fn start(source_id: &str, opts: CaptureOpts, video: VideoSink, audio: AudioSink, ended: EndedSink) -> Result<Capture, String> {
    #[cfg(any(target_os = "macos", windows))]
    return os::start(source_id, opts, video, audio, ended).map(|inner| Capture { inner });
    #[cfg(not(any(target_os = "macos", windows)))]
    {
        let _ = (source_id, opts, video, audio, ended);
        Err("Screen share is not supported on this system".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scale_bgra_copies_crops_and_downscales() {
        // 3x2 source with row padding (stride 16), cropped to 2x2 without scaling.
        let mut src = vec![0u8; 16 * 2];
        for y in 0..2 {
            for x in 0..3 {
                let o = y * 16 + x * 4;
                src[o..o + 4].copy_from_slice(&[(x * 10 + y) as u8, 1, 2, 255]);
            }
        }
        let out = scale_bgra(&src, 3, 2, 16, 2, 2);
        assert_eq!(out.len(), 2 * 2 * 4);
        assert_eq!(out[0], 0);
        assert_eq!(out[4], 10);
        assert_eq!(out[8], 1);
        // Uniform 4x4 downscaled to 2x2 keeps the colour.
        let src = [9u8, 8, 7, 255].repeat(16);
        let out = scale_bgra(&src, 4, 4, 16, 2, 2);
        assert_eq!(out, [9u8, 8, 7, 255].repeat(4));
    }

    #[test]
    fn fit_keeps_aspect_and_even() {
        assert_eq!(fit(3024, 1964, 1920, 1080), (1662, 1080));
        assert_eq!(fit(1280, 720, 1920, 1080), (1280, 720));
        assert_eq!(fit(2560, 1440, 1920, 1080), (1920, 1080));
        let (w, h) = fit(1001, 777, 1920, 1080);
        assert!(w % 2 == 0 && h % 2 == 0);
    }
}

/// Whether the OS lets this app capture the screen, checked without asking the user.
pub fn permission() -> &'static str {
    #[cfg(target_os = "macos")]
    {
        #[link(name = "CoreGraphics", kind = "framework")]
        unsafe extern "C" {
            fn CGPreflightScreenCaptureAccess() -> bool;
        }
        // SAFETY: no arguments, only reads the TCC state.
        if unsafe { CGPreflightScreenCaptureAccess() } { "granted" } else { "denied" }
    }
    #[cfg(not(target_os = "macos"))]
    {
        "granted" // Windows has no capture permission for desktop apps
    }
}
