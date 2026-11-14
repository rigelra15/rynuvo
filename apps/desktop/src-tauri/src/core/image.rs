//! Client-side image preparation: downscale + WebP, GIFs pass through.
use image::{GenericImageView, ImageFormat, imageops::FilterType};

pub const MAX_SIDE: u32 = 2560;
pub const MAX_BYTES: usize = 5 * 1024 * 1024;

#[derive(Debug)]
pub struct Prepared {
    pub bytes: Vec<u8>,
    pub mime: &'static str,
    pub width: u32,
    pub height: u32,
}

const UNSUPPORTED: &str = "Unsupported image format";

pub fn prepare(bytes: &[u8]) -> Result<Prepared, String> {
    let fmt = image::guess_format(bytes).map_err(|_| UNSUPPORTED.to_string())?;
    if fmt == ImageFormat::Gif {
        let img = image::load_from_memory_with_format(bytes, fmt).map_err(|_| UNSUPPORTED.to_string())?;
        if bytes.len() > MAX_BYTES {
            return Err("GIF is too large (max 5 MB)".into());
        }
        let (width, height) = img.dimensions();
        return Ok(Prepared { bytes: bytes.to_vec(), mime: "image/gif", width, height });
    }
    if !matches!(fmt, ImageFormat::Png | ImageFormat::Jpeg | ImageFormat::WebP | ImageFormat::Bmp) {
        return Err(UNSUPPORTED.into());
    }
    let img = image::load_from_memory_with_format(bytes, fmt).map_err(|_| UNSUPPORTED.to_string())?;
    let (w, h) = img.dimensions();
    let img = if w.max(h) > MAX_SIDE {
        let scale = MAX_SIDE as f64 / w.max(h) as f64;
        let (nw, nh) = (((w as f64) * scale).round().max(1.0) as u32, ((h as f64) * scale).round().max(1.0) as u32);
        img.resize_exact(nw, nh, FilterType::Lanczos3)
    } else {
        img
    };
    let rgba = img.to_rgba8();
    let (width, height) = rgba.dimensions();
    for quality in [80.0, 60.0] {
        let enc = webp::Encoder::from_rgba(&rgba, width, height);
        let out = enc.encode(quality).to_vec();
        if out.len() <= MAX_BYTES {
            return Ok(Prepared { bytes: out, mime: "image/webp", width, height });
        }
    }
    Err("Image is still too large after compression".into())
}
