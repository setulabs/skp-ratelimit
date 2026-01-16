//! Request extensions for accessing rate limit info in handlers.
//!
//! This module provides extension types that can be injected into
//! request handlers to access rate limit information.
//!
//! # Example
//!
//! ```ignore
//! use axum::Extension;
//! use oc_ratelimit_advanced::extensions::RateLimitExt;
//!
//! async fn handler(Extension(rate_limit): Extension<RateLimitExt>) {
//!     println!("Remaining: {}", rate_limit.remaining);
//! }
//! ```

use crate::decision::Decision;
use crate::quota::Quota;

/// Rate limit information available via request extensions.
///
/// This is automatically added to requests when using the rate limit middleware.
#[derive(Debug, Clone)]
pub struct RateLimitExt {
    /// The key used for rate limiting this request.
    pub key: String,
    /// The quota applied to this request.
    pub quota: Quota,
    /// The rate limit decision.
    pub decision: Decision,
    /// Whether the request was allowed.
    pub allowed: bool,
    /// Remaining requests in the current window.
    pub remaining: u64,
    /// Maximum requests allowed.
    pub limit: u64,
    /// Seconds until reset.
    pub reset_seconds: u64,
}

impl RateLimitExt {
    /// Create a new rate limit extension from a decision.
    pub fn new(key: impl Into<String>, quota: Quota, decision: Decision) -> Self {
        let info = decision.info();
        Self {
            key: key.into(),
            allowed: decision.is_allowed(),
            remaining: info.remaining,
            limit: info.limit,
            reset_seconds: info.reset_seconds(),
            quota,
            decision,
        }
    }

    /// Check if the request was allowed.
    pub fn is_allowed(&self) -> bool {
        self.allowed
    }

    /// Check if the request was denied.
    pub fn is_denied(&self) -> bool {
        !self.allowed
    }
}

/// Rate limit info that can be serialized to JSON.
///
/// Useful for returning rate limit information in API responses.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RateLimitResponse {
    /// Whether the request was allowed.
    pub allowed: bool,
    /// Maximum requests allowed per window.
    pub limit: u64,
    /// Remaining requests in current window.
    pub remaining: u64,
    /// Seconds until the rate limit resets.
    pub reset_in_seconds: u64,
    /// If denied, the reason.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_after_seconds: Option<u64>,
}

impl From<&RateLimitExt> for RateLimitResponse {
    fn from(ext: &RateLimitExt) -> Self {
        Self {
            allowed: ext.allowed,
            limit: ext.limit,
            remaining: ext.remaining,
            reset_in_seconds: ext.reset_seconds,
            retry_after_seconds: ext
                .decision
                .info()
                .retry_after
                .map(|d| d.as_secs()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decision::RateLimitInfo;
    use std::time::{Duration, Instant};

    #[test]
    fn test_rate_limit_ext() {
        let info = RateLimitInfo::new(100, 50, Instant::now() + Duration::from_secs(60), Instant::now());
        let decision = Decision::allowed(info);
        let quota = Quota::per_minute(100);

        let ext = RateLimitExt::new("user:123", quota, decision);

        assert!(ext.is_allowed());
        assert!(!ext.is_denied());
        assert_eq!(ext.remaining, 50);
        assert_eq!(ext.limit, 100);
    }

    #[test]
    fn test_rate_limit_response_serialization() {
        let info = RateLimitInfo::new(100, 0, Instant::now() + Duration::from_secs(30), Instant::now())
            .with_retry_after(Duration::from_secs(30));
        let decision = Decision::denied(info);
        let quota = Quota::per_minute(100);

        let ext = RateLimitExt::new("user:123", quota, decision);
        let response: RateLimitResponse = (&ext).into();

        assert!(!response.allowed);
        assert_eq!(response.limit, 100);
        assert_eq!(response.remaining, 0);
        assert!(response.retry_after_seconds.is_some());
    }
}
