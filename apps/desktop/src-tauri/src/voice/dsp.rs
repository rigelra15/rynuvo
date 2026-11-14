//! Microphone cleanup: echo cancellation, noise suppression, gain control (sonora), then RNNoise.
use nnnoiseless::DenoiseState;
use sonora::{
    AudioProcessing, Config, StreamConfig,
    config::{EchoCanceller, GainController2, HighPassFilter, NoiseSuppression},
};

/// Samples in one 10 ms DSP frame at 48 kHz.
pub const DSP_FRAME: usize = 480;

pub struct Processor {
    apm: AudioProcessing,
    rnn: Box<DenoiseState<'static>>,
    scratch: [f32; DSP_FRAME],
}

impl Default for Processor {
    fn default() -> Self {
        Self::new()
    }
}

impl Processor {
    pub fn new() -> Self {
        let config = Config {
            echo_canceller: Some(EchoCanceller::default()),
            noise_suppression: Some(NoiseSuppression::default()),
            gain_controller2: Some(GainController2 { adaptive_digital: Some(Default::default()), ..Default::default() }),
            high_pass_filter: Some(HighPassFilter::default()),
            ..Default::default()
        };
        let apm = AudioProcessing::builder()
            .config(config)
            .capture_config(StreamConfig::new(48_000, 1))
            .render_config(StreamConfig::new(48_000, 1))
            .build();
        Processor { apm, rnn: DenoiseState::new(), scratch: [0.0; DSP_FRAME] }
    }

    /// Clean one captured 10 ms frame in place; returns RNNoise's voice probability (0–1).
    pub fn capture(&mut self, frame: &mut [f32; DSP_FRAME]) -> f32 {
        self.scratch.copy_from_slice(frame);
        if let Err(e) = self.apm.process_capture_f32(&[&self.scratch], &mut [&mut frame[..]]) {
            tracing::debug!("apm capture: {e:?}");
        }
        // RNNoise works on i16-scaled samples.
        for (s, x) in self.scratch.iter_mut().zip(frame.iter()) {
            *s = x * 32768.0;
        }
        let vad = self.rnn.process_frame(&mut frame[..], &self.scratch);
        for x in frame.iter_mut() {
            *x = if x.is_finite() { (*x / 32768.0).clamp(-1.0, 1.0) } else { 0.0 };
        }
        vad
    }

    /// Feed one 10 ms frame of what the speakers are playing (the echo reference).
    pub fn render(&mut self, frame: &[f32; DSP_FRAME]) {
        let mut out = [0f32; DSP_FRAME];
        if let Err(e) = self.apm.process_render_f32(&[frame], &mut [&mut out[..]]) {
            tracing::debug!("apm render: {e:?}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn noise(seed: &mut u32) -> f32 {
        *seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (*seed >> 8) as f32 / (1u32 << 24) as f32 * 2.0 - 1.0
    }

    #[test]
    fn processor_output_finite_and_bounded() {
        let mut p = Processor::new();
        let mut seed = 7u32;
        for f in 0..200 {
            let mut frame: [f32; DSP_FRAME] = std::array::from_fn(|i| {
                if f % 2 == 0 { noise(&mut seed) } else { (2.0 * std::f32::consts::PI * 440.0 * (f * DSP_FRAME + i) as f32 / 48_000.0).sin() }
            });
            p.render(&[0.2; DSP_FRAME]);
            let vad = p.capture(&mut frame);
            assert!((0.0..=1.0).contains(&vad), "vad {vad}");
            assert!(frame.iter().all(|x| x.is_finite() && x.abs() <= 1.0));
        }
    }

    #[test]
    fn silence_has_low_vad() {
        let mut p = Processor::new();
        let mut max = 0f32;
        for _ in 0..100 {
            let mut frame = [0f32; DSP_FRAME];
            max = max.max(p.capture(&mut frame));
        }
        assert!(max < 0.2, "silence vad {max}");
    }
}
