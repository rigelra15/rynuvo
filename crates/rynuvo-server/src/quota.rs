//! Monthly data budget for the relay (voice + screen share through the server), counted as bytes
//! in + bytes out. At 80% the admins are warned; at 100% screen share through the server stops
//! until the next month (direct connections and voice keep working).
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicU8, AtomicU64, Ordering},
    },
    time::Duration,
};

use rynuvo_proto::WsServerMsg;
use sqlx::SqlitePool;

use crate::ws::Hub;

const GB: f64 = 1_000_000_000.0;
pub const WARN_PCT: u64 = 80;

#[derive(Clone, Default)]
pub struct Quota {
    limit: Arc<AtomicU64>,
    /// Bytes this period, including what is not flushed yet.
    used: Arc<AtomicU64>,
    /// Bytes added since the last flush.
    pending: Arc<AtomicU64>,
    period: Arc<Mutex<String>>,
}

impl Quota {
    /// `limit` in bytes; 0 = unlimited.
    pub fn new(limit: u64) -> Quota {
        let q = Quota::default();
        q.limit.store(limit, Ordering::Relaxed);
        q
    }

    pub fn set_limit(&self, limit: u64) {
        self.limit.store(limit, Ordering::Relaxed);
    }

    pub fn add(&self, n: u64) {
        self.used.fetch_add(n, Ordering::Relaxed);
        self.pending.fetch_add(n, Ordering::Relaxed);
    }

    pub fn used(&self) -> u64 {
        self.used.load(Ordering::Relaxed)
    }

    pub fn limit(&self) -> u64 {
        self.limit.load(Ordering::Relaxed)
    }

    pub fn period(&self) -> String {
        self.period.lock().unwrap().clone()
    }

    /// Switch to `period` whose stored total is `stored` (startup, or a new month).
    pub fn set_period(&self, period: &str, stored: u64) {
        *self.period.lock().unwrap() = period.to_string();
        self.pending.store(0, Ordering::Relaxed);
        self.used.store(stored, Ordering::Relaxed);
    }

    fn pct(&self) -> u64 {
        let l = self.limit();
        if l == 0 { 0 } else { (self.used() as u128 * 100 / l as u128) as u64 }
    }

    pub fn relay_blocked(&self) -> bool {
        self.limit() > 0 && self.used() >= self.limit()
    }

    /// 0 ok, 1 warn, 2 used up.
    pub fn level(&self) -> u8 {
        if self.relay_blocked() {
            2
        } else if self.limit() > 0 && self.pct() >= WARN_PCT {
            1
        } else {
            0
        }
    }

    /// Admin alert text for the current level, if any.
    pub fn alert(&self) -> Option<String> {
        let (used, limit) = (self.used() as f64 / GB, self.limit() as f64 / GB);
        match self.level() {
            2 => Some(format!(
                "This month's data budget is used up ({used:.0} of {limit:.0} GB). Screen share through the server is paused until next month; direct connections and voice still work."
            )),
            1 => Some(format!("{}% of this month's data budget is used ({used:.0} of {limit:.0} GB).", self.pct())),
            _ => None,
        }
    }
}

/// "YYYY-MM" of a Unix time in milliseconds (UTC).
pub fn period_of(ms: i64) -> String {
    let (y, m, _) = crate::civil(ms);
    format!("{y:04}-{m:02}")
}

pub async fn load_used(db: &SqlitePool, period: &str) -> sqlx::Result<u64> {
    let v: Option<(i64,)> = sqlx::query_as("SELECT bytes FROM monthly_usage WHERE period = ?").bind(period).fetch_optional(db).await?;
    Ok(v.map_or(0, |(b,)| b.max(0) as u64))
}

/// Add what was counted since the last flush to this period's row.
pub async fn flush(db: &SqlitePool, q: &Quota) -> sqlx::Result<()> {
    let n = q.pending.swap(0, Ordering::Relaxed);
    if n == 0 {
        return Ok(());
    }
    let r = sqlx::query("INSERT INTO monthly_usage (period, bytes) VALUES (?, ?) ON CONFLICT(period) DO UPDATE SET bytes = bytes + excluded.bytes")
        .bind(q.period())
        .bind(n as i64)
        .execute(db)
        .await;
    if r.is_err() {
        q.pending.fetch_add(n, Ordering::Relaxed); // try again next time
    }
    r.map(|_| ())
}

/// Load this month's total, then flush every minute, roll over at month end and alert admins
/// when the level rises.
pub async fn spawn(db: SqlitePool, q: Quota, hub: Hub) {
    let period = period_of(crate::now_ms());
    match load_used(&db, &period).await {
        Ok(u) => q.set_period(&period, u),
        Err(e) => {
            tracing::warn!("quota: could not load usage: {e}");
            q.set_period(&period, 0);
        }
    }
    let last = Arc::new(AtomicU8::new(q.level()));
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(60)).await;
            if let Err(e) = flush(&db, &q).await {
                tracing::warn!("quota: flush failed: {e}");
            }
            let now = period_of(crate::now_ms());
            if now != q.period() {
                tracing::info!("quota: new period {now}");
                q.set_period(&now, load_used(&db, &now).await.unwrap_or(0));
            }
            let level = q.level();
            if level > last.swap(level, Ordering::Relaxed) {
                if let Some(msg) = q.alert() {
                    tracing::warn!("{msg}");
                    hub.send_to_admins(WsServerMsg::AdminAlert { message: msg });
                }
            }
        }
    });
}
