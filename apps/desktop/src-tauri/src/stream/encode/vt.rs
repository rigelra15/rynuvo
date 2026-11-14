//! VideoToolbox hardware encoder (H.264 / HEVC).
use apple_cf::{
    cf::{CFDictionary, CFNumber, CFString},
    cm::CMTime,
    iosurface::{IOSurface, IOSurfaceLockOptions},
};
use videotoolbox::prelude::{Codec as VtCodec, CompressionSession, FrameProperties};

use super::{Codec, Encoded, Frame, VideoEncoder, avcc_to_annexb};

pub struct VtEncoder {
    session: CompressionSession,
    codec: Codec,
    width: u32,
    height: u32,
    fps: u32,
    frame_no: i64,
    /// Staging surface for BGRA frames that do not come from ScreenCaptureKit.
    staging: Option<IOSurface>,
}

impl VtEncoder {
    pub fn new(codec: Codec, width: u32, height: u32, fps: u32, kbps: u32) -> Result<Self, String> {
        let vt = match codec {
            Codec::H264 => VtCodec::H264,
            Codec::Hevc => VtCodec::HEVC,
            Codec::Av1 => return Err("no AV1 encoder in VideoToolbox".into()),
        };
        let session = CompressionSession::builder(width as i32, height as i32, vt)
            .with_real_time(true)
            .with_allow_frame_reordering(false)
            .with_average_bit_rate((kbps * 1000) as i32)
            .with_expected_frame_rate(fps as f64)
            .with_max_keyframe_interval((fps * 4) as i32)
            .build()
            .map_err(|e| format!("VideoToolbox: {e:?}"))?;
        Ok(VtEncoder { session, codec, width, height, fps, frame_no: 0, staging: None })
    }

    fn stage(&mut self, width: u32, height: u32, data: &[u8]) -> Result<&IOSurface, String> {
        if self.staging.as_ref().is_none_or(|s| s.width() != width as usize || s.height() != height as usize) {
            self.staging = IOSurface::create(width as usize, height as usize, u32::from_be_bytes(*b"BGRA"), 4);
        }
        let s = self.staging.as_ref().ok_or("staging surface")?;
        let mut g = s.lock(IOSurfaceLockOptions::NONE).map_err(|e| format!("surface lock {e}"))?;
        let stride = g.bytes_per_row();
        let base = g.base_address_mut().ok_or("surface not writable")?;
        let row = width as usize * 4;
        for y in 0..height as usize {
            // SAFETY: locked for writing; each destination row has `stride` ≥ row bytes.
            unsafe { std::ptr::copy_nonoverlapping(data[y * row..].as_ptr(), base.add(y * stride), row) };
        }
        drop(g);
        Ok(s)
    }
}

impl VideoEncoder for VtEncoder {
    fn encode(&mut self, f: &Frame, force_key: bool) -> Result<Option<Encoded>, String> {
        let (w, h) = f.size();
        if (w, h) != (self.width, self.height) {
            return Err(format!("frame is {w}×{h}, encoder is {}×{}", self.width, self.height));
        }
        let t = CMTime::new(self.frame_no, self.fps as i32);
        self.frame_no += 1;
        let props = FrameProperties::new().with_force_key_frame(force_key);
        let out = match f {
            Frame::Bgra { width, height, data } => {
                let (width, height) = (*width, *height);
                let data = data.clone();
                let s = self.stage(width, height, &data)?.clone();
                self.session.encode_with_properties(&s, t, props)
            }
            Frame::Surface(s) => self.session.encode_with_properties(s, t, props),
        }
        .map_err(|e| format!("VideoToolbox encode: {e:?}"))?;
        let Some(sb) = out.cm_sample_buffer() else { return Ok(None) };
        let fd = sb.format_description().ok_or("no format description")?;
        let ps = fd.video_parameter_sets().map_err(|e| format!("parameter sets {e}"))?;
        let len_size = ps.nal_unit_header_length.clamp(1, 4) as usize;
        let mut data = Vec::with_capacity(out.data.len() + 128);
        avcc_to_annexb(&out.data, len_size, &mut data);
        let types = super::nal_types(self.codec, &data);
        let key = match self.codec {
            Codec::Hevc => types.iter().any(|t| (16..=21).contains(t)),
            _ => types.contains(&5),
        };
        if key {
            let mut with_ps = Vec::with_capacity(data.len() + 256);
            for p in &ps.parameter_sets {
                with_ps.extend_from_slice(&[0, 0, 0, 1]);
                with_ps.extend_from_slice(p);
            }
            with_ps.extend_from_slice(&data);
            data = with_ps;
        }
        Ok(Some(Encoded { data, key }))
    }

    fn set_bitrate(&mut self, kbps: u32) {
        let d = CFDictionary::from_pairs(&[(&CFString::new("AverageBitRate"), &CFNumber::from_i64(kbps as i64 * 1000))]);
        if let Err(e) = self.session.set_properties(&d) {
            tracing::debug!("VideoToolbox bitrate change failed: {e:?}");
        }
    }

    fn codec(&self) -> Codec {
        self.codec
    }

    fn hardware(&self) -> bool {
        true
    }
}
