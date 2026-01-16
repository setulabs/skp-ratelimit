//! Rate limit manager for per-route configuration.
//!
//! The `RateLimitManager` allows you to configure different rate limits
//! for different routes or patterns, with optional default fallback.
//!
//! # Example
//!
//! ```ignore
//! use oc_ratelimit_advanced::{RateLimitManager, Quota, GCRA, MemoryStorage};
//!
//! let storage = MemoryStorage::new();
//! let manager = RateLimitManager::builder()
//!     .default_quota(Quota::per_second(10))
//!     .route("/api/search", Quota::per_minute(30))
//!     .route("/api/auth/login", Quota::per_minute(5))
//!     .route_pattern("/api/users/*", Quota::per_second(20))
//!     .build(GCRA::new(), storage);
//! ```

use std::collections::HashMap;
use std::sync::Arc;

use crate::algorithm::Algorithm;
use crate::decision::Decision;
use crate::error::Result;
use crate::key::Key;
use crate::quota::Quota;
use crate::storage::Storage;

/// A rate limit configuration for a specific route.
#[derive(Debug, Clone)]
pub struct RouteConfig {
    /// The quota for this route.
    pub quota: Quota,
    /// Optional custom key suffix.
    pub key_suffix: Option<String>,
}

impl RouteConfig {
    /// Create a new route config with the given quota.
    pub fn new(quota: Quota) -> Self {
        Self {
            quota,
            key_suffix: None,
        }
    }

    /// Add a custom key suffix.
    pub fn with_key_suffix(mut self, suffix: impl Into<String>) -> Self {
        self.key_suffix = Some(suffix.into());
        self
    }
}

impl From<Quota> for RouteConfig {
    fn from(quota: Quota) -> Self {
        Self::new(quota)
    }
}

/// Manager for per-route rate limiting.
///
/// This provides a centralized way to configure different rate limits
/// for different routes or patterns.
pub struct RateLimitManager<A, S, K> {
    algorithm: A,
    storage: Arc<S>,
    key_extractor: K,
    default_quota: Option<Quota>,
    routes: HashMap<String, RouteConfig>,
    patterns: Vec<(String, RouteConfig)>,
}

impl<A, S, K> RateLimitManager<A, S, K>
where
    A: Algorithm,
    S: Storage,
{
    /// Create a new rate limit manager builder.
    pub fn builder() -> RateLimitManagerBuilder<K> {
        RateLimitManagerBuilder::new()
    }

    /// Check and record a request.
    pub async fn check_and_record<R>(&self, path: &str, request: &R) -> Result<Decision>
    where
        K: Key<R>,
    {
        let config = self.get_config(path);

        let Some(quota) = config.map(|c| &c.quota).or(self.default_quota.as_ref()) else {
            // No quota configured, allow the request
            return Ok(Decision::allowed(crate::decision::RateLimitInfo::new(
                u64::MAX,
                u64::MAX,
                std::time::Instant::now() + std::time::Duration::from_secs(3600),
                std::time::Instant::now(),
            )));
        };

        // Build the key
        let base_key = self.key_extractor.extract(request).unwrap_or_else(|| "unknown".to_string());
        let key = if let Some(suffix) = config.and_then(|c| c.key_suffix.as_ref()) {
            format!("{}:{}", base_key, suffix)
        } else {
            format!("{}:{}", base_key, path)
        };

        self.algorithm
            .check_and_record(&*self.storage, &key, quota)
            .await
    }

    /// Check without recording.
    pub async fn check<R>(&self, path: &str, request: &R) -> Result<Decision>
    where
        K: Key<R>,
    {
        let config = self.get_config(path);

        let Some(quota) = config.map(|c| &c.quota).or(self.default_quota.as_ref()) else {
            return Ok(Decision::allowed(crate::decision::RateLimitInfo::new(
                u64::MAX,
                u64::MAX,
                std::time::Instant::now() + std::time::Duration::from_secs(3600),
                std::time::Instant::now(),
            )));
        };

        let base_key = self.key_extractor.extract(request).unwrap_or_else(|| "unknown".to_string());
        let key = if let Some(suffix) = config.and_then(|c| c.key_suffix.as_ref()) {
            format!("{}:{}", base_key, suffix)
        } else {
            format!("{}:{}", base_key, path)
        };

        self.algorithm.check(&*self.storage, &key, quota).await
    }

    /// Get the configuration for a path.
    fn get_config(&self, path: &str) -> Option<&RouteConfig> {
        // Exact match first
        if let Some(config) = self.routes.get(path) {
            return Some(config);
        }

        // Pattern matching
        for (pattern, config) in &self.patterns {
            if pattern_matches(pattern, path) {
                return Some(config);
            }
        }

        None
    }

    /// Reset rate limit for a specific key.
    pub async fn reset(&self, key: &str) -> Result<()> {
        self.algorithm.reset(&*self.storage, key).await
    }
}

