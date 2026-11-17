//! Video encoders: VideoToolbox (hardware) on macOS, OpenH264 (software) everywhere as fallback.
//! Output is Annex B (start codes); key frames carry their parameter sets.
#[cfg(target_os = "macos")]
mod vt;
mod soft;
#[cfg(windows)]
pub mod mf;
pub mod nv12;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Codec {
    H264 = 1,
    Hevc = 2,
    Av1 = 3,
}

impl Codec {
    pub fn name(self) -> &'static str {
        match self {
            Codec::H264 => "h264",
            Codec::Hevc => "hevc",
            Codec::Av1 => "av1",
        }
    }
    pub fn from_name(s: &str) -> Option<Codec> {
        [Codec::H264, Codec::Hevc, Codec::Av1].into_iter().find(|c| c.name() == s)
    }
    pub fn from_u8(v: u8) -> Option<Codec> {
        [Codec::H264, Codec::Hevc, Codec::Av1].into_iter().find(|c| *c as u8 == v)
    }
    /// Preference when both sides support several: AV1, then HEVC, then H.264.
    pub fn rank(self) -> u8 {
        match self {
            Codec::Av1 => 3,
            Codec::Hevc => 2,
            Codec::H264 => 1,
        }
    }
}

/// Best codec this side can encode that every viewer can decode. H.264 is the floor.
pub fn negotiate(encodable: &[Codec], decodable: &[&[Codec]]) -> Codec {
    encodable
        .iter()
        .copied()
        .filter(|c| decodable.iter().all(|d| d.contains(c)))
        .max_by_key(|c| c.rank())
        .unwrap_or(Codec::H264)
}

/// One captured picture.
pub enum Frame {
    /// Tightly packed BGRA (tests, Windows capture).
    Bgra { width: u32, height: u32, data: Vec<u8> },
    /// A GPU surface from ScreenCaptureKit (BGRA), encoded without copying.
    #[cfg(target_os = "macos")]
    Surface(apple_cf::iosurface::IOSurface),
}

