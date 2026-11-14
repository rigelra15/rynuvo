//! The boundary between the voice engine and the sound card, so tests can swap in a fake.
use ringbuf::{HeapCons, HeapProd};
use std::sync::mpsc;

/// Audio plumbing handed to a backend: it pushes the mic (48 kHz mono f32) into `capture`,
/// pulls what to play (48 kHz mono f32) from `playback`, and reports fatal stream errors on `errors`.
pub struct AudioIo {
    pub capture: HeapProd<f32>,
    pub playback: HeapCons<f32>,
    pub errors: mpsc::Sender<String>,
}

pub trait AudioBackend: Send {
    /// Open the named devices (`None` = system default) and start streaming.
    fn start(&mut self, input: Option<&str>, output: Option<&str>, io: AudioIo) -> Result<(), String>;
    fn stop(&mut self);
}

/// Streaming linear-interpolation resampler for one channel.
pub struct Resampler {
    step: f64,
    pos: f64,
    prev: f32,
}

impl Resampler {
    pub fn new(from_rate: u32, to_rate: u32) -> Self {
        Resampler { step: from_rate as f64 / to_rate as f64, pos: 0.0, prev: 0.0 }
    }

    pub fn is_identity(&self) -> bool {
        self.step == 1.0
    }

    pub fn process(&mut self, input: &[f32], out: &mut Vec<f32>) {
        for &x in input {
            while self.pos < 1.0 {
                out.push(self.prev + (x - self.prev) * self.pos as f32);
                self.pos += self.step;
            }
            self.pos -= 1.0;
            self.prev = x;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resampler_keeps_duration() {
        let mut r = Resampler::new(44_100, 48_000);
        let mut out = Vec::new();
        for _ in 0..10 {
            r.process(&vec![0.5; 4_410], &mut out);
        }
        assert!((out.len() as i64 - 48_000).abs() <= 2, "{}", out.len());
        assert!(out[10..].iter().all(|x| (x - 0.5).abs() < 1e-6));

        let mut r = Resampler::new(48_000, 44_100);
        let mut out = Vec::new();
        r.process(&vec![0.1; 48_000], &mut out);
        assert!((out.len() as i64 - 44_100).abs() <= 2, "{}", out.len());
    }
}
