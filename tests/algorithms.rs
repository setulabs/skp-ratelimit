//! Integration tests for rate limiting algorithms.

use skp_ratelimit::{Algorithm, MemoryStorage, Quota, GCRA};

#[tokio::test]
async fn test_gcra_basic_rate_limiting() {
    let storage = MemoryStorage::new();
    let algorithm = GCRA::new();
    let quota = Quota::per_second(5).with_burst(5);

    // First 5 requests should be allowed (burst)
    for i in 1..=5 {
        let decision = algorithm
            .check_and_record(&storage, "test:user", &quota)
            .await
            .unwrap();
        assert!(
            decision.is_allowed(),
            "Request {} should be allowed (burst)",
            i
        );
    }

    // 6th request should be denied
    let decision = algorithm
        .check_and_record(&storage, "test:user", &quota)
        .await
        .unwrap();
    assert!(decision.is_denied(), "6th request should be denied");
    assert!(
        decision.info().retry_after.is_some(),
        "Should have retry_after"
    );
}

#[tokio::test]
async fn test_separate_keys_independent() {
    let storage = MemoryStorage::new();
    let algorithm = GCRA::new();
    let quota = Quota::per_second(2).with_burst(2);

    // Exhaust quota for user1
    for _ in 0..2 {
        algorithm
            .check_and_record(&storage, "user:1", &quota)
            .await
            .unwrap();
    }
    let decision = algorithm
        .check_and_record(&storage, "user:1", &quota)
        .await
        .unwrap();
    assert!(decision.is_denied(), "user:1 should be rate limited");

    // user2 should still have quota
    let decision = algorithm
        .check_and_record(&storage, "user:2", &quota)
        .await
        .unwrap();
    assert!(decision.is_allowed(), "user:2 should be allowed");
}

#[tokio::test]
async fn test_rate_limit_headers() {
    let storage = MemoryStorage::new();
    let algorithm = GCRA::new();
    let quota = Quota::per_minute(100).with_burst(50);

    let decision = algorithm
        .check_and_record(&storage, "test:headers", &quota)
        .await
        .unwrap();

    let headers = decision.info().to_headers();

    // Verify required headers exist
    let header_names: Vec<_> = headers.iter().map(|(k, _)| *k).collect();
    assert!(
        header_names.contains(&"X-RateLimit-Limit"),
        "Missing X-RateLimit-Limit"
    );
    assert!(
        header_names.contains(&"X-RateLimit-Remaining"),
        "Missing X-RateLimit-Remaining"
    );
    assert!(
        header_names.contains(&"X-RateLimit-Reset"),
        "Missing X-RateLimit-Reset"
    );
}

#[tokio::test]
async fn test_storage_operations() {
    use skp_ratelimit::storage::{Storage, StorageEntry};
    use std::time::Duration;

    let storage = MemoryStorage::new();

    // Test set/get
    let entry = StorageEntry::new(10, 1000);
    storage
        .set("test:key", entry.clone(), Duration::from_secs(60))
        .await
        .unwrap();

    let retrieved = storage.get("test:key").await.unwrap();
    assert_eq!(retrieved, Some(entry));

    // Test delete
    storage.delete("test:key").await.unwrap();
    let retrieved = storage.get("test:key").await.unwrap();
    assert!(retrieved.is_none());
}

#[tokio::test]
async fn test_increment_operation() {
    use skp_ratelimit::storage::Storage;
    use std::time::Duration;

    let storage = MemoryStorage::new();

    let count = storage
        .increment("test:counter", 1, 1000, Duration::from_secs(60))
        .await
        .unwrap();
    assert_eq!(count, 1);

    let count = storage
        .increment("test:counter", 5, 1000, Duration::from_secs(60))
        .await
        .unwrap();
    assert_eq!(count, 6);
}
