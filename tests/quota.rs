//! Integration tests for quota configuration.

use skp_ratelimit::Quota;
use std::time::Duration;

#[test]
fn test_quota_per_second() {
    let quota = Quota::per_second(10);
    assert_eq!(quota.max_requests(), 10);
    assert_eq!(quota.window(), Duration::from_secs(1));
}

#[test]
fn test_quota_per_minute() {
    let quota = Quota::per_minute(60);
    assert_eq!(quota.max_requests(), 60);
    assert_eq!(quota.window(), Duration::from_secs(60));
}

#[test]
fn test_quota_with_burst() {
    let quota = Quota::per_second(10).with_burst(20);
    assert_eq!(quota.max_requests(), 10);
    assert_eq!(quota.effective_burst(), 20);
}

#[test]
fn test_quota_custom_window() {
    let quota = Quota::new(100, Duration::from_secs(300)); // 100 per 5 minutes
    assert_eq!(quota.max_requests(), 100);
    assert_eq!(quota.window(), Duration::from_secs(300));
}

#[test]
fn test_quota_builder() {
    use skp_ratelimit::QuotaBuilder;

    let quota = QuotaBuilder::new()
        .max_requests(50)
        .window(Duration::from_secs(60))
        .burst(100)
        .build()
        .unwrap();

    assert_eq!(quota.max_requests(), 50);
    assert_eq!(quota.window(), Duration::from_secs(60));
    assert_eq!(quota.effective_burst(), 100);
}
