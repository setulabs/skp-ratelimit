//! GCRA (Generic Cell Rate Algorithm) implementation.
//!
//! GCRA is an efficient rate limiting algorithm that tracks a Theoretical Arrival Time (TAT)
//! instead of counters. It's known for:
//! - Low memory usage (only one timestamp per key)
//! - Precise control over request spacing
//! - Excellent burst handling with even distribution
//!
//! # How It Works
//!
//! Instead of counting requests in a window, GCRA tracks when the next request
//! is theoretically allowed (TAT - Theoretical Arrival Time).
//!
//! ```text
//! Period: 100ms between requests (10/sec)
//! Burst: 3 requests allowed ahead
//!
//! Time 0ms:   Request arrives, TAT = 0 + 100 = 100ms. ALLOWED
//! Time 10ms:  Request arrives, TAT = 100 (>10), but 100 < 10+300 (burst). ALLOWED, TAT = 200
//! Time 20ms:  Request arrives, TAT = 200 (>20), but 200 < 20+300. ALLOWED, TAT = 300
//! Time 30ms:  Request arrives, TAT = 300 (>30), but 300 < 30+300. ALLOWED, TAT = 400
//! Time 40ms:  Request arrives, TAT = 400 (>40), and 400 > 40+300. DENIED
//! Time 350ms: Request arrives, TAT = max(350, 400) + 100 = 500. ALLOWED
//! ```

use std::time::Duration;

use crate::algorithm::{current_timestamp_ms, timestamp_to_instant, Algorithm};
use crate::decision::{Decision, DecisionMetadata, RateLimitInfo};
use crate::error::Result;
use crate::quota::Quota;
use crate::storage::{Storage, StorageEntry};

/// GCRA (Generic Cell Rate Algorithm) rate limiter.
///
/// This is the recommended algorithm for most use cases, offering:
/// - Precise rate control with even request spacing
/// - Low memory usage
/// - Excellent burst handling
/// - Clear "retry after" semantics
///
/// # Example
///
/// ```ignore
/// use oc_ratelimit_advanced::{GCRA, Quota, MemoryStorage};
/// use std::time::Duration;
///
/// let algorithm = GCRA::new();
/// let storage = MemoryStorage::new();
/// let quota = Quota::per_second(10).with_burst(15);
///
/// let decision = algorithm.check_and_record(&storage, "user:123", &quota).await?;
/// ```
#[derive(Debug, Clone, Default)]
pub struct GCRA;

impl GCRA {
    /// Create a new GCRA algorithm instance.
    pub fn new() -> Self {
        Self
    }

    /// Calculate the decision based on current TAT and quota.
    fn calculate_decision(
        &self,
        current_tat: Option<u64>,
        now: u64,
        quota: &Quota,
    ) -> (bool, u64) {
        let period_ms = quota.period().as_millis() as u64;
        let max_tat_offset_ms = quota.max_tat_offset().as_millis() as u64;

        // Get effective TAT (starts from now if first request)
        let effective_tat = current_tat.unwrap_or(now);

        // New TAT would be max(now, current_tat) + period
        let new_tat = effective_tat.max(now) + period_ms;

        // Calculate how far ahead we'd be
        let tat_offset = new_tat.saturating_sub(now);

        // Check if within burst tolerance
        if tat_offset <= max_tat_offset_ms + period_ms {
            // Allowed: update TAT
            (true, new_tat)
        } else {
            // Denied: keep current TAT
            (false, effective_tat)
        }
    }

    /// Build rate limit info from current state.
    fn build_info(&self, tat: u64, now: u64, quota: &Quota, allowed: bool) -> RateLimitInfo {
        let period_ms = quota.period().as_millis() as u64;
        let max_tat_offset_ms = quota.max_tat_offset().as_millis() as u64;
        let limit = quota.effective_burst();

        // Calculate remaining "tokens" (how many more requests fit in burst)
        let tat_offset = tat.saturating_sub(now);
        let remaining = if tat_offset == 0 {
            limit
        } else {
            let used = (tat_offset / period_ms) + 1;
            limit.saturating_sub(used)
        };

        // Reset time: when TAT catches up to current time
        let reset_at = if tat > now {
            timestamp_to_instant(tat)
        } else {
            timestamp_to_instant(now)
        };

        let mut info = RateLimitInfo::new(limit, remaining, reset_at, timestamp_to_instant(now))
            .with_algorithm("gcra")
            .with_metadata(DecisionMetadata::new().with_tat(tat));

        // If denied, calculate retry-after
        if !allowed {
            let wait_ms = tat.saturating_sub(now).saturating_sub(max_tat_offset_ms);
            if wait_ms > 0 {
                info = info.with_retry_after(Duration::from_millis(wait_ms));
            }
        }

        info
    }
}

