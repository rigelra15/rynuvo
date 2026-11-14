use image::{ImageFormat, RgbaImage};
use rynuvo_desktop_lib::core::image::prepare;
use std::io::Cursor;

fn encode(w: u32, h: u32, fmt: ImageFormat) -> Vec<u8> {
    let img = RgbaImage::from_fn(w, h, |x, y| image::Rgba([(x % 255) as u8, (y % 255) as u8, 128, 255]));
    let mut out = Cursor::new(Vec::new());
    if fmt == ImageFormat::Jpeg {
        image::DynamicImage::ImageRgba8(img).to_rgb8().write_to(&mut out, fmt).unwrap();
    } else {
        img.write_to(&mut out, fmt).unwrap();
    }
    out.into_inner()
}

#[test]
fn prepare_downscales_large_png() {
    let p = prepare(&encode(5000, 1000, ImageFormat::Png)).unwrap();
    assert_eq!(p.mime, "image/webp");
    assert_eq!((p.width, p.height), (2560, 512));
    let back = image::load_from_memory_with_format(&p.bytes, ImageFormat::WebP).unwrap();
    assert_eq!((back.width(), back.height()), (2560, 512));
}

#[test]
fn prepare_keeps_small_dims() {
    let p = prepare(&encode(300, 200, ImageFormat::Jpeg)).unwrap();
    assert_eq!((p.width, p.height, p.mime), (300, 200, "image/webp"));
}

#[test]
fn prepare_gif_passthrough() {
    let gif = encode(40, 30, ImageFormat::Gif);
    let p = prepare(&gif).unwrap();
    assert_eq!(p.mime, "image/gif");
    assert_eq!(p.bytes, gif);
    assert_eq!((p.width, p.height), (40, 30));
}

#[test]
fn prepare_rejects_text() {
    assert_eq!(prepare(b"hello world").unwrap_err(), "Unsupported image format");
}
