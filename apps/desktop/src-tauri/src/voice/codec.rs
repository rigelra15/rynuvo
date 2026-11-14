//! Opus encode/decode for 20 ms mono frames at 48 kHz.
use super::FRAME;

/// What the jitter buffer hands the decoder for one 20 ms slot.
#[derive(Debug, Clone, PartialEq)]
pub enum Frame {
    /// The packet for this slot.
    Data(Vec<u8>),
    /// This slot's packet is missing but the next one arrived: recover from its in-band FEC.
    Fec(Vec<u8>),
    /// Nothing to recover from: packet-loss concealment.
    Lost,
}

pub struct Encoder(opus::Encoder);

impl Encoder {
    /// 48 kHz mono VOIP, 64 kbps, in-band FEC tuned for ~10% loss.
    pub fn new() -> Result<Self, String> {
        let e = || -> Result<opus::Encoder, opus::Error> {
            let mut e = opus::Encoder::new(48_000, opus::Channels::Mono, opus::Application::Voip)?;
            e.set_bitrate(opus::Bitrate::Bits(64_000))?;
            e.set_inband_fec(true)?;
            e.set_packet_loss_perc(10)?;
            e.set_signal(opus::Signal::Voice)?;
            Ok(e)
        };
        e().map(Encoder).map_err(|e| format!("Opus encoder: {e}"))
    }

    pub fn encode(&mut self, pcm: &[f32; FRAME]) -> Vec<u8> {
        let mut buf = [0u8; 1275];
        match self.0.encode_float(pcm, &mut buf) {
            Ok(n) => buf[..n].to_vec(),
            Err(e) => {
                tracing::warn!("opus encode failed: {e}");
                Vec::new()
            }
        }
    }
}

pub struct Decoder(opus::Decoder);

impl Decoder {
    pub fn new() -> Result<Self, String> {
        opus::Decoder::new(48_000, opus::Channels::Mono).map(Decoder).map_err(|e| format!("Opus decoder: {e}"))
    }

    pub fn decode(&mut self, frame: Frame) -> [f32; FRAME] {
        let mut out = [0f32; FRAME];
        let r = match &frame {
            Frame::Data(d) => self.0.decode_float(d, &mut out, false),
            Frame::Fec(d) => self.0.decode_float(d, &mut out, true),
            Frame::Lost => self.0.decode_float(&[], &mut out, false),
        };
        if r.is_err() {
            out = [0f32; FRAME];
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(n: usize, start: usize) -> Vec<f32> {
        (start..start + n).map(|i| 0.5 * (2.0 * std::f32::consts::PI * 440.0 * i as f32 / 48_000.0).sin()).collect()
    }

    #[test]
    fn opus_roundtrip_preserves_tone() {
        let mut enc = Encoder::new().unwrap();
        let mut dec = Decoder::new().unwrap();
        let mut out = Vec::new();
        let mut inp = Vec::new();
        for f in 0..25 {
            let pcm: [f32; FRAME] = tone(FRAME, f * FRAME).try_into().unwrap();
            inp.extend_from_slice(&pcm);
            let pkt = enc.encode(&pcm);
            assert!(!pkt.is_empty() && pkt.len() < 400);
            out.extend_from_slice(&dec.decode(Frame::Data(pkt)));
        }
        // Compare the last 10 frames allowing for codec delay: best correlation over small lags.
        let a = &inp[15 * FRAME..24 * FRAME];
        let best = (0..400)
            .map(|lag| {
                let b = &out[15 * FRAME + lag..24 * FRAME + lag];
                let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
                let na: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
                let nb: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
                dot / (na * nb + 1e-9)
            })
            .fold(f32::MIN, f32::max);
        assert!(best > 0.9, "correlation {best}");
    }

    #[test]
    fn lost_frame_decodes_to_plc() {
        let mut enc = Encoder::new().unwrap();
        let mut dec = Decoder::new().unwrap();
        for f in 0..5 {
            let pcm: [f32; FRAME] = tone(FRAME, f * FRAME).try_into().unwrap();
            dec.decode(Frame::Data(enc.encode(&pcm)));
        }
        let plc = dec.decode(Frame::Lost);
        assert_eq!(plc.len(), FRAME);
        assert!(plc.iter().all(|x| x.is_finite()));
        let next: [f32; FRAME] = tone(FRAME, 6 * FRAME).try_into().unwrap();
        let fec = dec.decode(Frame::Fec(enc.encode(&next)));
        assert!(fec.iter().all(|x| x.is_finite()));
    }
}
