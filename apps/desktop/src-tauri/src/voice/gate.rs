//! Voice-activity gate with hangover, so word endings are not clipped.

/// Frames (20 ms) the gate stays open after speech drops below the threshold.
pub const HANGOVER: u32 = 15;

pub struct VadGate {
    threshold: f32,
    hold: u32,
}

impl VadGate {
    pub fn new(threshold: f32) -> Self {
        VadGate { threshold, hold: 0 }
    }

    pub fn set_threshold(&mut self, t: f32) {
        self.threshold = t.clamp(0.0, 1.0);
    }

    /// Feed one frame's voice probability; true while the gate is open.
    pub fn update(&mut self, prob: f32) -> bool {
        if prob >= self.threshold {
            self.hold = HANGOVER;
            true
        } else if self.hold > 0 {
            self.hold -= 1;
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opens_and_holds_for_hangover() {
        let mut g = VadGate::new(0.5);
        assert!(!g.update(0.1));
        assert!(g.update(0.9));
        for _ in 0..HANGOVER {
            assert!(g.update(0.0), "held open");
        }
        assert!(!g.update(0.0), "closes after hangover");
        g.set_threshold(0.05);
        assert!(g.update(0.1));
    }
}
