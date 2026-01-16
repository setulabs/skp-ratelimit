//! Token Bucket rate limiting algorithm.

use std::time::Duration;

use crate::algorithm::{current_timestamp_ms, timestamp_to_instant, Algorithm};
use crate::decision::{Decision, DecisionMetadata, RateLimitInfo};
use crate::error::Result;
use crate::quota::Quota;
use crate::storage::{Storage, StorageEntry};

/// Token Bucket rate limiting algorithm.
///
/// Allows controlled bursts while enforcing an average rate limit.
/// Tokens are refilled at a constant rate up to maximum capacity.
#[derive(Debug, Clone, Default)]
pub struct TokenBucket;

impl TokenBucket {
    /// Create a new Token Bucket algorithm instance.
    pub fn new() -> Self {
        Self
    }

    /// Calculate token refill based on elapsed time.
    fn calculate_refill(&self, elapsed_ms: u64, refill_rate: f64) -> f64 {
        let elapsed_secs = elapsed_ms as f64 / 1000.0;
        elapsed_secs * refill_rate
    }

    /// Build rate limit info from current state.
    fn build_info(&self, tokens: f64, quota: &Quota, now: u64) -> RateLimitInfo {
        let max_tokens = quota.effective_burst();
        let remaining = tokens.floor() as u64;
        let refill_rate = quota.effective_refill_rate();

        let time_to_next_token = if tokens < 1.0 {
            ((1.0 - tokens) / refill_rate * 1000.0) as u64
        } else {
            0
        };

        let tokens_needed = max_tokens as f64 - tokens;
        let time_to_full = if tokens_needed > 0.0 {
            (tokens_needed / refill_rate * 1000.0) as u64
        } else {
            0
        };

        let reset_at = timestamp_to_instant(now + time_to_full);
        let window_start = timestamp_to_instant(now);

        let mut info = RateLimitInfo::new(max_tokens, remaining, reset_at, window_start)
            .with_algorithm("token_bucket")
            .with_metadata(DecisionMetadata::new().with_tokens_available(tokens));

        if remaining == 0 && time_to_next_token > 0 {
            info = info.with_retry_after(Duration::from_millis(time_to_next_token));
        }

        info
    }
}

impl Algorithm for TokenBucket {
    fn name(&self) -> &'static str {
        "token_bucket"
    }

    async fn check_and_record<S: Storage>(
        &self,
        storage: &S,
        key: &str,
        quota: &Quota,
    ) -> Result<Decision> {
        let now = current_timestamp_ms();
        let max_tokens = quota.effective_burst() as f64;
        let refill_rate = quota.effective_refill_rate();

        let ttl_ms = ((max_tokens / refill_rate) * 1000.0 * 2.0) as u64;
        let ttl = Duration::from_millis(ttl_ms.max(1000));

        let decision = storage
            .execute_atomic(key, ttl, |entry| {
                let (mut tokens, last_update) = match entry {
                    Some(e) => (e.tokens.unwrap_or(max_tokens), e.last_update),
                    None => (max_tokens, now),
                };

                if now > last_update {
                    let elapsed = now - last_update;
                    let refill = self.calculate_refill(elapsed, refill_rate);
                    tokens = (tokens + refill).min(max_tokens);
                }

                if tokens >= 1.0 {
                    tokens -= 1.0;
                    let new_entry = StorageEntry::with_tokens(tokens, now);
                    let info = self.build_info(tokens, quota, now);
                    (new_entry, Decision::allowed(info))
                } else {
                    let new_entry = StorageEntry::with_tokens(tokens, now);
                    let info = self.build_info(tokens, quota, now);
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
        let max_tokens = quota.effective_burst() as f64;
        let refill_rate = quota.effective_refill_rate();

        let entry = storage.get(key).await?;

        let (mut tokens, last_update) = match entry {
            Some(e) => (e.tokens.unwrap_or(max_tokens), e.last_update),
            None => (max_tokens, now),
        };

        if now > last_update {
            let elapsed = now - last_update;
            let refill = self.calculate_refill(elapsed, refill_rate);
            tokens = (tokens + refill).min(max_tokens);
        }

        let info = self.build_info(tokens, quota, now);

        Ok(if tokens >= 1.0 {
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
    async fn test_token_bucket_basic() {
        let algorithm = TokenBucket::new();
        let storage = MemoryStorage::new();
        let quota = Quota::per_minute(5).with_burst(5);

        for i in 1..=5 {
            let decision = algorithm.check_and_record(&storage, "user:1", &quota).await.unwrap();
            assert!(decision.is_allowed(), "Request {} should be allowed", i);
        }

        let decision = algorithm.check_and_record(&storage, "user:1", &quota).await.unwrap();
        assert!(decision.is_denied());
    }

    #[tokio::test]
    async fn test_token_bucket_burst() {
        let algorithm = TokenBucket::new();
        let storage = MemoryStorage::new();
        let quota = Quota::per_second(1).with_burst(10);

        for i in 1..=10 {
            let decision = algorithm.check_and_record(&storage, "user:1", &quota).await.unwrap();
            assert!(decision.is_allowed(), "Burst request {} should be allowed", i);
        }

        let decision = algorithm.check_and_record(&storage, "user:1", &quota).await.unwrap();
        assert!(decision.is_denied());
    }

    #[tokio::test]
    async fn test_token_bucket_refill() {
        let algorithm = TokenBucket::new();
        let storage = MemoryStorage::new();
        let quota = Quota::per_second(10).with_burst(1);

        algorithm.check_and_record(&storage, "user:1", &quota).await.unwrap();

        let decision = algorithm.check_and_record(&storage, "user:1", &quota).await.unwrap();
        assert!(decision.is_denied());

        tokio::time::sleep(Duration::from_millis(150)).await;

        let decision = algorithm.check_and_record(&storage, "user:1", &quota).await.unwrap();
        assert!(decision.is_allowed());
    }
}
