//! Policy engine for advanced rate limiting decisions.
//!
//! Policies allow customizing rate limiting behavior beyond simple allow/deny:
//! - Penalty for errors (consume extra tokens on 4xx/5xx)
//! - Credits for cached responses
//! - Custom decision logic
//!
//! # Example
//!
//! ```ignore
//! use oc_ratelimit_advanced::policy::{Policy, PenaltyPolicy, CreditPolicy};
//!
//! let policy = PenaltyPolicy::new(2); // Consume 2x tokens on errors
//! ```

use crate::decision::Decision;
use crate::quota::Quota;

/// Policy for adjusting rate limit behavior.
///
/// Policies can modify the cost of requests or adjust quotas based on
/// response status or other factors.
pub trait Policy: Send + Sync + 'static {
    /// Calculate the token cost for this request.
    ///
    /// Default is 1 token per request.
    fn token_cost(&self, _quota: &Quota) -> u64 {
        1
    }

    /// Called after a response is generated.
    ///
    /// Returns the number of tokens to refund (positive) or charge additionally (negative).
    fn on_response(&self, _status_code: u16, _decision: &Decision) -> i64 {
        0
    }

    /// Get the policy name for logging.
    fn name(&self) -> &'static str;
}

/// Default policy - standard allow/deny based on quota.
#[derive(Debug, Clone, Default)]
pub struct DefaultPolicy;

impl DefaultPolicy {
    /// Create a new default policy.
    pub fn new() -> Self {
        Self
    }
}

impl Policy for DefaultPolicy {
    fn name(&self) -> &'static str {
        "default"
    }
}

/// Penalty policy - consume extra tokens on errors.
///
/// Useful to discourage clients from making failing requests.
#[derive(Debug, Clone)]
pub struct PenaltyPolicy {
    /// Multiplier for token cost on 4xx errors
    pub client_error_multiplier: u64,
    /// Multiplier for token cost on 5xx errors
    pub server_error_multiplier: u64,
}

impl PenaltyPolicy {
    /// Create a new penalty policy with the given multiplier for all errors.
    pub fn new(multiplier: u64) -> Self {
        Self {
            client_error_multiplier: multiplier,
            server_error_multiplier: multiplier,
        }
    }

    /// Set different multipliers for client vs server errors.
    pub fn with_multipliers(client_error: u64, server_error: u64) -> Self {
        Self {
            client_error_multiplier: client_error,
            server_error_multiplier: server_error,
        }
    }
}

impl Default for PenaltyPolicy {
    fn default() -> Self {
        Self::new(2)
    }
}

impl Policy for PenaltyPolicy {
    fn on_response(&self, status_code: u16, _decision: &Decision) -> i64 {
        match status_code {
            400..=499 => -((self.client_error_multiplier - 1) as i64),
            500..=599 => -((self.server_error_multiplier - 1) as i64),
            _ => 0,
        }
    }

    fn name(&self) -> &'static str {
        "penalty"
    }
}

/// Credit policy - refund tokens for cached responses.
///
/// Useful when 304 Not Modified responses should not count against limit.
#[derive(Debug, Clone)]
pub struct CreditPolicy {
    /// Refund tokens for 304 Not Modified
    pub refund_not_modified: bool,
    /// Refund tokens for 204 No Content
    pub refund_no_content: bool,
}

impl CreditPolicy {
    /// Create a new credit policy that refunds tokens for 304 responses.
    pub fn new() -> Self {
        Self {
            refund_not_modified: true,
            refund_no_content: false,
        }
    }

    /// Also refund for 204 No Content responses.
    pub fn with_no_content(mut self) -> Self {
        self.refund_no_content = true;
        self
    }
}

impl Default for CreditPolicy {
    fn default() -> Self {
        Self::new()
    }
}

impl Policy for CreditPolicy {
    fn on_response(&self, status_code: u16, _decision: &Decision) -> i64 {
        if status_code == 304 && self.refund_not_modified {
            return 1;
        }
        if status_code == 204 && self.refund_no_content {
            return 1;
        }
        0
    }

    fn name(&self) -> &'static str {
        "credit"
    }
}

/// Composite policy - chain multiple policies together.
#[derive(Default)]
pub struct CompositePolicy {
    policies: Vec<Box<dyn Policy>>,
}

impl CompositePolicy {
    /// Create a new composite policy.
    pub fn new() -> Self {
        Self {
            policies: Vec::new(),
        }
    }

    /// Add a policy to the chain.
    pub fn with<P: Policy>(mut self, policy: P) -> Self {
        self.policies.push(Box::new(policy));
        self
    }
}

impl Policy for CompositePolicy {
    fn token_cost(&self, quota: &Quota) -> u64 {
        self.policies
            .iter()
            .map(|p| p.token_cost(quota))
            .max()
            .unwrap_or(1)
    }

    fn on_response(&self, status_code: u16, decision: &Decision) -> i64 {
        self.policies
            .iter()
            .map(|p| p.on_response(status_code, decision))
            .sum()
    }

    fn name(&self) -> &'static str {
        "composite"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_policy() {
        let policy = DefaultPolicy::new();
        let quota = Quota::per_minute(100);
        assert_eq!(policy.token_cost(&quota), 1);
        assert_eq!(policy.name(), "default");
    }

    #[test]
    fn test_penalty_policy() {
        let policy = PenaltyPolicy::new(3);
        let quota = Quota::per_minute(100);
        let decision = crate::decision::Decision::allowed(
            crate::decision::RateLimitInfo::new(100, 99, std::time::Instant::now(), std::time::Instant::now()),
        );

        // 200 OK - no penalty
        assert_eq!(policy.on_response(200, &decision), 0);

        // 404 Not Found - penalty (return negative to charge more)
        assert_eq!(policy.on_response(404, &decision), -2);

        // 500 Server Error - penalty
        assert_eq!(policy.on_response(500, &decision), -2);
    }

    #[test]
    fn test_credit_policy() {
        let policy = CreditPolicy::new().with_no_content();
        let quota = Quota::per_minute(100);
        let decision = crate::decision::Decision::allowed(
            crate::decision::RateLimitInfo::new(100, 99, std::time::Instant::now(), std::time::Instant::now()),
        );

        // 304 Not Modified - refund
        assert_eq!(policy.on_response(304, &decision), 1);

        // 204 No Content - refund
        assert_eq!(policy.on_response(204, &decision), 1);

        // 200 OK - no refund
        assert_eq!(policy.on_response(200, &decision), 0);
    }

    #[test]
    fn test_composite_policy() {
        let policy = CompositePolicy::new()
            .with(PenaltyPolicy::new(2))
            .with(CreditPolicy::new());

        let decision = crate::decision::Decision::allowed(
            crate::decision::RateLimitInfo::new(100, 99, std::time::Instant::now(), std::time::Instant::now()),
        );

        // Penalty and credit sum together
        assert_eq!(policy.on_response(404, &decision), -1); // -1 from penalty
        assert_eq!(policy.on_response(304, &decision), 1); // +1 from credit
    }
}
