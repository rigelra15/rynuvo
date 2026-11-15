//! Listen together: YouTube Music search and streams, the Opus/WebM demuxer, lyrics, and the
//! controller that keeps this app's playback in time with the server's music state.
pub mod webm;
pub mod ytm;
pub mod ctl;
pub mod lyrics;

/// Wall-clock ms (the server's clock is aligned to this through each state's `server_time`).
pub fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as i64)
}