impl Frame {
    pub fn size(&self) -> (u32, u32) {
        match self {
            Frame::Bgra { width, height, .. } => (*width, *height),
            #[cfg(target_os = "macos")]
            Frame::Surface(s) => (s.width() as u32, s.height() as u32),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Encoded {
    pub data: Vec<u8>,
    pub key: bool,
}

pub trait VideoEncoder: Send {
    fn encode(&mut self, f: &Frame, force_key: bool) -> Result<Option<Encoded>, String>;
    fn set_bitrate(&mut self, kbps: u32);
    fn codec(&self) -> Codec;
    fn hardware(&self) -> bool;
}

/// Codecs this machine can encode, best first.
pub fn available() -> Vec<Codec> {
    #[cfg(target_os = "macos")]
    {
        vec![Codec::Hevc, Codec::H264]
    }
    #[cfg(windows)]
    {
        // HEVC only from a GPU encoder that works; H.264 always (software when there is no GPU one).
        let mut v = Vec::new();
        if !mf::is_broken(Codec::Hevc) && mf_has(Codec::Hevc) {
            v.push(Codec::Hevc);
        }
        v.push(Codec::H264);
        v
    }
    #[cfg(not(any(target_os = "macos", windows)))]
    {
        vec![Codec::H264]
    }
}

/// Whether a hardware encoder for `codec` came up once (checked a single time per run).
#[cfg(windows)]
fn mf_has(codec: Codec) -> bool {
    use std::sync::OnceLock;
    static HEVC: OnceLock<bool> = OnceLock::new();
    let cell = match codec {
        Codec::Hevc => &HEVC,
        _ => return true,
    };
    *cell.get_or_init(|| mf::MfEncoder::new(codec, 320, 240, 30, 1000).is_ok())
}

/// One encoder as found on this machine (Settings self-check).
#[derive(Debug, Clone, serde::Serialize)]
pub struct EncoderInfo {
    pub codec: &'static str,
    pub hardware: bool,
    pub works: bool,
}

/// Try to open each codec at a small size and report what actually came up.
pub fn probe() -> Vec<EncoderInfo> {
    available()
        .into_iter()
        .map(|c| match open(c, 320, 240, 30, 1000) {
            Ok(e) => EncoderInfo { codec: c.name(), hardware: e.hardware(), works: true },
            Err(_) => EncoderInfo { codec: c.name(), hardware: false, works: false },
        })
        .collect()
}

/// Open the best encoder for `codec`: hardware first, software H.264 when that fails.
pub fn open(codec: Codec, width: u32, height: u32, fps: u32, kbps: u32) -> Result<Box<dyn VideoEncoder>, String> {
    #[cfg(target_os = "macos")]
    match vt::VtEncoder::new(codec, width, height, fps, kbps) {
        Ok(e) => return Ok(Box::new(e)),
        Err(e) => tracing::warn!("hardware {} encoder unavailable: {e}", codec.name()),
    }
    #[cfg(windows)]
    match mf::MfEncoder::new(codec, width, height, fps, kbps) {
        Ok(e) => return Ok(Box::new(e)),
        Err(e) => tracing::warn!("hardware {} encoder unavailable: {e}", codec.name()),
    }
    if codec != Codec::H264 {
        return Err(format!("no {} encoder", codec.name()));
    }
    Ok(Box::new(soft::SoftEncoder::new(width, height, fps, kbps)?))
}

/// The software encoder, for tests and as a last resort.
pub fn open_software(width: u32, height: u32, fps: u32, kbps: u32) -> Result<Box<dyn VideoEncoder>, String> {
    Ok(Box::new(soft::SoftEncoder::new(width, height, fps, kbps)?))
}

/// Convert length-prefixed NAL units (AVCC/HVCC) to Annex B start codes.
pub(crate) fn avcc_to_annexb(src: &[u8], len_size: usize, out: &mut Vec<u8>) {
    let mut i = 0;
    while i + len_size <= src.len() {
        let mut n = 0usize;
        for b in &src[i..i + len_size] {
            n = (n << 8) | *b as usize;
        }
        i += len_size;
        if n == 0 || i + n > src.len() {
            break;
        }
        out.extend_from_slice(&[0, 0, 0, 1]);
        out.extend_from_slice(&src[i..i + n]);
        i += n;
    }
}

/// NAL unit types in an Annex B stream.
pub fn nal_types(codec: Codec, annexb: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut i = 0;
    while i + 3 < annexb.len() {
        if annexb[i] == 0 && annexb[i + 1] == 0 && annexb[i + 2] == 1 {
            let h = annexb[i + 3];
            out.push(match codec {
                Codec::Hevc => (h >> 1) & 0x3f,
                _ => h & 0x1f,
            });
            i += 3;
        } else {
            i += 1;
        }
    }
    out
}

/// A copy of a frame as tightly packed BGRA (software paths only).
pub(crate) fn to_bgra(f: &Frame) -> Result<(u32, u32, std::borrow::Cow<'_, [u8]>), String> {
    match f {
        Frame::Bgra { width, height, data } => Ok((*width, *height, std::borrow::Cow::Borrowed(data))),
        #[cfg(target_os = "macos")]
        Frame::Surface(s) => {
            let g = s.lock_read_only().map_err(|e| format!("surface lock {e}"))?;
            let (w, h, stride) = (g.width(), g.height(), g.bytes_per_row());
            let base = g.base_address();
            let mut out = Vec::with_capacity(w * h * 4);
            for y in 0..h {
                // SAFETY: the surface is locked; each row has `stride` ≥ w*4 readable bytes.
                let row = unsafe { std::slice::from_raw_parts(base.add(y * stride), w * 4) };
                out.extend_from_slice(row);
            }
            Ok((w as u32, h as u32, std::borrow::Cow::Owned(out)))
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn probe_reports_every_codec_and_h264_always_works() {
        let p = super::probe();
        assert_eq!(p.len(), super::available().len());
        let h = p.iter().find(|e| e.codec == "h264").expect("h264 listed");
        assert!(h.works, "software H.264 is always there");
        #[cfg(target_os = "macos")]
        assert!(h.hardware, "VideoToolbox on a Mac");
    }

    use super::*;

    pub fn pattern(w: u32, h: u32, t: u8) -> Frame {
        let mut data = Vec::with_capacity((w * h * 4) as usize);
        for y in 0..h {
            for x in 0..w {
                data.extend_from_slice(&[(x as u8).wrapping_add(t), (y as u8).wrapping_mul(2), t.wrapping_mul(3), 255]);
            }
        }
        Frame::Bgra { width: w, height: h, data }
    }

    fn decodes_h264(stream: &[Encoded], w: usize, h: usize) {
        let mut d = openh264::decoder::Decoder::new().unwrap();
        let mut got = 0;
        for e in stream {
            if let Ok(Some(y)) = d.decode(&e.data) {
                use openh264::formats::YUVSource;
                assert_eq!(y.dimensions(), (w, h));
                got += 1;
            }
        }
        assert!(got > 0, "nothing decoded");
    }

    #[test]
    fn negotiate_prefers_best_common() {
        let all = [Codec::Hevc, Codec::H264];
        assert_eq!(negotiate(&all, &[&[Codec::H264, Codec::Hevc]]), Codec::Hevc);
        assert_eq!(negotiate(&all, &[&[Codec::H264, Codec::Hevc], &[Codec::H264]]), Codec::H264);
        assert_eq!(negotiate(&all, &[]), Codec::Hevc);
        assert_eq!(negotiate(&[Codec::H264], &[&[Codec::Av1]]), Codec::H264);
    }

    #[test]
    fn annexb_conversion() {
        let mut out = Vec::new();
        avcc_to_annexb(&[0, 0, 0, 2, 0x65, 9, 0, 0, 0, 1, 0x41], 4, &mut out);
        assert_eq!(out, vec![0, 0, 0, 1, 0x65, 9, 0, 0, 0, 1, 0x41]);
        assert_eq!(nal_types(Codec::H264, &out), vec![5, 1]);
    }

    #[test]
    fn soft_h264_roundtrip_decodes() {
        let mut e = open_software(320, 240, 30, 1000).unwrap();
        let mut out = Vec::new();
        for t in 0..5 {
            if let Some(x) = e.encode(&pattern(320, 240, t), false).unwrap() {
                out.push(x);
            }
        }
        assert!(out[0].key);
        assert!(nal_types(Codec::H264, &out[0].data).contains(&7), "first frame carries SPS");
        assert!(!out[1].key);
        decodes_h264(&out, 320, 240);
    }

    #[test]
    fn force_key_honored() {
        let mut e = open_software(160, 120, 30, 500).unwrap();
        e.encode(&pattern(160, 120, 0), false).unwrap();
        e.encode(&pattern(160, 120, 1), false).unwrap();
        let k = e.encode(&pattern(160, 120, 2), true).unwrap().unwrap();
        assert!(k.key);
        assert!(nal_types(Codec::H264, &k.data).contains(&7));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn vt_h264_produces_keyframe_with_sps() {
        let mut e = open(Codec::H264, 320, 240, 30, 1000).unwrap();
        assert!(e.hardware());
        let mut out = Vec::new();
        for t in 0..5 {
            if let Some(x) = e.encode(&pattern(320, 240, t), t == 3).unwrap() {
                out.push(x);
            }
        }
        assert!(out[0].key);
        let types = nal_types(Codec::H264, &out[0].data);
        assert!(types.contains(&7) && types.contains(&8) && types.contains(&5), "{types:?}");
        assert!(out.iter().skip(1).any(|f| f.key), "forced key frame");
        decodes_h264(&out, 320, 240);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn vt_hevc_keyframe_has_vps() {
        let mut e = open(Codec::Hevc, 320, 240, 30, 1000).unwrap();
        assert_eq!(e.codec(), Codec::Hevc);
        let f = e.encode(&pattern(320, 240, 0), false).unwrap().unwrap();
        assert!(f.key);
        let types = nal_types(Codec::Hevc, &f.data);
        assert!(types.contains(&32) && types.contains(&33) && types.contains(&34), "{types:?}");
        e.set_bitrate(500);
        assert!(e.encode(&pattern(320, 240, 1), false).unwrap().is_some());
    }
}
