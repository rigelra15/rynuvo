//! Monthly data budget: counting, alerts, persistence and the period rollover.
use rynuvo_server::{
    db,
    quota::{Quota, flush, load_used, period_of},
};

const GB: u64 = 1_000_000_000;

#[test]
fn period_is_the_utc_calendar_month() {
    assert_eq!(period_of(1_794_614_400_000), "2026-11"); // 2026-11-14 00:00 UTC
    assert_eq!(period_of(1_793_491_200_000), "2026-11"); // 2026-11-01 00:00 UTC
    assert_eq!(period_of(1_793_491_199_999), "2026-10");
    assert_eq!(period_of(1_798_761_600_000), "2027-01"); // 2027-01-01
}

#[test]
fn alerts_at_80_and_100_percent() {
    let q = Quota::new(400 * GB);
    q.add(319 * GB);
    assert_eq!(q.alert(), None);
    assert!(!q.relay_blocked());
    q.add(GB);
    let a = q.alert().unwrap();
    assert!(a.contains("80%"), "{a}");
    assert!(a.contains("320 of 400 GB"), "{a}");
    q.add(80 * GB);
    assert!(q.relay_blocked());
    let a = q.alert().unwrap();
    assert!(a.contains("Screen share through the server is paused"), "{a}");
}

#[test]
fn zero_limit_means_unlimited() {
    let q = Quota::new(0);
    q.add(10_000 * GB);
    assert!(!q.relay_blocked());
    assert_eq!(q.alert(), None);
}

#[tokio::test]
async fn flush_persists_per_period_and_rollover_resets() {
    let pool = db::open_pool("sqlite::memory:").await.unwrap();
    let q = Quota::new(400 * GB);
    q.set_period("2026-11", 0);
    q.add(5_000);
    flush(&pool, &q).await.unwrap();
    q.add(1_000);
    flush(&pool, &q).await.unwrap();
    flush(&pool, &q).await.unwrap(); // nothing new: no double counting
    assert_eq!(load_used(&pool, "2026-11").await.unwrap(), 6_000);
    assert_eq!(q.used(), 6_000);
    // A restart picks the month's total back up.
    let again = Quota::new(400 * GB);
    again.set_period("2026-11", load_used(&pool, "2026-11").await.unwrap());
    assert_eq!(again.used(), 6_000);
    // New month: counting starts over.
    again.set_period("2026-12", load_used(&pool, "2026-12").await.unwrap());
    assert_eq!(again.used(), 0);
}
