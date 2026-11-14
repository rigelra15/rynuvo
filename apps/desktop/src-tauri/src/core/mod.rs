//! Tauri-independent client core: identity storage, HTTP API, realtime session.
pub mod http;
pub mod identity;
pub mod image;
pub mod logs;
pub mod nettest;
pub mod session;

pub fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as i64)
}
