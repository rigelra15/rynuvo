//! How much output to keep queued ahead of the sound card. Starts small for low voice latency and
//! grows when the voice thread was late and the card ran dry (heard as music breaking up), then
//! eases back once things have been calm for a while.
use super::FRAME;

/// Starting and lowest amount (40 ms).
pub const MIN: usize = 2 * FRAME;
/// Most it grows to (160 ms); the ring holds 200 ms.
pub const MAX: usize = 8 * FRAME;
/// Queued output below this when refilling counts as having run dry.
const DRY: usize = FRAME / 4;
/// Calm refills (10 ms each) before stepping back down one frame.
const CALM: u32 = 1_000;

pub struct Ahead {
    target: usize,
    calm: u32,
}

impl Default for Ahead {
    fn default() -> Self {
        Ahead { target: MIN, calm: 0 }
    }
}

impl Ahead {
    /// Called once per refill with how much was still queued; returns how much to queue up to.
    pub fn update(&mut self, queued: usize) -> usize {
        if queued < DRY {
            self.target = (self.target + FRAME).min(MAX);
            self.calm = 0;
        } else {
            self.calm += 1;
            if self.calm >= CALM {
                self.calm = 0;
                self.target = self.target.saturating_sub(FRAME).max(MIN);
            }
        }
        self.target
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_small() {
        assert_eq!(Ahead::default().update(MIN), MIN);
    }

    #[test]
    fn running_dry_queues_more_up_to_a_limit() {
        let mut a = Ahead::default();
        assert_eq!(a.update(0), MIN + FRAME);
        assert_eq!(a.update(DRY - 1), MIN + 2 * FRAME);
        for _ in 0..20 {
            a.update(0);
        }
        assert_eq!(a.update(0), MAX);
    }

    #[test]
    fn calm_eases_back_down_but_not_below_the_start() {
        let mut a = Ahead::default();
        a.update(0);
        a.update(0);
        for _ in 0..CALM - 1 {
            assert_eq!(a.update(MIN), MIN + 2 * FRAME);
        }
        assert_eq!(a.update(MIN), MIN + FRAME, "one step down after a calm stretch");
        a.update(0);
        assert_eq!(a.update(MIN), MIN + 2 * FRAME, "a new dry spell resets the calm count");
        for _ in 0..CALM * 10 {
            a.update(MIN);
        }
        assert_eq!(a.update(MIN), MIN);
    }
}
