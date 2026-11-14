//! OpenH264 software encoder (fallback; CPU heavy at high resolutions).
use openh264::{
    OpenH264API,
    encoder::{BitRate, Encoder, EncoderConfig, FrameRate, FrameType, IntraFramePeriod, RateControlMode, UsageType},
    formats::{BgraSliceU8, YUVBuffer},
};

use super::{Codec, Encoded, Frame, VideoEncoder, to_bgra};

pub struct SoftEncoder {
    enc: Encoder,
    width: u32,
    height: u32,
    fps: u32,
    kbps: u32,
    /// Bitrate the encoder was built with; rebuilt when the target drifts far from it.
    built_kbps: u32,
    yuv: YUVBuffer,
}

fn build(fps: u32, kbps: u32) -> Result<Encoder, String> {
    let cfg = EncoderConfig::new()
        .bitrate(BitRate::from_bps(kbps * 1000))
        .max_frame_rate(FrameRate::from_hz(fps as f32))
        .usage_type(UsageType::ScreenContentRealTime)
        .rate_control_mode(RateControlMode::Bitrate)
        .skip_frames(true)
        .intra_frame_period(IntraFramePeriod::from_num_frames(fps * 4));
    Encoder::with_api_config(OpenH264API::from_source(), cfg).map_err(|e| format!("openh264: {e}"))
}

impl SoftEncoder {
    pub fn new(width: u32, height: u32, fps: u32, kbps: u32) -> Result<Self, String> {
        if width % 2 != 0 || height % 2 != 0 || width == 0 || height == 0 {
            return Err("frame size must be even".into());
        }
        Ok(SoftEncoder {
            enc: build(fps, kbps)?,
            width,
            height,
            fps,
            kbps,
            built_kbps: kbps,
            yuv: YUVBuffer::new(width as usize, height as usize),
        })
    }
}

impl VideoEncoder for SoftEncoder {
    fn encode(&mut self, f: &Frame, force_key: bool) -> Result<Option<Encoded>, String> {
        let (w, h, bgra) = to_bgra(f)?;
        if (w, h) != (self.width, self.height) {
            return Err(format!("frame is {w}×{h}, encoder is {}×{}", self.width, self.height));
        }
        let drift = self.kbps.abs_diff(self.built_kbps) as f32 / self.built_kbps as f32;
        if drift > 0.25 {
            self.enc = build(self.fps, self.kbps)?;
            self.built_kbps = self.kbps;
        }
        self.yuv.read_bgra8(BgraSliceU8::new(&bgra, (w as usize, h as usize)));
        if force_key {
            self.enc.force_intra_frame();
        }
        let bs = self.enc.encode(&self.yuv).map_err(|e| format!("openh264 encode: {e}"))?;
        let key = matches!(bs.frame_type(), FrameType::IDR | FrameType::I);
        if matches!(bs.frame_type(), FrameType::Skip | FrameType::Invalid) {
            return Ok(None);
        }
        Ok(Some(Encoded { data: bs.to_vec(), key }))
    }

    fn set_bitrate(&mut self, kbps: u32) {
        self.kbps = kbps;
    }

    fn codec(&self) -> Codec {
        Codec::H264
    }

    fn hardware(&self) -> bool {
        false
    }
}
