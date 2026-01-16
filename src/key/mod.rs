//! Key extraction for rate limiting.
//!
//! This module provides the `Key` trait for extracting rate limiting keys from
//! HTTP requests, along with pre-built extractors for common patterns.
//!
//! # Overview
//!
//! Rate limiting keys determine how requests are grouped together. For example:
//! - Limit by IP address: all requests from the same IP share a quota
//! - Limit by user ID: all requests from the same user share a quota
//! - Limit by route: different quotas for different endpoints
//!
//! # Example
//!
//! ```ignore
//! use skp_ratelimit::key::{Key, IpKey, CompositeKey};
//!
//! // Simple IP-based key
//! let ip_key = IpKey::new();
//!
//! // Composite key: IP + path
//! let composite = CompositeKey::new(IpKey::new(), PathKey::new());
//! ```

mod composite;
mod extractors;

pub use composite::{CompositeKey, CompositeKey3, EitherKey, OptionalKey};
pub use extractors::*;

/// Trait for extracting rate limiting keys from requests.
///
/// The key determines how requests are grouped for rate limiting purposes.
/// Return `None` if the key cannot be extracted (e.g., missing header),
/// which typically results in the request being allowed.
///
/// # Type Parameters
///
/// - `R`: The request type (e.g., `axum::extract::Request`, `actix_web::HttpRequest`)
pub trait Key<R>: Send + Sync + 'static {
    /// Extract a rate limiting key from the request.
    ///
    /// Returns `None` if the key cannot be extracted, which typically
    /// means the request should be allowed (fail open).
    fn extract(&self, request: &R) -> Option<String>;

    /// Get the key name for logging/metrics.
    fn name(&self) -> &'static str;
}

/// A constant key that applies the same limit to all requests.
#[derive(Debug, Clone, Default)]
pub struct GlobalKey;

impl GlobalKey {
    /// Create a new global key.
    pub fn new() -> Self {
        Self
    }
}

impl<R> Key<R> for GlobalKey {
    fn extract(&self, _request: &R) -> Option<String> {
        Some("global".to_string())
    }

    fn name(&self) -> &'static str {
        "global"
    }
}

/// A key that extracts a specific field from the request.
///
/// This is a generic extractor that can be configured with a closure.
#[derive(Clone)]
pub struct FnKey<F> {
    extractor: F,
    name: &'static str,
}

impl<F> std::fmt::Debug for FnKey<F> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FnKey").field("name", &self.name).finish()
    }
}

impl<F> FnKey<F> {
    /// Create a new function-based key extractor.
    pub fn new(name: &'static str, extractor: F) -> Self {
        Self { extractor, name }
    }
}

impl<R, F> Key<R> for FnKey<F>
where
    F: Fn(&R) -> Option<String> + Send + Sync + 'static,
{
    fn extract(&self, request: &R) -> Option<String> {
        (self.extractor)(request)
    }

    fn name(&self) -> &'static str {
        self.name
    }
}

/// A key that always returns a static value.
#[derive(Debug, Clone)]
pub struct StaticKey {
    key: String,
}

impl StaticKey {
    /// Create a new static key.
    pub fn new(key: impl Into<String>) -> Self {
        Self { key: key.into() }
    }
}

impl<R> Key<R> for StaticKey {
    fn extract(&self, _request: &R) -> Option<String> {
        Some(self.key.clone())
    }

    fn name(&self) -> &'static str {
        "static"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_global_key() {
        let key = GlobalKey::new();
        let request = ();
        assert_eq!(Key::<()>::extract(&key, &request), Some("global".to_string()));
        assert_eq!(Key::<()>::name(&key), "global");
    }

    #[test]
    fn test_static_key() {
        let key = StaticKey::new("my-key");
        let request = ();
        assert_eq!(key.extract(&request), Some("my-key".to_string()));
    }

    #[test]
    fn test_fn_key() {
        let key: FnKey<fn(&i32) -> Option<String>> = FnKey::new("custom", |_: &i32| Some("from-fn".to_string()));
        assert_eq!(key.extract(&42), Some("from-fn".to_string()));
        assert_eq!(key.name(), "custom");
    }
}
