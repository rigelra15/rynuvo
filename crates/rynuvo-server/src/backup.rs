//! Daily SQLite snapshots in `<data_dir>/backups`, newest `keep` retained. Media is not copied
//! (same disk); `scripts/pull-backup.sh` pulls both to the owner's Mac.
use std::{
    path::{Path, PathBuf},
    time::Duration,
};

use sqlx::SqlitePool;

/// "YYYY-MM-DD" of a Unix time in milliseconds (UTC).
pub fn date_of(ms: i64) -> String {
    let (y, m, d) = crate::civil(ms);
    format!("{y:04}-{m:02}-{d:02}")
}

/// A consistent copy of the live database for `date`, unless that day already has one.
pub async fn snapshot(db: &SqlitePool, dir: &Path, date: &str) -> anyhow::Result<Option<PathBuf>> {
    std::fs::create_dir_all(dir)?;
    let path = dir.join(format!("rynuvo-{date}.db"));
    if path.exists() {
        return Ok(None);
    }
    let tmp = dir.join(format!("rynuvo-{date}.db.tmp"));
    let _ = std::fs::remove_file(&tmp);
    sqlx::query("VACUUM INTO ?").bind(tmp.to_string_lossy().into_owned()).execute(db).await?;
    std::fs::rename(&tmp, &path)?;
    Ok(Some(path))
}

/// Delete all but the newest `keep` snapshots; other files are left alone.
pub fn prune(dir: &Path, keep: usize) -> std::io::Result<Vec<PathBuf>> {
    let mut snaps: Vec<PathBuf> = std::fs::read_dir(dir)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with("rynuvo-") && n.ends_with(".db")))
        .collect();
    snaps.sort(); // dates in the name sort chronologically
    let n = snaps.len().saturating_sub(keep);
    let old: Vec<PathBuf> = snaps.into_iter().take(n).collect();
    for p in &old {
        std::fs::remove_file(p)?;
    }
    Ok(old)
}

/// Snapshot once a day (checked hourly, so a restart never skips a day).
pub fn spawn(db: SqlitePool, dir: PathBuf, keep: usize) {
    if keep == 0 {
        return;
    }
    tokio::spawn(async move {
        loop {
            match snapshot(&db, &dir, &date_of(crate::now_ms())).await {
                Ok(Some(p)) => {
                    tracing::info!("backup written: {}", p.display());
                    if let Err(e) = prune(&dir, keep) {
                        tracing::warn!("backup prune failed: {e}");
                    }
                }
                Ok(None) => {}
                Err(e) => tracing::warn!("backup failed: {e}"),
            }
            tokio::time::sleep(Duration::from_secs(3600)).await;
        }
    });
}
