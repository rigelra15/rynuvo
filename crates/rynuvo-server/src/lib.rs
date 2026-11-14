pub mod app;
pub mod auth;
pub mod config;
pub mod db;
pub mod disk;
pub mod error;
pub mod routes;
pub mod stun;
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
