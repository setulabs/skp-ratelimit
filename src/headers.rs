//! HTTP headers for rate limiting.
//!
//! Standard and extended headers for communicating rate limit status.

/// Standard rate limit header names.
pub mod names {
    /// Maximum requests allowed per window.
    pub const RATE_LIMIT_LIMIT: &str = "X-RateLimit-Limit";

    /// Remaining requests in current window.
    pub const RATE_LIMIT_REMAINING: &str = "X-RateLimit-Remaining";

    /// Seconds until the rate limit resets.
    pub const RATE_LIMIT_RESET: &str = "X-RateLimit-Reset";

    /// Seconds until the client should retry (standard HTTP header).
    pub const RETRY_AFTER: &str = "Retry-After";

    /// The policy name in effect (extended).
    pub const RATE_LIMIT_POLICY: &str = "X-RateLimit-Policy";

    /// The rate at which requests are consumed (extended).
    pub const RATE_LIMIT_WINDOW: &str = "X-RateLimit-Window";
}

/// Builder for rate limit headers.
#[derive(Debug, Default)]
pub struct RateLimitHeaders {
    limit: Option<u64>,
    remaining: Option<u64>,
    reset: Option<u64>,
    retry_after: Option<u64>,
    policy: Option<String>,
    window: Option<String>,
}

impl RateLimitHeaders {
    /// Create a new header builder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the limit header.
    pub fn limit(mut self, limit: u64) -> Self {
        self.limit = Some(limit);
        self
    }

    /// Set the remaining header.
    pub fn remaining(mut self, remaining: u64) -> Self {
        self.remaining = Some(remaining);
        self
    }

    /// Set the reset header (seconds until reset).
    pub fn reset(mut self, reset_seconds: u64) -> Self {
        self.reset = Some(reset_seconds);
        self
    }

    /// Set the retry-after header (seconds until retry).
    pub fn retry_after(mut self, seconds: u64) -> Self {
        self.retry_after = Some(seconds);
        self
    }

    /// Set the policy header.
    pub fn policy(mut self, policy: impl Into<String>) -> Self {
        self.policy = Some(policy.into());
        self
    }

    /// Set the window header (e.g., "60s", "1m").
    pub fn window(mut self, window: impl Into<String>) -> Self {
        self.window = Some(window.into());
        self
    }

    /// Convert to a vector of (name, value) pairs.
    pub fn to_vec(&self) -> Vec<(&'static str, String)> {
        let mut headers = Vec::new();

        if let Some(limit) = self.limit {
            headers.push((names::RATE_LIMIT_LIMIT, limit.to_string()));
        }
        if let Some(remaining) = self.remaining {
            headers.push((names::RATE_LIMIT_REMAINING, remaining.to_string()));
        }
        if let Some(reset) = self.reset {
            headers.push((names::RATE_LIMIT_RESET, reset.to_string()));
        }
        if let Some(retry_after) = self.retry_after {
            headers.push((names::RETRY_AFTER, retry_after.to_string()));
        }
        if let Some(ref policy) = self.policy {
            headers.push((names::RATE_LIMIT_POLICY, policy.clone()));
        }
        if let Some(ref window) = self.window {
            headers.push((names::RATE_LIMIT_WINDOW, window.clone()));
        }

        headers
    }
}

impl From<&crate::decision::RateLimitInfo> for RateLimitHeaders {
    fn from(info: &crate::decision::RateLimitInfo) -> Self {
        let mut headers = Self::new()
            .limit(info.limit)
            .remaining(info.remaining)
            .reset(info.reset_seconds());

        if let Some(retry) = info.retry_after {
            headers = headers.retry_after(retry.as_secs());
        }

        if let Some(algo) = info.algorithm {
            headers = headers.policy(algo);
        }

        headers
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_header_builder() {
        let headers = RateLimitHeaders::new()
            .limit(100)
            .remaining(50)
            .reset(30)
            .policy("gcra")
            .to_vec();

        assert_eq!(headers.len(), 4);
        assert!(headers.iter().any(|(k, v)| *k == "X-RateLimit-Limit" && v == "100"));
        assert!(headers.iter().any(|(k, v)| *k == "X-RateLimit-Remaining" && v == "50"));
        assert!(headers.iter().any(|(k, v)| *k == "X-RateLimit-Reset" && v == "30"));
        assert!(headers.iter().any(|(k, v)| *k == "X-RateLimit-Policy" && v == "gcra"));
    }

    #[test]
    fn test_headers_with_retry_after() {
        let headers = RateLimitHeaders::new()
            .limit(100)
            .remaining(0)
            .retry_after(60)
            .to_vec();

        assert!(headers.iter().any(|(k, v)| *k == "Retry-After" && v == "60"));
    }
}
