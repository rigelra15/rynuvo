use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicU8, Ordering},
    },
    time::Duration,
};

use rynuvo_proto::WsServerMsg;

use crate::ws::Hub;

/// Shared view of the data disk usage (percent used).
#[derive(Clone, Default)]
pub struct DiskState {
    used_pct: Arc<AtomicU8>,
    warn_pct: Arc<AtomicU8>,
    block_pct: Arc<AtomicU8>,
}

impl DiskState {
    pub fn new(warn_pct: u8, block_pct: u8) -> Self {
        let s = Self::default();
        s.warn_pct.store(warn_pct, Ordering::Relaxed);
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
    /// Admin alert text for the current level, if any.
    pub fn alert(&self) -> Option<String> {
        let used = self.used_pct();
        if self.blocked() {
            Some(format!("Disk server {used}% penuh — upload gambar dihentikan sementara."))
        } else if used >= self.warn_pct.load(Ordering::Relaxed) {
            Some(format!("Disk server sudah {used}% terpakai."))
        } else {
            None
        }
    }
}

/// Record a usage reading (used by the watcher and by tests).
pub fn check_now(state: &DiskState, used_pct: u8) {
    state.used_pct.store(used_pct, Ordering::Relaxed);
}

fn read_pct(path: &PathBuf) -> std::io::Result<u8> {
    let total = fs4::total_space(path)?;
    let avail = fs4::available_space(path)?;
    if total == 0 {
        return Ok(0);
    }
    Ok((((total - avail) as f64 / total as f64) * 100.0).round() as u8)
}

/// Poll disk usage every 60s; alert admins when the level rises.
pub fn spawn_watcher(path: PathBuf, state: DiskState, hub: Hub) {
    tokio::spawn(async move {
        let mut last_level = 0u8; // 0 ok, 1 warn, 2 block
        loop {
            match read_pct(&path) {
                Ok(p) => {
                    check_now(&state, p);
                    let level = if state.blocked() { 2 } else if state.alert().is_some() { 1 } else { 0 };
                    if level > last_level {
                        if let Some(msg) = state.alert() {
                            tracing::warn!("{msg}");
                            hub.send_to_admins(WsServerMsg::AdminAlert { message: msg });
                        }
                    }
                    last_level = level;
                }
                Err(e) => tracing::warn!("disk check failed: {e}"),
            }
            tokio::time::sleep(Duration::from_secs(60)).await;
        }
    });
}
