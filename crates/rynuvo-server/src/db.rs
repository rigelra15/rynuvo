use sqlx::{
    SqlitePool,
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions},
};
use std::str::FromStr;

/// Open the SQLite pool and run migrations. `url` is `sqlite::memory:` or a file path URL.
pub async fn open_pool(url: &str) -> anyhow::Result<SqlitePool> {
    let memory = url.contains(":memory:");
    let mut opts = SqliteConnectOptions::from_str(url)?
        .create_if_missing(true)
        .foreign_keys(true);
    if !memory {
        opts = opts.journal_mode(SqliteJournalMode::Wal);
    }
    let pool = SqlitePoolOptions::new()
        .max_connections(if memory { 1 } else { 4 })
        .connect_with(opts)
        .await?;
    sqlx::migrate!("./migrations").run(&pool).await?;
    Ok(pool)
}
