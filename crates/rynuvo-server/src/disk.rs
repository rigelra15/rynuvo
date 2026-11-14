use std::sync::{
    Arc,
    atomic::{AtomicU8, Ordering},
};

/// Shared view of the data disk usage (percent used).
#[derive(Clone, Default)]
pub struct DiskState {
    used_pct: Arc<AtomicU8>,
    block_pct: Arc<AtomicU8>,
}

impl DiskState {
    pub fn new(block_pct: u8) -> Self {
        let s = Self::default();
        s.block_pct.store(block_pct, Ordering::Relaxed);
        s
    }
    pub fn used_pct(&self) -> u8 {
        self.used_pct.load(Ordering::Relaxed)
    }
    pub fn blocked(&self) -> bool {
        let b = self.block_pct.load(Ordering::Relaxed);
        b > 0 && self.used_pct() >= b
    }
}

/// Record a usage reading (used by the watcher and by tests).
pub fn check_now(state: &DiskState, used_pct: u8) {
    state.used_pct.store(used_pct, Ordering::Relaxed);
}
