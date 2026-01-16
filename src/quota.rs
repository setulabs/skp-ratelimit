//! Quota configuration for rate limiting.
//!
//! A `Quota` defines the rate limiting parameters: how many requests are allowed
//! over what time period, and optionally how much burst capacity is available.
//!
//! # Examples
//!
//! ```ignore
//! use oc_ratelimit_advanced::Quota;
//! use std::time::Duration;
//!
//! // 100 requests per minute
//! let quota = Quota::per_minute(100);
//!
//! // 100 requests per minute with burst of 150
//! let quota = Quota::per_minute(100).with_burst(150);
//!
//! // GCRA-style: one request per 100ms
//! let quota = Quota::simple(Duration::from_millis(100));
//!
//! // Custom: 50 requests per 30 seconds
//! let quota = Quota::new(50, Duration::from_secs(30));
//! ```

use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::error::{ConfigError, Result};

/// Rate limiting quota configuration.
///
/// A quota defines the maximum number of requests allowed within a time window,
/// along with optional burst capacity for handling traffic spikes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Quota {
    /// Maximum number of requests in the window.
    max_requests: u64,

    /// Time window duration.
    window: Duration,

    /// Maximum burst size (defaults to max_requests if not set).
    burst: Option<u64>,

    /// Refill rate for token-based algorithms (tokens per second).
    /// If not set, calculated from max_requests / window.
    refill_rate: Option<f64>,
}

impl Quota {
    /// Create a new quota with the given maximum requests and window.
    ///
    /// # Arguments
    ///
    /// * `max_requests` - Maximum requests allowed in the window
    /// * `window` - Duration of the rate limiting window
    ///
    /// # Panics
    ///
    /// Panics if `max_requests` is 0 or `window` is zero duration.
    pub fn new(max_requests: u64, window: Duration) -> Self {
        assert!(max_requests > 0, "max_requests must be greater than 0");
        assert!(!window.is_zero(), "window must be non-zero");

        Self {
            max_requests,
            window,
            burst: None,
            refill_rate: None,
        }
    }

    /// Create a quota allowing `n` requests per second.
    pub fn per_second(n: u64) -> Self {
        Self::new(n, Duration::from_secs(1))
    }

    /// Create a quota allowing `n` requests per minute.
    pub fn per_minute(n: u64) -> Self {
        Self::new(n, Duration::from_secs(60))
    }

    /// Create a quota allowing `n` requests per hour.
    pub fn per_hour(n: u64) -> Self {
        Self::new(n, Duration::from_secs(3600))
    }

    /// Create a quota allowing `n` requests per day.
    pub fn per_day(n: u64) -> Self {
        Self::new(n, Duration::from_secs(86400))
    }

    /// Create a GCRA-style simple quota with a fixed period between requests.
    ///
    /// This is equivalent to 1 request per `period`, suitable for GCRA algorithm.
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Allow 1 request every 100ms (10 requests per second)
    /// let quota = Quota::simple(Duration::from_millis(100));
    /// ```
    pub fn simple(period: Duration) -> Self {
        Self::new(1, period)
    }

    /// Create a GCRA-style quota with burst allowance.
    ///
    /// # Arguments
    ///
    /// * `period` - Minimum time between requests
    /// * `burst` - Maximum burst capacity
    pub fn with_period_and_burst(period: Duration, burst: u64) -> Self {
        Self::new(1, period).with_burst(burst)
    }

    /// Try to create a new quota, returning an error if invalid.
    pub fn try_new(max_requests: u64, window: Duration) -> Result<Self> {
        if max_requests == 0 {
            return Err(ConfigError::InvalidQuota("max_requests must be greater than 0".into()).into());
        }
        if window.is_zero() {
            return Err(ConfigError::InvalidQuota("window must be non-zero".into()).into());
        }
        Ok(Self {
            max_requests,
            window,
            burst: None,
            refill_rate: None,
        })
    }

    /// Set the burst size (maximum requests that can be made instantly).
    ///
    /// Burst must be >= max_requests.
    pub fn with_burst(mut self, burst: u64) -> Self {
        self.burst = Some(burst.max(self.max_requests));
        self
    }

    /// Set a custom refill rate (tokens per second).
    ///
    /// If not set, the refill rate is calculated as `max_requests / window_seconds`.
    pub fn with_refill_rate(mut self, rate: f64) -> Self {
        self.refill_rate = Some(rate);
        self
    }

    /// Get the maximum requests allowed per window.
    pub fn max_requests(&self) -> u64 {
        self.max_requests
    }

    /// Get the window duration.
    pub fn window(&self) -> Duration {
        self.window
    }

    /// Get the effective burst size.
    ///
    /// Returns the configured burst, or `max_requests` if not set.
    pub fn effective_burst(&self) -> u64 {
        self.burst.unwrap_or(self.max_requests)
    }

    /// Get the effective refill rate (tokens per second).
    ///
    /// Returns the configured rate, or calculates from `max_requests / window_seconds`.
    pub fn effective_refill_rate(&self) -> f64 {
        self.refill_rate.unwrap_or_else(|| {
            self.max_requests as f64 / self.window.as_secs_f64()
        })
    }

