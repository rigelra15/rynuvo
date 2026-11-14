//! Short join/leave chimes, generated so no audio files ship with the app.
use std::f32::consts::PI;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Join,
    Leave,
}

/// Two soft notes: rising for join, falling for leave. 48 kHz mono, under 250 ms.
pub fn samples(t: Tone) -> Vec<f32> {
    let (a, b) = match t {
        Tone::Join => (660.0, 880.0),
        Tone::Leave => (880.0, 587.0),
    };
    let note = |f: f32, n: usize| -> Vec<f32> {
        (0..n)
            .map(|i| {
                let t = i as f32 / 48_000.0;
                let env = (i.min(n - i) as f32 / 240.0).min(1.0) * (1.0 - i as f32 / n as f32 * 0.5);
                0.18 * env * (2.0 * PI * f * t).sin()
            })
            .collect()
    };
    let mut v = note(a, 4_800);
    v.extend(note(b, 6_000));
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tones_are_short() {
        for t in [Tone::Join, Tone::Leave] {
            let s = samples(t);
            assert!(!s.is_empty() && s.len() <= 12_000, "{} samples", s.len());
            assert!(s.iter().all(|x| x.abs() <= 0.5));
        }
    }
}
