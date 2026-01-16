//! Sliding Window rate limiting algorithm.

use std::time::Duration;

use crate::algorithm::{current_timestamp_ms, timestamp_to_instant, Algorithm};
use crate::decision::{Decision, RateLimitInfo};
use crate::error::Result;
use crate::quota::Quota;
use crate::storage::{Storage, StorageEntry};

/// Sliding Window rate limiting algorithm.
///
/// Uses weighted combination of current and previous windows
/// to eliminate the boundary burst problem.
#[derive(Debug, Clone, Default)]
pub struct SlidingWindow;

impl SlidingWindow {
    /// Create a new Sliding Window algorithm instance.
    pub fn new() -> Self {
        Self
    }

    /// Calculate the current window start.
    fn window_start(&self, now: u64, window_ms: u64) -> u64 {
        (now / window_ms) * window_ms
    }

    /// Calculate weighted count using current and previous window.
    fn weighted_count(&self, current: u64, previous: u64, window_progress: f64) -> f64 {
        current as f64 + (previous as f64 * (1.0 - window_progress))
    }
}

impl Algorithm for SlidingWindow {
    fn name(&self) -> &'static str {
        "sliding_window"
    }

    async fn check_and_record<S: Storage>(
        &self,
        storage: &S,
        key: &str,
        quota: &Quota,
    ) -> Result<Decision> {
        let now = current_timestamp_ms();
        let window_ms = quota.window().as_millis() as u64;
        let window_start = self.window_start(now, window_ms);
        let ttl = Duration::from_millis(window_ms * 2);
        let limit = quota.max_requests();

        let decision = storage
            .execute_atomic(key, ttl, |entry| {
                let (current_count, prev_count, entry_window) = match &entry {
                    Some(e) if e.window_start == window_start => {
                        (e.count, e.prev_count.unwrap_or(0), window_start)
                    }
                    Some(e) if e.window_start == window_start.saturating_sub(window_ms) => {
                        // We're in a new window, use current as previous
                        (0, e.count, window_start)
                    }
                    _ => (0, 0, window_start),
                };

                let window_progress = (now - window_start) as f64 / window_ms as f64;
                let weighted = self.weighted_count(current_count, prev_count, window_progress);

                if weighted < limit as f64 {
                    let new_entry = StorageEntry::new(current_count + 1, entry_window)
                        .set_prev_count(prev_count)
                        .set_last_update(now);
                    
                    let remaining = (limit as f64 - weighted - 1.0).max(0.0) as u64;
                    let reset_at = timestamp_to_instant(window_start + window_ms);
                    let info = RateLimitInfo::new(limit, remaining, reset_at, timestamp_to_instant(window_start))
                        .with_algorithm("sliding_window");
                    
                    (new_entry, Decision::allowed(info))
                } else {
                    let new_entry = entry.unwrap_or_else(|| StorageEntry::new(current_count, window_start));
                    
                    let reset_at = timestamp_to_instant(window_start + window_ms);
                    let retry_after = Duration::from_millis(window_start + window_ms - now);
                    let info = RateLimitInfo::new(limit, 0, reset_at, timestamp_to_instant(window_start))
                        .with_algorithm("sliding_window")
                        .with_retry_after(retry_after);
                    
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
        let window_start = self.window_start(now, window_ms);
        let limit = quota.max_requests();

        let entry = storage.get(key).await?;

        let (current_count, prev_count) = match &entry {
            Some(e) if e.window_start == window_start => {
                (e.count, e.prev_count.unwrap_or(0))
            }
            Some(e) if e.window_start == window_start.saturating_sub(window_ms) => {
                (0, e.count)
            }
            _ => (0, 0),
        };

        let window_progress = (now - window_start) as f64 / window_ms as f64;
        let weighted = self.weighted_count(current_count, prev_count, window_progress);

        let remaining = (limit as f64 - weighted).max(0.0) as u64;
        let reset_at = timestamp_to_instant(window_start + window_ms);
        let info = RateLimitInfo::new(limit, remaining, reset_at, timestamp_to_instant(window_start))
            .with_algorithm("sliding_window");

        Ok(if weighted < limit as f64 {
            Decision::allowed(info)
        } else {
            let retry_after = Duration::from_millis(window_start + window_ms - now);
            Decision::denied(info.with_retry_after(retry_after))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::MemoryStorage;

    #[tokio::test]
    async fn test_sliding_window_basic() {
        let algorithm = SlidingWindow::new();
        let storage = MemoryStorage::new();
        let quota = Quota::per_minute(5);

        for i in 1..=5 {
            let decision = algorithm.check_and_record(&storage, "user:1", &quota).await.unwrap();
            assert!(decision.is_allowed(), "Request {} should be allowed", i);
        }

        let decision = algorithm.check_and_record(&storage, "user:1", &quota).await.unwrap();
        assert!(decision.is_denied());
    }
}
