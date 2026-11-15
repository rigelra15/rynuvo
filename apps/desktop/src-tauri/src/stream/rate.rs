//! Sender bitrate control from viewer loss reports (AIMD).
pub const MIN_KBPS: u32 = 500;
/// Sending starts here and climbs on clean reports (slow start).
pub const START_KBPS: u32 = 2_500;

pub struct Controller {
    max: u32,
    target: u32,
}

impl Controller {
    pub fn new(max_kbps: u32) -> Self {
        let max = max_kbps.max(MIN_KBPS);
        Controller { max, target: START_KBPS.min(max) }
    }
    pub fn on_report(&mut self, loss: f32) {
        let t = self.target as f32;
        let next = if loss > 0.05 {
            t * 0.85
        } else if loss < 0.01 {
            t * 1.05
        } else {
            t
        };
        self.target = (next as u32).clamp(MIN_KBPS, self.max);
    }
    pub fn set_max(&mut self, max_kbps: u32) {
        self.max = max_kbps.max(MIN_KBPS);
        self.target = self.target.min(self.max);
    }
    pub fn target_kbps(&self) -> u32 {
        self.target
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_low_and_ramps_up_to_the_preset() {
        // Full rate at once floods a slow (mobile) uplink before any loss report can arrive, and
        // starves the streamer's own server connection.
        let mut c = Controller::new(10_000);
        assert_eq!(c.target_kbps(), START_KBPS);
        let mut reports = 0;
        while c.target_kbps() < 10_000 {
            c.on_report(0.0);
            reports += 1;
        }
        assert!(reports <= 40, "reaches the preset within ~20 s of clean reports, took {reports}");
        assert_eq!(Controller::new(1_000).target_kbps(), 1_000, "a preset below the start rate starts at its max");
    }

    #[test]
    fn rate_backs_off_on_loss() {
        let mut c = Controller::new(8000);
        while c.target_kbps() < 8000 {
            c.on_report(0.0);
        }
        c.on_report(0.10);
        assert_eq!(c.target_kbps(), 6800);
        for _ in 0..50 {
            c.on_report(0.5);
        }
        assert_eq!(c.target_kbps(), MIN_KBPS);
    }

    #[test]
    fn rate_recovers_and_clamps() {
        let mut c = Controller::new(4000);
        c.on_report(0.2);
        let low = c.target_kbps();
        c.on_report(0.0);
        assert!(c.target_kbps() > low);
        c.on_report(0.03); // between thresholds: hold
        let held = c.target_kbps();
        c.on_report(0.03);
        assert_eq!(c.target_kbps(), held);
        for _ in 0..100 {
            c.on_report(0.0);
        }
        assert_eq!(c.target_kbps(), 4000);
        c.set_max(2000);
        assert_eq!(c.target_kbps(), 2000);
    }
}
