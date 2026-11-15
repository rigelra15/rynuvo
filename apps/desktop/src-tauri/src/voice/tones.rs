//! Short join/leave chimes, generated so no audio files ship with the app.
use std::f32::consts::PI;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Join,
    Leave,
}

const RATE: f32 = 48_000.0;
const LEN_MS: f32 = 560.0;

/// Three soft bell notes a fifth and an octave apart, overlapping like a little arpeggio: rising for
/// join, falling for leave. 48 kHz mono, a bit over half a second.
pub fn samples(t: Tone) -> Vec<f32> {
    const D5: f32 = 587.33;
    const A5: f32 = 880.0;
    const D6: f32 = 1174.66;
    let notes: [(f32, f32); 3] = match t {
        Tone::Join => [(0.0, D5), (95.0, A5), (190.0, D6)],
        Tone::Leave => [(0.0, D6), (95.0, A5), (190.0, D5)],
    };
    let n = (LEN_MS / 1000.0 * RATE) as usize;
    let mut out = vec![0.0f32; n];
    for (start_ms, f) in notes {
        let start = (start_ms / 1000.0 * RATE) as usize;
        for (i, o) in out.iter_mut().enumerate().skip(start) {
            let t = (i - start) as f32 / RATE;
            // 5 ms attack, then a bell's exponential ring-out.
            let env = (t / 0.005).min(1.0) * (-t / 0.14).exp();
            let ph = 2.0 * PI * f * t;
            // A touch of the octave and twelfth for a rounder, glassy tone.
            *o += env * (ph.sin() + 0.22 * (2.0 * ph).sin() + 0.06 * (3.0 * ph).sin());
        }
    }
    let peak = out.iter().fold(0.0f32, |m, x| m.max(x.abs())).max(1e-6);
    let fade = (0.04 * RATE) as usize;
    for (i, o) in out.iter_mut().enumerate() {
        let tail = ((n - i) as f32 / fade as f32).min(1.0);
        *o = *o / peak * 0.2 * tail;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Zero crossings per second in `s` (a rough pitch).
    fn pitch(s: &[f32]) -> f32 {
        let z = s.windows(2).filter(|w| (w[0] < 0.0) != (w[1] < 0.0)).count();
        z as f32 / 2.0 / (s.len() as f32 / 48_000.0)
    }

    #[test]
    fn chimes_are_short_soft_and_click_free() {
        for t in [Tone::Join, Tone::Leave] {
            let s = samples(t);
            assert!((19_200..=33_600).contains(&s.len()), "0.4–0.7 s, got {} samples", s.len());
            assert!(s.iter().all(|x| x.abs() <= 0.5));
            assert!(s[0].abs() < 0.01, "starts from silence");
            assert!(s[s.len() - 480..].iter().all(|x| x.abs() < 0.01), "rings out to silence");
        }
    }

    #[test]
    fn join_rises_and_leave_falls() {
        let early = |s: &[f32]| pitch(&s[480..4_800]);
        let late = |s: &[f32]| pitch(&s[9_600..14_400]);
        let j = samples(Tone::Join);
        let l = samples(Tone::Leave);
        assert!(late(&j) > early(&j) * 1.2, "join {} → {}", early(&j), late(&j));
        assert!(late(&l) < early(&l) / 1.2, "leave {} → {}", early(&l), late(&l));
    }
}
