//! Leaky Bucket rate limiting algorithm.
//!
//! The Leaky Bucket algorithm smooths out bursty traffic by processing
//! requests at a constant rate, like water leaking from a bucket.

use std::time::Duration;

use crate::algorithm::{current_timestamp_ms, timestamp_to_instant, Algorithm};
use crate::decision::{Decision, DecisionMetadata, RateLimitInfo};
use crate::error::Result;
use crate::quota::Quota;
use crate::storage::{Storage, StorageEntry};

/// Leaky Bucket rate limiting algorithm.
///
/// Enforces a constant output rate regardless of input bursts.
/// Requests that would overflow the bucket are rejected.
#[derive(Debug, Clone, Default)]
pub struct LeakyBucket;

impl LeakyBucket {
    /// Create a new Leaky Bucket algorithm instance.
    pub fn new() -> Self {
        Self
    }

    /// Calculate how much has "leaked" based on elapsed time.
    fn calculate_leak(&self, elapsed_ms: u64, leak_rate: f64) -> f64 {
        let elapsed_secs = elapsed_ms as f64 / 1000.0;
        elapsed_secs * leak_rate
    }
}

impl Algorithm for LeakyBucket {
    fn name(&self) -> &'static str {
        "leaky_bucket"
    }

    async fn check_and_record<S: Storage>(
        &self,
        storage: &S,
        key: &str,
        quota: &Quota,
    ) -> Result<Decision> {
        let now = current_timestamp_ms();
        let max_level = quota.effective_burst() as f64;
        let leak_rate = quota.effective_refill_rate(); // tokens leak out per second

        let ttl_ms = ((max_level / leak_rate) * 1000.0 * 2.0) as u64;
        let ttl = Duration::from_millis(ttl_ms.max(1000));

        let decision = storage
            .execute_atomic(key, ttl, |entry| {
                let (mut level, last_update) = match entry {
                    Some(e) => (e.tokens.unwrap_or(0.0), e.last_update),
                    None => (0.0, now),
                };

                // Leak tokens based on elapsed time
                if now > last_update {
                    let elapsed = now - last_update;
                    let leaked = self.calculate_leak(elapsed, leak_rate);
                    level = (level - leaked).max(0.0);
                }

                // Try to add a "drop" to the bucket
                if level + 1.0 <= max_level {
                    level += 1.0;
                    let new_entry = StorageEntry::with_tokens(level, now);
                    
                    let remaining = (max_level - level).floor() as u64;
                    let drain_time = (level / leak_rate * 1000.0) as u64;
                    let reset_at = timestamp_to_instant(now + drain_time);
                    
                    let info = RateLimitInfo::new(max_level as u64, remaining, reset_at, timestamp_to_instant(now))
                        .with_algorithm("leaky_bucket")
                        .with_metadata(DecisionMetadata::new().with_tokens_available(max_level - level));
                    
                    (new_entry, Decision::allowed(info))
                } else {
                    let new_entry = StorageEntry::with_tokens(level, now);
                    
                    // Calculate when there's room for another request
                    let wait_ms = ((level + 1.0 - max_level) / leak_rate * 1000.0) as u64;
                    let reset_at = timestamp_to_instant(now + wait_ms);
                    
                    let info = RateLimitInfo::new(max_level as u64, 0, reset_at, timestamp_to_instant(now))
                        .with_algorithm("leaky_bucket")
                        .with_retry_after(Duration::from_millis(wait_ms));
                    
                    (new_entry, Decision::denied(info))
                }
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
        let max_level = quota.effective_burst() as f64;
        let leak_rate = quota.effective_refill_rate();

        let entry = storage.get(key).await?;

        let (mut level, last_update) = match entry {
            Some(e) => (e.tokens.unwrap_or(0.0), e.last_update),
            None => (0.0, now),
        };

        if now > last_update {
            let elapsed = now - last_update;
            let leaked = self.calculate_leak(elapsed, leak_rate);
            level = (level - leaked).max(0.0);
        }

        let remaining = (max_level - level).floor() as u64;
        let drain_time = (level / leak_rate * 1000.0) as u64;
        let reset_at = timestamp_to_instant(now + drain_time);

        let info = RateLimitInfo::new(max_level as u64, remaining, reset_at, timestamp_to_instant(now))
            .with_algorithm("leaky_bucket");

        Ok(if level + 1.0 <= max_level {
            Decision::allowed(info)
        } else {
            let wait_ms = ((level + 1.0 - max_level) / leak_rate * 1000.0) as u64;
            Decision::denied(info.with_retry_after(Duration::from_millis(wait_ms)))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::MemoryStorage;

    #[tokio::test]
    async fn test_leaky_bucket_basic() {
        let algorithm = LeakyBucket::new();
        let storage = MemoryStorage::new();
        let quota = Quota::per_second(10).with_burst(5);

        for i in 1..=5 {
            let decision = algorithm.check_and_record(&storage, "user:1", &quota).await.unwrap();
            assert!(decision.is_allowed(), "Request {} should be allowed", i);
        }

        let decision = algorithm.check_and_record(&storage, "user:1", &quota).await.unwrap();
        assert!(decision.is_denied());
    }

    #[tokio::test]
    async fn test_leaky_bucket_drain() {
        let algorithm = LeakyBucket::new();
        let storage = MemoryStorage::new();
        let quota = Quota::per_second(10).with_burst(2);

        // Fill the bucket
        algorithm.check_and_record(&storage, "user:1", &quota).await.unwrap();
        algorithm.check_and_record(&storage, "user:1", &quota).await.unwrap();

        let decision = algorithm.check_and_record(&storage, "user:1", &quota).await.unwrap();
        assert!(decision.is_denied());

        // Wait for some to drain
        tokio::time::sleep(Duration::from_millis(150)).await;

        let decision = algorithm.check_and_record(&storage, "user:1", &quota).await.unwrap();
        assert!(decision.is_allowed());
    }
}
