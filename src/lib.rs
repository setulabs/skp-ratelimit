//! Advanced, modular rate limiting library for Rust.
//!
//! `skp_ratelimit` provides a comprehensive rate limiting solution with:
//!
//! - **Multiple Algorithms**: GCRA, Token Bucket, Leaky Bucket, Sliding Log, and more
//! - **Pluggable Storage**: In-memory with GC, Redis with connection pooling
//! - **Per-Route Quotas**: Different limits for different endpoints
//! - **Composite Keys**: Rate limit by IP + Path, User + API Key, etc.
//! - **Framework Integration**: Axum and Actix-web middleware
//!
//! # Quick Start
//!
//! ```ignore
//! use skp_ratelimit::{GCRA, Quota, MemoryStorage, Algorithm};
//! use std::time::Duration;
//!
//! #[tokio::main]
//! async fn main() {
//!     // Create storage and algorithm
//!     let storage = MemoryStorage::new();
//!     let algorithm = GCRA::new();
//!     let quota = Quota::per_second(10).with_burst(15);
//!
//!     // Check and record a request
//!     let decision = algorithm.check_and_record(&storage, "user:123", &quota).await.unwrap();
//!
//!     if decision.is_allowed() {
//!         println!("Request allowed! {} remaining", decision.info().remaining);
//!     } else {
//!         println!("Rate limited! Retry after {:?}", decision.info().retry_after);
//!     }
//! }
//! ```
//!
//! # Algorithms
//!
//! | Algorithm | Best For | Memory | Feature Flag |
//! |-----------|----------|--------|--------------|
//! | GCRA | Precise rate control | Low | `gcra` |
//! | Token Bucket | Bursty traffic | Low | default |
//! | Leaky Bucket | Smooth output | Low | `leaky-bucket` |
//! | Sliding Log | Precision critical | High | `sliding-log` |
//! | Sliding Window | General purpose | Low | default |
//! | Fixed Window | Simple use cases | Low | default |
//! | Concurrent | Limit parallelism | Low | `concurrent` |
//!
//! # Feature Flags
//!
//! - `memory` (default): In-memory storage with garbage collection
//! - `redis`: Redis storage backend
//! - `axum`: Axum middleware integration
//! - `gcra`: GCRA algorithm
//! - `leaky-bucket`: Leaky Bucket algorithm
//! - `sliding-log`: Sliding Log algorithm
//! - `concurrent`: Concurrent request limiter

pub mod algorithm;
pub mod decision;
pub mod error;
pub mod extensions;
pub mod headers;
pub mod key;
pub mod manager;
pub mod policy;
pub mod quota;
pub mod storage;

#[cfg(feature = "axum")]
pub mod middleware;

// Re-export main types
pub use algorithm::Algorithm;
pub use decision::{Decision, DecisionMetadata, RateLimitInfo};
pub use error::{ConfigError, ConnectionError, RateLimitError, Result, StorageError};
pub use key::{CompositeKey, FnKey, GlobalKey, Key, StaticKey};
pub use manager::{RateLimitManager, RateLimitManagerBuilder, RouteConfig};
pub use quota::{Quota, QuotaBuilder};
pub use storage::{Storage, StorageEntry};

// Re-export policy types
pub use policy::{CompositePolicy, CreditPolicy, DefaultPolicy, PenaltyPolicy, Policy};

// Re-export extensions and headers
pub use extensions::{RateLimitExt, RateLimitResponse};
pub use headers::RateLimitHeaders;

// Re-export algorithms
pub use algorithm::{FixedWindow, SlidingWindow, TokenBucket};

#[cfg(feature = "gcra")]
pub use algorithm::GCRA;

#[cfg(feature = "leaky-bucket")]
pub use algorithm::LeakyBucket;

#[cfg(feature = "sliding-log")]
pub use algorithm::SlidingLog;

#[cfg(feature = "concurrent")]
pub use algorithm::ConcurrentLimiter;

// Re-export storage types
#[cfg(feature = "memory")]
pub use storage::{GcConfig, GcInterval, MemoryStorage};

/// Prelude module for convenient imports.
pub mod prelude {
    pub use crate::algorithm::Algorithm;
    pub use crate::decision::{Decision, RateLimitInfo};
    pub use crate::error::{RateLimitError, Result};
    pub use crate::quota::Quota;
    pub use crate::storage::Storage;

    pub use crate::algorithm::{FixedWindow, SlidingWindow, TokenBucket};

    #[cfg(feature = "gcra")]
    pub use crate::algorithm::GCRA;

    #[cfg(feature = "leaky-bucket")]
    pub use crate::algorithm::LeakyBucket;

    #[cfg(feature = "sliding-log")]
    pub use crate::algorithm::SlidingLog;

    #[cfg(feature = "concurrent")]
    pub use crate::algorithm::ConcurrentLimiter;

    #[cfg(feature = "memory")]
    pub use crate::storage::{GcConfig, GcInterval, MemoryStorage};
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "memory")]
    #[tokio::test]
    async fn test_integration_gcra() {
        use crate::prelude::*;

        let storage = MemoryStorage::new();
        let algorithm = GCRA::new();
        let quota = Quota::per_second(10).with_burst(5);

        // Should allow burst
        for i in 1..=5 {
            let decision = algorithm
                .check_and_record(&storage, "user:1", &quota)
                .await
                .unwrap();
            assert!(decision.is_allowed(), "Request {} should be allowed", i);
        }

        // Should deny after burst
        let decision = algorithm
            .check_and_record(&storage, "user:1", &quota)
            .await
            .unwrap();
        assert!(decision.is_denied());
        assert!(decision.info().retry_after.is_some());
    }

    #[cfg(feature = "memory")]
    #[tokio::test]
    async fn test_integration_token_bucket() {
        let storage = MemoryStorage::new();
        let algorithm = TokenBucket::new();
        let quota = Quota::per_minute(60).with_burst(10);

        let decision = algorithm
            .check_and_record(&storage, "user:1", &quota)
            .await
            .unwrap();

        assert!(decision.is_allowed());
        assert_eq!(decision.info().remaining, 9);
        assert_eq!(decision.info().algorithm, Some("token_bucket"));
    }

    #[cfg(feature = "memory")]
    #[tokio::test]
    async fn test_integration_headers() {
        let storage = MemoryStorage::new();
        let algorithm = FixedWindow::new();
        let quota = Quota::per_minute(100);

        let decision = algorithm
            .check_and_record(&storage, "user:1", &quota)
            .await
            .unwrap();

        let headers = decision.info().to_headers();
        assert!(headers.iter().any(|(k, _)| *k == "X-RateLimit-Limit"));
        assert!(headers.iter().any(|(k, _)| *k == "X-RateLimit-Remaining"));
        assert!(headers.iter().any(|(k, _)| *k == "X-RateLimit-Reset"));
    }

    #[cfg(all(feature = "memory", feature = "concurrent"))]
    #[tokio::test]
    async fn test_integration_concurrent() {
        let limiter = ConcurrentLimiter::new(2);

        let _permit1 = limiter.try_acquire("user:1").unwrap();
        let _permit2 = limiter.try_acquire("user:1").unwrap();

        assert!(limiter.try_acquire("user:1").is_none());
        assert_eq!(limiter.remaining("user:1"), 0);
    }
}
