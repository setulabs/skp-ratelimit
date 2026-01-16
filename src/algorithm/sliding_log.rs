//! Sliding Log rate limiting algorithm.
//!
//! The Sliding Log algorithm stores timestamps of all requests within the window,
//! providing the highest accuracy but with higher memory usage.

use std::time::Duration;

use crate::algorithm::{current_timestamp_ms, timestamp_to_instant, Algorithm};
use crate::decision::{Decision, RateLimitInfo};
use crate::error::Result;
use crate::quota::Quota;
use crate::storage::{Storage, StorageEntry};

/// Sliding Log rate limiting algorithm.
///
/// Stores timestamp of every request for highest precision.
/// Best for accuracy-critical applications.
#[derive(Debug, Clone, Default)]
pub struct SlidingLog;

impl SlidingLog {
    /// Create a new Sliding Log algorithm instance.
    pub fn new() -> Self {
        Self
    }

    /// Filter timestamps to only include those within the window.
    fn filter_window(&self, timestamps: &[u64], window_start: u64) -> Vec<u64> {
        timestamps
            .iter()
            .filter(|&&ts| ts >= window_start)
            .copied()
            .collect()
    }
}

impl Algorithm for SlidingLog {
    fn name(&self) -> &'static str {
        "sliding_log"
    }

    async fn check_and_record<S: Storage>(
        &self,
        storage: &S,
        key: &str,
        quota: &Quota,
    ) -> Result<Decision> {
        let now = current_timestamp_ms();
        let window_ms = quota.window().as_millis() as u64;
        let window_start = now.saturating_sub(window_ms);
        let ttl = Duration::from_millis(window_ms * 2);
        let limit = quota.max_requests();

        let decision = storage
            .execute_atomic(key, ttl, |entry| {
                let mut timestamps = entry
                    .and_then(|e| e.timestamps)
                    .unwrap_or_default();

                // Filter to only requests within window
                timestamps = self.filter_window(&timestamps, window_start);
                let current_count = timestamps.len() as u64;

                if current_count < limit {
                    timestamps.push(now);
                    let new_entry = StorageEntry::with_timestamps(timestamps);
                    
                    let remaining = limit - current_count - 1;
                    let reset_at = timestamp_to_instant(now + window_ms);
                    let info = RateLimitInfo::new(limit, remaining, reset_at, timestamp_to_instant(window_start))
                        .with_algorithm("sliding_log");
                    
                    (new_entry, Decision::allowed(info))
                } else {
                    let new_entry = StorageEntry::with_timestamps(timestamps.clone());
                    
                    // Find when oldest request will expire
                    let oldest = timestamps.first().copied().unwrap_or(now);
                    let retry_ms = oldest + window_ms - now;
                    let reset_at = timestamp_to_instant(oldest + window_ms);
                    
                    let info = RateLimitInfo::new(limit, 0, reset_at, timestamp_to_instant(window_start))
                        .with_algorithm("sliding_log")
                        .with_retry_after(Duration::from_millis(retry_ms));
                    
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
        let window_ms = quota.window().as_millis() as u64;
        let window_start = now.saturating_sub(window_ms);
        let limit = quota.max_requests();

        let entry = storage.get(key).await?;
        let timestamps = entry
            .and_then(|e| e.timestamps)
            .unwrap_or_default();

        let filtered = self.filter_window(&timestamps, window_start);
        let current_count = filtered.len() as u64;

        let remaining = limit.saturating_sub(current_count);
        let reset_at = if let Some(&oldest) = filtered.first() {
            timestamp_to_instant(oldest + window_ms)
        } else {
            timestamp_to_instant(now + window_ms)
        };

        let info = RateLimitInfo::new(limit, remaining, reset_at, timestamp_to_instant(window_start))
            .with_algorithm("sliding_log");

        Ok(if current_count < limit {
            Decision::allowed(info)
        } else {
            let oldest = filtered.first().copied().unwrap_or(now);
            let retry_ms = oldest + window_ms - now;
            Decision::denied(info.with_retry_after(Duration::from_millis(retry_ms)))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::MemoryStorage;

    #[tokio::test]
    async fn test_sliding_log_basic() {
        let algorithm = SlidingLog::new();
        let storage = MemoryStorage::new();
        let quota = Quota::per_minute(5);

        for i in 1..=5 {
            let decision = algorithm.check_and_record(&storage, "user:1", &quota).await.unwrap();
            assert!(decision.is_allowed(), "Request {} should be allowed", i);
        }

        let decision = algorithm.check_and_record(&storage, "user:1", &quota).await.unwrap();
        assert!(decision.is_denied());
    }

    #[tokio::test]
    async fn test_sliding_log_precision() {
        let algorithm = SlidingLog::new();
        let storage = MemoryStorage::new();
        // 2 requests per 200ms
        let quota = Quota::new(2, Duration::from_millis(200));

        algorithm.check_and_record(&storage, "user:1", &quota).await.unwrap();
        tokio::time::sleep(Duration::from_millis(50)).await;
        algorithm.check_and_record(&storage, "user:1", &quota).await.unwrap();

        let decision = algorithm.check_and_record(&storage, "user:1", &quota).await.unwrap();
        assert!(decision.is_denied());

        // Wait for first request to expire from window
        tokio::time::sleep(Duration::from_millis(200)).await;

        let decision = algorithm.check_and_record(&storage, "user:1", &quota).await.unwrap();
        assert!(decision.is_allowed());
    }
}
