//! Daily rolling log files (3 kept) and a tail reader for "Send log to admin".
//! Nothing here logs request headers or bodies; HTTP logging records method and path only.
use std::{
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
    sync::OnceLock,
};

use tracing_appender::rolling::{Builder, Rotation};

static DIR: OnceLock<PathBuf> = OnceLock::new();
const PREFIX: &str = "rynuvo";
const MAX_TAIL: usize = 1024 * 1024;

pub fn init(dir: &Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(dir)?;
    let appender = Builder::new()
        .rotation(Rotation::DAILY)
        .filename_prefix(PREFIX)
        .filename_suffix("log")
        .max_log_files(3)
        .build(dir)?;
    tracing_subscriber::fmt()
        .with_writer(appender)
        .with_ansi(false)
        .with_max_level(tracing::Level::INFO)
        .try_init()
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    let _ = DIR.set(dir.to_path_buf());
    tracing::info!(version = env!("CARGO_PKG_VERSION"), os = std::env::consts::OS, "rynuvo started");
    Ok(())
}

fn newest(dir: &Path) -> Option<PathBuf> {
    std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().starts_with(PREFIX))
        .max_by_key(|e| e.metadata().and_then(|m| m.modified()).ok())
        .map(|e| e.path())
}

/// Last `max_bytes` (capped at 1 MB) of the newest log file; empty if logging isn't set up.
pub fn collect_tail(max_bytes: usize) -> String {
    let Some(path) = DIR.get().and_then(|d| newest(d)) else { return String::new() };
    let Ok(mut f) = std::fs::File::open(path) else { return String::new() };
    let len = f.metadata().map(|m| m.len()).unwrap_or(0);
    let take = max_bytes.min(MAX_TAIL) as u64;
    let _ = f.seek(SeekFrom::Start(len.saturating_sub(take)));
    let mut buf = Vec::new();
    let _ = f.read_to_end(&mut buf);
    String::from_utf8_lossy(&buf).into_owned()
}

/// Export only aggregate stream performance lines: never share raw logs containing IPs or identifiers.
pub fn export_stream_diagnostics(downloads: &Path) -> anyhow::Result<PathBuf> {
    let entries: Vec<_> = collect_tail(MAX_TAIL)
        .lines()
        .filter(|line| line.contains("rynuvo-stream-diag"))
        .map(str::to_owned)
        .collect();
    if entries.is_empty() {
        anyhow::bail!("No stream diagnostics yet. Start watching or sharing a screen for a few seconds, then retry.");
    }
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    let path = downloads.join(format!("rynuvo-stream-diagnostics-{stamp}.log"));
    let header = format!("Rynuvo stream diagnostics\nVersion: {}\nOS: {}\nOnly aggregate stream metrics are included.\n\n",
        env!("CARGO_PKG_VERSION"), std::env::consts::OS);
    std::fs::write(&path, format!("{header}{}\n", entries.join("\n")))?;
    Ok(path)
}
