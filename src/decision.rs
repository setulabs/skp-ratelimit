//! Decision types for rate limiting results.
//!
//! When a rate limit check is performed, the result is a `Decision` that indicates
//! whether the request is allowed or denied, along with metadata about the current
//! rate limit state.

use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

/// The result of a rate limit check.
#[derive(Debug, Clone)]
pub struct Decision {
    /// Whether the request is allowed.
    allowed: bool,
    /// Rate limit information.
    info: RateLimitInfo,
}

impl Decision {
    /// Create a new "allowed" decision.
    pub fn allowed(info: RateLimitInfo) -> Self {
        Self {
            allowed: true,
            info,
        }
    }

    /// Create a new "denied" decision.
    pub fn denied(info: RateLimitInfo) -> Self {
        Self {
            allowed: false,
            info,
        }
    }

    /// Check if the request is allowed.
    pub fn is_allowed(&self) -> bool {
        self.allowed
    }

    /// Check if the request is denied.
    pub fn is_denied(&self) -> bool {
        !self.allowed
    }

    /// Get the rate limit info.
    pub fn info(&self) -> &RateLimitInfo {
        &self.info
    }

    /// Consume the decision and return the info.
    pub fn into_info(self) -> RateLimitInfo {
        self.info
    }
}

/// Information about the current rate limit state.
#[derive(Debug, Clone)]
pub struct RateLimitInfo {
    /// Maximum requests allowed.
    pub limit: u64,
    /// Remaining requests in the current window.
    pub remaining: u64,
    /// When the rate limit resets.
    pub reset_at: Instant,
    /// Start of the current window.
    pub window_start: Instant,
    /// How long to wait before retrying (only set when rate limited).
    pub retry_after: Option<Duration>,
    /// Name of the algorithm that made this decision.
    pub algorithm: Option<&'static str>,
    /// Additional metadata.
    pub metadata: Option<DecisionMetadata>,
}

impl RateLimitInfo {
    /// Create a new rate limit info.
    pub fn new(limit: u64, remaining: u64, reset_at: Instant, window_start: Instant) -> Self {
        Self {
            limit,
            remaining,
            reset_at,
            window_start,
            retry_after: None,
            algorithm: None,
            metadata: None,
        }
    }

    /// Set the retry-after duration.
    pub fn with_retry_after(mut self, duration: Duration) -> Self {
        self.retry_after = Some(duration);
        self
    }

    /// Set the algorithm name.
    pub fn with_algorithm(mut self, name: &'static str) -> Self {
        self.algorithm = Some(name);
        self
    }

    /// Set additional metadata.
    pub fn with_metadata(mut self, metadata: DecisionMetadata) -> Self {
        self.metadata = Some(metadata);
        self
    }

    /// Get the remaining time until reset as a Duration.
    pub fn time_until_reset(&self) -> Duration {
        self.reset_at.saturating_duration_since(Instant::now())
    }

    /// Get reset time as seconds from now.
    pub fn reset_seconds(&self) -> u64 {
        self.time_until_reset().as_secs()
    }

    /// Convert to HTTP headers.
    ///
    /// Returns a vector of (header_name, header_value) pairs.
    pub fn to_headers(&self) -> Vec<(&'static str, String)> {
        let mut headers = vec![
            ("X-RateLimit-Limit", self.limit.to_string()),
            ("X-RateLimit-Remaining", self.remaining.to_string()),
            ("X-RateLimit-Reset", self.reset_seconds().to_string()),
        ];

        if let Some(retry_after) = self.retry_after {
            headers.push(("Retry-After", retry_after.as_secs().to_string()));
        }

        if let Some(algorithm) = self.algorithm {
            headers.push(("X-RateLimit-Policy", algorithm.to_string()));
        }

        headers
    }
}

/// Additional metadata about a rate limit decision.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionMetadata {
    /// The key that was rate limited.
    pub key: Option<String>,
    /// The route that was rate limited.
    pub route: Option<String>,
    /// Tokens consumed (for token bucket).
    pub tokens_consumed: Option<f64>,
    /// Current tokens available (for token bucket).
    pub tokens_available: Option<f64>,
    /// Theoretical arrival time (for GCRA).
    pub tat: Option<u64>,
}

impl DecisionMetadata {
    /// Create new empty metadata.
    pub fn new() -> Self {
        Self {
            key: None,
            route: None,
            tokens_consumed: None,
            tokens_available: None,
            tat: None,
        }
    }

    /// Set the key.
    pub fn with_key(mut self, key: impl Into<String>) -> Self {
        self.key = Some(key.into());
        self
    }

    /// Set the route.
    pub fn with_route(mut self, route: impl Into<String>) -> Self {
        self.route = Some(route.into());
        self
    }

    /// Set tokens consumed.
    pub fn with_tokens_consumed(mut self, tokens: f64) -> Self {
        self.tokens_consumed = Some(tokens);
        self
    }

    /// Set tokens available.
    pub fn with_tokens_available(mut self, tokens: f64) -> Self {
        self.tokens_available = Some(tokens);
        self
    }

    /// Set GCRA TAT.
    pub fn with_tat(mut self, tat: u64) -> Self {
        self.tat = Some(tat);
        self
    }
}

impl Default for DecisionMetadata {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decision_allowed() {
        let info = RateLimitInfo::new(100, 99, Instant::now(), Instant::now());
        let decision = Decision::allowed(info);
        
        assert!(decision.is_allowed());
        assert!(!decision.is_denied());
        assert_eq!(decision.info().limit, 100);
        assert_eq!(decision.info().remaining, 99);
    }

    #[test]
    fn test_decision_denied() {
        let info = RateLimitInfo::new(100, 0, Instant::now(), Instant::now())
            .with_retry_after(Duration::from_secs(30));
        let decision = Decision::denied(info);
        
        assert!(decision.is_denied());
        assert!(!decision.is_allowed());
        assert_eq!(decision.info().remaining, 0);
        assert_eq!(decision.info().retry_after, Some(Duration::from_secs(30)));
    }

    #[test]
    fn test_rate_limit_info_headers() {
        let reset = Instant::now() + Duration::from_secs(60);
        let info = RateLimitInfo::new(100, 50, reset, Instant::now())
            .with_algorithm("gcra")
            .with_retry_after(Duration::from_secs(10));

        let headers = info.to_headers();
        
        assert!(headers.iter().any(|(k, v)| *k == "X-RateLimit-Limit" && v == "100"));
        assert!(headers.iter().any(|(k, v)| *k == "X-RateLimit-Remaining" && v == "50"));
        assert!(headers.iter().any(|(k, _)| *k == "X-RateLimit-Reset"));
        assert!(headers.iter().any(|(k, v)| *k == "Retry-After" && v == "10"));
        assert!(headers.iter().any(|(k, v)| *k == "X-RateLimit-Policy" && v == "gcra"));
    }

    #[test]
    fn test_decision_metadata() {
        let metadata = DecisionMetadata::new()
            .with_key("user:123")
            .with_route("/api/data")
            .with_tokens_available(5.5);

        assert_eq!(metadata.key, Some("user:123".into()));
        assert_eq!(metadata.route, Some("/api/data".into()));
        assert_eq!(metadata.tokens_available, Some(5.5));
    }
}
