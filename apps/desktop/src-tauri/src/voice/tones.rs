//! The app's little sounds (join/leave, mute/deafen, someone going live), generated so no audio
//! files ship with the app. All are soft bell notes: calm, but clear about what happened. Rising
//! means something switched on, falling means off.
use std::f32::consts::PI;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Join,
    Leave,
    Mute,
    Unmute,
    Deafen,
    Undeafen,
    /// Someone in the call started sharing their screen.
    Live,
}

const RATE: f32 = 48_000.0;

const A3: f32 = 220.0;
const D4: f32 = 293.66;
const A4: f32 = 440.0;
const D5: f32 = 587.33;
const A5: f32 = 880.0;
const CS6: f32 = 1108.73;
const D6: f32 = 1174.66;
const E6: f32 = 1318.51;

/// (start ms, Hz) per note, the whole length in ms, how fast each note rings out (s), and loudness.
fn shape(t: Tone) -> (&'static [(f32, f32)], f32, f32, f32) {
    match t {
        // Three notes a fifth and an octave apart, like a little arpeggio.
        Tone::Join => (&[(0.0, D5), (95.0, A5), (190.0, D6)], 560.0, 0.14, 0.2),
        Tone::Leave => (&[(0.0, D6), (95.0, A5), (190.0, D5)], 560.0, 0.14, 0.2),
        // Two quick notes a fourth apart.
        Tone::Mute => (&[(0.0, D5), (70.0, A4)], 260.0, 0.07, 0.17),
        Tone::Unmute => (&[(0.0, A4), (70.0, D5)], 260.0, 0.07, 0.17),
        // An octave lower and a little longer: a muffled "door closing".
        Tone::Deafen => (&[(0.0, D4), (85.0, A3)], 320.0, 0.09, 0.2),
        Tone::Undeafen => (&[(0.0, A3), (85.0, D4)], 320.0, 0.09, 0.2),
        // A bright major chord rolled upwards.
        Tone::Live => (&[(0.0, A5), (60.0, CS6), (120.0, E6)], 480.0, 0.12, 0.15),
    }
}

/// 48 kHz mono, a quarter to half a second, starting and ending in silence.
pub fn samples(t: Tone) -> Vec<f32> {
    let (notes, len_ms, ring, level) = shape(t);
    let n = (len_ms / 1000.0 * RATE) as usize;
    let mut out = vec![0.0f32; n];
    for &(start_ms, f) in notes {
        let start = (start_ms / 1000.0 * RATE) as usize;
        for (i, o) in out.iter_mut().enumerate().skip(start) {
            let t = (i - start) as f32 / RATE;
            // 5 ms attack, then a bell's exponential ring-out.
            let env = (t / 0.005).min(1.0) * (-t / ring).exp();
            let ph = 2.0 * PI * f * t;
            // A touch of the octave and twelfth for a rounder, glassy tone.
            *o += env * (ph.sin() + 0.22 * (2.0 * ph).sin() + 0.06 * (3.0 * ph).sin());
        }
    }
    let peak = out.iter().fold(0.0f32, |m, x| m.max(x.abs())).max(1e-6);
    let fade = (0.04 * RATE) as usize;
    for (i, o) in out.iter_mut().enumerate() {
        let tail = ((n - i) as f32 / fade as f32).min(1.0);
        *o = *o / peak * level * tail;
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

    const ALL: [Tone; 7] = [Tone::Join, Tone::Leave, Tone::Mute, Tone::Unmute, Tone::Deafen, Tone::Undeafen, Tone::Live];

    #[test]
    fn chimes_are_short_soft_and_click_free() {
        for t in ALL {
            let s = samples(t);
            assert!((9_600..=33_600).contains(&s.len()), "{t:?}: 0.2–0.7 s, got {} samples", s.len());
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

    #[test]
    fn switching_off_falls_and_switching_on_rises() {
        // Pitch of the first and the last note of a short two-note blip.
        let first = |s: &[f32]| pitch(&s[240..2_400]);
        let last = |s: &[f32]| pitch(&s[s.len() / 2..s.len() * 3 / 4]);
        for (off, on) in [(Tone::Mute, Tone::Unmute), (Tone::Deafen, Tone::Undeafen)] {
            let (a, b) = (samples(off), samples(on));
            assert!(last(&a) < first(&a), "{off:?} falls: {} → {}", first(&a), last(&a));
            assert!(last(&b) > first(&b), "{on:?} rises: {} → {}", first(&b), last(&b));
        }
        // Mute and deafen don't sound alike: deafen sits lower.
        assert!(pitch(&samples(Tone::Deafen)) < pitch(&samples(Tone::Mute)) * 0.8);
    }

    #[test]
    fn going_live_is_its_own_brighter_sound() {
        let live = pitch(&samples(Tone::Live));
        assert!(live > pitch(&samples(Tone::Join)), "live sits above the join chime");
    }
}