impl Algorithm for GCRA {
    fn name(&self) -> &'static str {
        "gcra"
    }

    async fn check_and_record<S: Storage>(
        &self,
        storage: &S,
        key: &str,
        quota: &Quota,
    ) -> Result<Decision> {
        let now = current_timestamp_ms();
        let period_ms = quota.period().as_millis() as u64;
        
        // TTL based on max TAT offset (how far ahead we can schedule)
        let ttl = Duration::from_millis(
            quota.max_tat_offset().as_millis() as u64 + period_ms * 2
        );

        let decision = storage
            .execute_atomic(key, ttl, |entry| {
                let current_tat = entry.and_then(|e| e.tat);
                let (allowed, new_tat) = self.calculate_decision(current_tat, now, quota);
                
                let new_entry = StorageEntry::with_tat(new_tat);
                let info = self.build_info(new_tat, now, quota, allowed);
                
                let decision = if allowed {
                    Decision::allowed(info)
                } else {
                    Decision::denied(info)
                };
                
                (new_entry, decision)
            })
            .await?;

        Ok(decision)
    }

    async fn check<S: Storage>(
        &self,
        storage: &S,
        key: &str,
        quota: &Quota,
    ) -> Result<Decision> {
        let now = current_timestamp_ms();

        let entry = storage.get(key).await?;
        let current_tat = entry.and_then(|e| e.tat);
        
        let (allowed, effective_tat) = self.calculate_decision(current_tat, now, quota);
        let info = self.build_info(effective_tat, now, quota, allowed);

        Ok(if allowed {
            Decision::allowed(info)
        } else {
            Decision::denied(info)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::MemoryStorage;

    #[tokio::test]
    async fn test_gcra_basic() {
        let algorithm = GCRA::new();
        let storage = MemoryStorage::new();
        let quota = Quota::per_second(10); // 100ms between requests

        // First request should be allowed
        let decision = algorithm
            .check_and_record(&storage, "user:1", &quota)
            .await
            .unwrap();
        assert!(decision.is_allowed());
    }

    #[tokio::test]
    async fn test_gcra_burst() {
        let algorithm = GCRA::new();
        let storage = MemoryStorage::new();
        // 1 request per second with burst of 5
        let quota = Quota::per_second(1).with_burst(5);

        // Should allow burst of 5 requests
        for i in 1..=5 {
            let decision = algorithm
                .check_and_record(&storage, "user:1", &quota)
                .await
                .unwrap();
            assert!(decision.is_allowed(), "Request {} should be allowed", i);
        }

        // 6th request should be denied
        let decision = algorithm
            .check_and_record(&storage, "user:1", &quota)
            .await
            .unwrap();
        assert!(decision.is_denied(), "Request 6 should be denied");
        assert!(decision.info().retry_after.is_some());
    }

    #[tokio::test]
    async fn test_gcra_recovery() {
        let algorithm = GCRA::new();
        let storage = MemoryStorage::new();
        // 10 requests per second (100ms period), burst of 2
        let quota = Quota::per_second(10).with_burst(2);

        // Use up burst
        algorithm.check_and_record(&storage, "user:1", &quota).await.unwrap();
        algorithm.check_and_record(&storage, "user:1", &quota).await.unwrap();

        // Should be denied
        let decision = algorithm
            .check_and_record(&storage, "user:1", &quota)
            .await
            .unwrap();
        assert!(decision.is_denied());

        // Wait for one period
        tokio::time::sleep(Duration::from_millis(150)).await;

        // Should be allowed again
        let decision = algorithm
            .check_and_record(&storage, "user:1", &quota)
            .await
            .unwrap();
        assert!(decision.is_allowed());
    }

    #[tokio::test]
    async fn test_gcra_check_without_record() {
        let algorithm = GCRA::new();
        let storage = MemoryStorage::new();
        let quota = Quota::per_second(10).with_burst(5);

        // Check without recording
        let decision = algorithm.check(&storage, "user:1", &quota).await.unwrap();
        assert!(decision.is_allowed());

        // Check again - should still be allowed (no consumption)
        let decision = algorithm.check(&storage, "user:1", &quota).await.unwrap();
        assert!(decision.is_allowed());

        // Now record one
        algorithm.check_and_record(&storage, "user:1", &quota).await.unwrap();

        // Check should show one less remaining
        let decision = algorithm.check(&storage, "user:1", &quota).await.unwrap();
        assert!(decision.info().remaining < 5);
    }

    #[tokio::test]
    async fn test_gcra_separate_keys() {
        let algorithm = GCRA::new();
        let storage = MemoryStorage::new();
        let quota = Quota::per_second(1).with_burst(1);

        // User 1 uses their quota
        algorithm.check_and_record(&storage, "user:1", &quota).await.unwrap();
        let decision = algorithm.check_and_record(&storage, "user:1", &quota).await.unwrap();
        assert!(decision.is_denied());

        // User 2 should still have quota
        let decision = algorithm.check_and_record(&storage, "user:2", &quota).await.unwrap();
        assert!(decision.is_allowed());
    }

    #[tokio::test]
    async fn test_gcra_reset() {
        let algorithm = GCRA::new();
        let storage = MemoryStorage::new();
        let quota = Quota::per_second(1).with_burst(1);

        // Use quota
        algorithm.check_and_record(&storage, "user:1", &quota).await.unwrap();
        let decision = algorithm.check_and_record(&storage, "user:1", &quota).await.unwrap();
        assert!(decision.is_denied());

        // Reset
        algorithm.reset(&storage, "user:1").await.unwrap();

        // Should be allowed again
        let decision = algorithm.check_and_record(&storage, "user:1", &quota).await.unwrap();
        assert!(decision.is_allowed());
    }

    #[test]
    fn test_algorithm_name() {
        let algorithm = GCRA::new();
        assert_eq!(algorithm.name(), "gcra");
    }
}
