pub mod app;
pub mod auth;
pub mod backup;
pub mod config;
pub mod db;
pub mod disk;
pub mod error;
pub mod music;
pub mod push;
pub mod quota;
pub mod routes;
pub mod stun;
pub mod voice;
pub mod ws;

#[cfg(feature = "test-util")]
pub mod testutil;

/// Current unix time in milliseconds.
pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// (year, month, day) of a Unix time in milliseconds, UTC (Howard Hinnant's civil-from-days).
pub fn civil(ms: i64) -> (i64, u32, u32) {
    let z = ms.div_euclid(86_400_000) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
    (y, m, d)
}