/// Check if a pattern matches a path.
///
/// Simple glob-style matching:
/// - `*` matches any single path segment
/// - `**` matches any number of segments
fn pattern_matches(pattern: &str, path: &str) -> bool {
    let pattern_parts: Vec<&str> = pattern.split('/').filter(|s| !s.is_empty()).collect();
    let path_parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();

    let mut pi = 0; // pattern index
    let mut pa = 0; // path index

    while pi < pattern_parts.len() && pa < path_parts.len() {
        let p = pattern_parts[pi];

        if p == "**" {
            // ** matches rest of path
            return true;
        } else if p == "*" {
            // * matches single segment
            pi += 1;
            pa += 1;
        } else if p == path_parts[pa] {
            // Exact match
            pi += 1;
            pa += 1;
        } else {
            return false;
        }
    }

    // Pattern exhausted - check if path is also exhausted
    pi == pattern_parts.len() && pa == path_parts.len()
}

/// Builder for RateLimitManager.
pub struct RateLimitManagerBuilder<K> {
    default_quota: Option<Quota>,
    routes: HashMap<String, RouteConfig>,
    patterns: Vec<(String, RouteConfig)>,
    key_extractor: Option<K>,
}

impl<K> Default for RateLimitManagerBuilder<K> {
    fn default() -> Self {
        Self::new()
    }
}

impl<K> RateLimitManagerBuilder<K> {
    /// Create a new builder.
    pub fn new() -> Self {
        Self {
            default_quota: None,
            routes: HashMap::new(),
            patterns: Vec::new(),
            key_extractor: None,
        }
    }

    /// Set the default quota for routes without specific configuration.
    pub fn default_quota(mut self, quota: Quota) -> Self {
        self.default_quota = Some(quota);
        self
    }

    /// Add a rate limit for a specific route.
    pub fn route(mut self, path: impl Into<String>, config: impl Into<RouteConfig>) -> Self {
        self.routes.insert(path.into(), config.into());
        self
    }

    /// Add a rate limit for a route pattern.
    ///
    /// Patterns support `*` for single segment and `**` for multiple segments.
    pub fn route_pattern(
        mut self,
        pattern: impl Into<String>,
        config: impl Into<RouteConfig>,
    ) -> Self {
        self.patterns.push((pattern.into(), config.into()));
        self
    }

    /// Set the key extractor.
    pub fn key_extractor(mut self, extractor: K) -> Self {
        self.key_extractor = Some(extractor);
        self
    }

    /// Build the manager with the given algorithm and storage.
    pub fn build<A, S>(self, algorithm: A, storage: S) -> RateLimitManager<A, S, K>
    where
        K: Default,
    {
        RateLimitManager {
            algorithm,
            storage: Arc::new(storage),
            key_extractor: self.key_extractor.unwrap_or_default(),
            default_quota: self.default_quota,
            routes: self.routes,
            patterns: self.patterns,
        }
    }

    /// Build the manager with a specific key extractor.
    pub fn build_with_key<A, S>(
        self,
        algorithm: A,
        storage: S,
        key_extractor: K,
    ) -> RateLimitManager<A, S, K> {
        RateLimitManager {
            algorithm,
            storage: Arc::new(storage),
            key_extractor,
            default_quota: self.default_quota,
            routes: self.routes,
            patterns: self.patterns,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pattern_matches_exact() {
        assert!(pattern_matches("/api/users", "/api/users"));
        assert!(!pattern_matches("/api/users", "/api/posts"));
    }

    #[test]
    fn test_pattern_matches_single_wildcard() {
        assert!(pattern_matches("/api/*/posts", "/api/users/posts"));
        assert!(pattern_matches("/api/*/posts", "/api/admins/posts"));
        assert!(!pattern_matches("/api/*/posts", "/api/users/comments"));
    }

    #[test]
    fn test_pattern_matches_double_wildcard() {
        assert!(pattern_matches("/api/**", "/api/users"));
        assert!(pattern_matches("/api/**", "/api/users/123/posts"));
        assert!(!pattern_matches("/api/**", "/v2/api/users"));
    }

    #[test]
    fn test_route_config_from_quota() {
        let config: RouteConfig = Quota::per_minute(60).into();
        assert_eq!(config.quota.max_requests(), 60);
        assert!(config.key_suffix.is_none());
    }
}
