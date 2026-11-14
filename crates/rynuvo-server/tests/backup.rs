//! Daily SQLite snapshots: a consistent copy per day, the newest `keep` retained.
use rynuvo_server::{
    backup::{date_of, prune, snapshot},
    db,
};

#[test]
fn date_is_utc_day() {
    assert_eq!(date_of(1_794_614_400_000), "2026-11-14");
    assert_eq!(date_of(1_794_614_399_999), "2026-11-13");
}

#[tokio::test]
async fn snapshot_is_a_readable_copy_and_once_per_day() {
    let tmp = tempfile::tempdir().unwrap();
    let pool = db::open_pool(&format!("sqlite://{}", tmp.path().join("live.db").display())).await.unwrap();
    sqlx::query("INSERT INTO users (nickname, nickname_key, is_admin) VALUES ('Dimas', 'dimas', 1)").execute(&pool).await.unwrap();
    let dir = tmp.path().join("backups");
    let p = snapshot(&pool, &dir, "2026-11-14").await.unwrap().expect("made a snapshot");
    assert_eq!(p.file_name().unwrap(), "rynuvo-2026-11-14.db");
    let copy = db::open_pool(&format!("sqlite://{}", p.display())).await.unwrap();
    let (n,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users").fetch_one(&copy).await.unwrap();
    assert_eq!(n, 1);
    assert!(snapshot(&pool, &dir, "2026-11-14").await.unwrap().is_none(), "one per day");
    assert!(std::fs::read_dir(&dir).unwrap().all(|e| !e.unwrap().file_name().to_string_lossy().ends_with(".tmp")));
}

#[test]
fn prune_keeps_the_newest() {
    let tmp = tempfile::tempdir().unwrap();
    for d in 1..=9 {
        std::fs::write(tmp.path().join(format!("rynuvo-2026-11-{d:02}.db")), b"x").unwrap();
    }
    std::fs::write(tmp.path().join("notes.txt"), b"keep me").unwrap();
    let removed = prune(tmp.path(), 7).unwrap();
    assert_eq!(removed.len(), 2);
    let mut left: Vec<String> = std::fs::read_dir(tmp.path()).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    left.sort();
    assert_eq!(left.first().unwrap(), "notes.txt");
    assert_eq!(left[1], "rynuvo-2026-11-03.db");
    assert_eq!(left.len(), 8);
}