    /// Get the period between requests for GCRA.
    ///
    /// For GCRA, this is the minimum time that must elapse between requests.
    pub fn period(&self) -> Duration {
        Duration::from_secs_f64(self.window.as_secs_f64() / self.max_requests as f64)
    }

    /// Get the maximum time shift for GCRA (burst tolerance).
    ///
    /// This is how far ahead the theoretical arrival time (TAT) can be
    /// while still allowing the request.
    pub fn max_tat_offset(&self) -> Duration {
        let burst = self.effective_burst();
        Duration::from_secs_f64(self.period().as_secs_f64() * (burst - 1) as f64)
    }

    /// Calculate how long until a quota would be fully replenished.
    pub fn full_replenish_time(&self) -> Duration {
        self.window
    }
}

impl Default for Quota {
    fn default() -> Self {
        Self::per_minute(60)
    }
}

/// Builder for creating quotas with validation.
#[derive(Debug, Default)]
pub struct QuotaBuilder {
    max_requests: Option<u64>,
    window: Option<Duration>,
    burst: Option<u64>,
    refill_rate: Option<f64>,
}

impl QuotaBuilder {
    /// Create a new quota builder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the maximum requests per window.
    pub fn max_requests(mut self, n: u64) -> Self {
        self.max_requests = Some(n);
        self
    }

    /// Set the window duration.
    pub fn window(mut self, duration: Duration) -> Self {
        self.window = Some(duration);
        self
    }

    /// Set the burst size.
    pub fn burst(mut self, n: u64) -> Self {
        self.burst = Some(n);
        self
    }

    /// Set the refill rate.
    pub fn refill_rate(mut self, rate: f64) -> Self {
        self.refill_rate = Some(rate);
        self
    }

    /// Build the quota, returning an error if invalid.
    pub fn build(self) -> Result<Quota> {
        let max_requests = self.max_requests
            .ok_or_else(|| ConfigError::MissingRequired("max_requests".into()))?;
        let window = self.window
            .ok_or_else(|| ConfigError::MissingRequired("window".into()))?;

        let mut quota = Quota::try_new(max_requests, window)?;

        if let Some(burst) = self.burst {
            quota = quota.with_burst(burst);
        }
        if let Some(rate) = self.refill_rate {
            quota = quota.with_refill_rate(rate);
        }

        Ok(quota)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_quota_per_second() {
        let quota = Quota::per_second(10);
        assert_eq!(quota.max_requests(), 10);
        assert_eq!(quota.window(), Duration::from_secs(1));
        assert_eq!(quota.effective_burst(), 10);
        assert!((quota.effective_refill_rate() - 10.0).abs() < 0.001);
    }

    #[test]
    fn test_quota_per_minute() {
        let quota = Quota::per_minute(60);
        assert_eq!(quota.max_requests(), 60);
        assert_eq!(quota.window(), Duration::from_secs(60));
        assert!((quota.effective_refill_rate() - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_quota_with_burst() {
        let quota = Quota::per_minute(60).with_burst(100);
        assert_eq!(quota.max_requests(), 60);
        assert_eq!(quota.effective_burst(), 100);
    }

    #[test]
    fn test_quota_burst_minimum() {
        // Burst should be at least max_requests
        let quota = Quota::per_minute(60).with_burst(30);
        assert_eq!(quota.effective_burst(), 60);
    }

    #[test]
    fn test_quota_simple() {
        let quota = Quota::simple(Duration::from_millis(100));
        assert_eq!(quota.max_requests(), 1);
        assert_eq!(quota.window(), Duration::from_millis(100));
        assert_eq!(quota.period(), Duration::from_millis(100));
    }

    #[test]
    fn test_quota_gcra_period() {
        let quota = Quota::per_second(10);
        assert_eq!(quota.period(), Duration::from_millis(100));
    }

    #[test]
    fn test_quota_max_tat_offset() {
        let quota = Quota::per_second(1).with_burst(5);
        // With 5 burst, can be 4 periods ahead
        let offset = quota.max_tat_offset();
        assert_eq!(offset, Duration::from_secs(4));
    }

    #[test]
    fn test_quota_builder() {
        let quota = QuotaBuilder::new()
            .max_requests(100)
            .window(Duration::from_secs(60))
            .burst(150)
            .build()
            .unwrap();

        assert_eq!(quota.max_requests(), 100);
        assert_eq!(quota.window(), Duration::from_secs(60));
        assert_eq!(quota.effective_burst(), 150);
    }

    #[test]
    fn test_quota_builder_missing_fields() {
        let result = QuotaBuilder::new()
            .max_requests(100)
            .build();
        assert!(result.is_err());

        let result = QuotaBuilder::new()
            .window(Duration::from_secs(60))
            .build();
        assert!(result.is_err());
    }

    #[test]
    #[should_panic]
    fn test_quota_zero_requests_panics() {
        Quota::new(0, Duration::from_secs(60));
    }

    #[test]
    #[should_panic]
    fn test_quota_zero_window_panics() {
        Quota::new(100, Duration::ZERO);
    }
}
