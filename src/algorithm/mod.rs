//! Rate limiting algorithm trait and implementations.
//!
//! This module defines the `Algorithm` trait and provides implementations
//! for various rate limiting algorithms.
//!
//! # Available Algorithms
//!
//! - **GCRA** (`gcra` feature): Generic Cell Rate Algorithm - precise, low memory
//! - **Token Bucket** (default): Controlled bursts with refilling tokens
//! - **Leaky Bucket** (`leaky-bucket` feature): Smooth constant output rate
//! - **Sliding Log** (`sliding-log` feature): High precision, stores all timestamps
//! - **Sliding Window** (default): Weighted window for balanced accuracy
//! - **Fixed Window** (default): Simple counter per time window
//! - **Concurrent** (`concurrent` feature): Limit simultaneous requests

#[cfg(feature = "gcra")]
mod gcra;
#[cfg(feature = "leaky-bucket")]
mod leaky_bucket;
#[cfg(feature = "sliding-log")]
mod sliding_log;
#[cfg(feature = "concurrent")]
mod concurrent;
mod fixed_window;
mod sliding_window;
mod token_bucket;

#[cfg(feature = "gcra")]
pub use gcra::GCRA;
#[cfg(feature = "leaky-bucket")]
pub use leaky_bucket::LeakyBucket;
#[cfg(feature = "sliding-log")]
pub use sliding_log::SlidingLog;
#[cfg(feature = "concurrent")]
pub use concurrent::ConcurrentLimiter;
pub use fixed_window::FixedWindow;
pub use sliding_window::SlidingWindow;
pub use token_bucket::TokenBucket;

use std::future::Future;

use crate::decision::Decision;
use crate::error::Result;
use crate::quota::Quota;
use crate::storage::Storage;

/// Rate limiting algorithm trait.
///
/// Each algorithm provides different trade-offs between accuracy, memory usage,
/// and burst handling. All implementations must be thread-safe.
///
/// # Algorithm Comparison
///
/// | Algorithm | Accuracy | Memory | Burst | Best For |
/// |-----------|----------|--------|-------|----------|
/// | GCRA | Highest | Low (1 timestamp) | Controlled | Precise rate control |
/// | Token Bucket | High | Low | Excellent | Bursty traffic |
/// | Leaky Bucket | High | Medium | None | Smooth output |
/// | Sliding Log | Highest | High | Good | Precision critical |
/// | Sliding Window | Medium | Low | Good | General purpose |
/// | Fixed Window | Low | Low | Poor | Simple use cases |
/// | Concurrent | N/A | Low | N/A | Limit parallelism |
pub trait Algorithm: Send + Sync + 'static {
    /// Get the algorithm name (for logging/metrics).
    fn name(&self) -> &'static str;

    /// Check if a request is allowed AND record it atomically.
    ///
    /// This is the primary method for rate limiting. It checks whether the
    /// request should be allowed and, if so, records it against the quota.
    fn check_and_record<S: Storage>(
        &self,
        storage: &S,
        key: &str,
        quota: &Quota,
    ) -> impl Future<Output = Result<Decision>> + Send;

    /// Check without recording (peek at current state).
    ///
    /// Useful for displaying rate limit info without consuming quota.
    fn check<S: Storage>(
        &self,
        storage: &S,
        key: &str,
        quota: &Quota,
    ) -> impl Future<Output = Result<Decision>> + Send;

    /// Reset the rate limit for a key.
    fn reset<S: Storage>(&self, storage: &S, key: &str) -> impl Future<Output = Result<()>> + Send {
        async move { storage.delete(key).await }
    }
}

/// Get the current timestamp in milliseconds since Unix epoch.
pub(crate) fn current_timestamp_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards")
        .as_millis() as u64
}

/// Convert a timestamp to an Instant (approximate).
pub(crate) fn timestamp_to_instant(timestamp_ms: u64) -> std::time::Instant {
    let now = std::time::Instant::now();
    let now_ms = current_timestamp_ms();

    if timestamp_ms >= now_ms {
        now + std::time::Duration::from_millis(timestamp_ms - now_ms)
    } else {
        now - std::time::Duration::from_millis(now_ms - timestamp_ms)
    }
}
