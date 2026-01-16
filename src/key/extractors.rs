//! Pre-built key extractors for common patterns.
//!
//! These extractors are generic and can work with any request type
//! that provides the necessary data through traits.

use std::net::IpAddr;

use crate::key::Key;

// ============================================================================
// Request Info Traits
// ============================================================================

/// Trait for requests that have an IP address.
pub trait HasIpAddr {
    /// Get the client IP address.
    fn client_ip(&self) -> Option<IpAddr>;
}

/// Trait for requests that have a path.
pub trait HasPath {
    /// Get the request path.
    fn path(&self) -> &str;
}

/// Trait for requests that have a method.
pub trait HasMethod {
    /// Get the request method (GET, POST, etc).
    fn method(&self) -> &str;
}

/// Trait for requests that have headers.
pub trait HasHeaders {
    /// Get a header value by name.
    fn header(&self, name: &str) -> Option<&str>;
}

// ============================================================================
// IP-based Extractors
// ============================================================================

/// Extract key from client IP address.
#[derive(Debug, Clone, Default)]
pub struct IpKey {
    /// Header to check for real IP (e.g., X-Forwarded-For).
    real_ip_header: Option<&'static str>,
}

impl IpKey {
    /// Create a new IP key extractor.
    pub fn new() -> Self {
        Self::default()
    }

    /// Use X-Forwarded-For header to get real IP behind proxy.
    pub fn with_forwarded_for() -> Self {
        Self {
            real_ip_header: Some("x-forwarded-for"),
        }
    }

    /// Use X-Real-IP header.
    pub fn with_real_ip() -> Self {
        Self {
            real_ip_header: Some("x-real-ip"),
        }
    }

    /// Use a custom header for real IP.
    pub fn with_header(header: &'static str) -> Self {
        Self {
            real_ip_header: Some(header),
        }
    }
}

impl<R> Key<R> for IpKey
where
    R: HasIpAddr + HasHeaders,
{
    fn extract(&self, request: &R) -> Option<String> {
        // Try real IP header first if configured
        if let Some(header) = self.real_ip_header {
            if let Some(value) = request.header(header) {
                // X-Forwarded-For might have multiple IPs, take the first
                let ip = value.split(',').next()?.trim();
                if !ip.is_empty() {
                    return Some(format!("ip:{}", ip));
                }
            }
        }

        // Fall back to direct IP
        request.client_ip().map(|ip| format!("ip:{}", ip))
    }

    fn name(&self) -> &'static str {
        "ip"
    }
}

// ============================================================================
// Path-based Extractors
// ============================================================================

/// Extract key from request path.
#[derive(Debug, Clone, Default)]
pub struct PathKey;

impl PathKey {
    /// Create a new path key extractor.
    pub fn new() -> Self {
        Self
    }
}

impl<R: HasPath> Key<R> for PathKey {
    fn extract(&self, request: &R) -> Option<String> {
        Some(format!("path:{}", request.path()))
    }

    fn name(&self) -> &'static str {
        "path"
    }
}

/// Extract key from the first N segments of the path.
#[derive(Debug, Clone)]
pub struct PathPrefixKey {
    segments: usize,
}

impl PathPrefixKey {
    /// Create a path prefix key that uses the first N segments.
    pub fn new(segments: usize) -> Self {
        Self { segments }
    }
}

impl<R: HasPath> Key<R> for PathPrefixKey {
    fn extract(&self, request: &R) -> Option<String> {
        let path = request.path();
        let prefix: String = path
            .split('/')
            .filter(|s| !s.is_empty())
            .take(self.segments)
            .collect::<Vec<_>>()
            .join("/");
        Some(format!("path:/{}", prefix))
    }

    fn name(&self) -> &'static str {
        "path_prefix"
    }
}

// ============================================================================
// Header-based Extractors
// ============================================================================

/// Extract key from a specific header.
#[derive(Debug, Clone)]
pub struct HeaderKey {
    header_name: &'static str,
}

impl HeaderKey {
    /// Create a new header key extractor.
    pub fn new(header_name: &'static str) -> Self {
        Self { header_name }
    }

    /// Extract from Authorization header.
    pub fn authorization() -> Self {
        Self::new("authorization")
    }

    /// Extract from X-API-Key header.
    pub fn api_key() -> Self {
        Self::new("x-api-key")
    }

    /// Extract from User-Agent header.
    pub fn user_agent() -> Self {
        Self::new("user-agent")
    }
}

impl<R: HasHeaders> Key<R> for HeaderKey {
    fn extract(&self, request: &R) -> Option<String> {
        request
            .header(self.header_name)
            .map(|v| format!("header:{}:{}", self.header_name, v))
    }

    fn name(&self) -> &'static str {
        "header"
    }
}

// ============================================================================
// Method-based Extractors
// ============================================================================

/// Extract key from HTTP method.
#[derive(Debug, Clone, Default)]
pub struct MethodKey;

impl MethodKey {
    /// Create a new method key extractor.
    pub fn new() -> Self {
        Self
    }
}

impl<R: HasMethod> Key<R> for MethodKey {
    fn extract(&self, request: &R) -> Option<String> {
        Some(format!("method:{}", request.method()))
    }

    fn name(&self) -> &'static str {
        "method"
    }
}

// ============================================================================
// Route-based Extractors
// ============================================================================

/// Extract key from matched route pattern.
///
/// Unlike `PathKey`, this uses the route pattern (e.g., `/users/{id}`)
/// rather than the actual path (e.g., `/users/123`).
#[derive(Debug, Clone)]
pub struct RouteKey {
    route_pattern: String,
}

impl RouteKey {
    /// Create a new route key with the given pattern.
    pub fn new(pattern: impl Into<String>) -> Self {
        Self {
            route_pattern: pattern.into(),
        }
    }
}

impl<R> Key<R> for RouteKey {
    fn extract(&self, _request: &R) -> Option<String> {
        Some(format!("route:{}", self.route_pattern))
    }

    fn name(&self) -> &'static str {
        "route"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::net::IpAddr;

    // Mock request for testing
    #[derive(Default)]
    struct MockRequest {
        ip: Option<IpAddr>,
        path: String,
        method: String,
        headers: HashMap<String, String>,
    }

    impl HasIpAddr for MockRequest {
        fn client_ip(&self) -> Option<IpAddr> {
            self.ip
        }
    }

    impl HasPath for MockRequest {
        fn path(&self) -> &str {
            &self.path
        }
    }

    impl HasMethod for MockRequest {
        fn method(&self) -> &str {
            &self.method
        }
    }

    impl HasHeaders for MockRequest {
        fn header(&self, name: &str) -> Option<&str> {
            self.headers.get(name).map(|s| s.as_str())
        }
    }

    #[test]
    fn test_ip_key() {
        let key = IpKey::new();
        let mut req = MockRequest::default();
        req.ip = Some("192.168.1.1".parse().unwrap());

        assert_eq!(key.extract(&req), Some("ip:192.168.1.1".to_string()));
    }

    #[test]
    fn test_ip_key_with_forwarded_for() {
        let key = IpKey::with_forwarded_for();
        let mut req = MockRequest::default();
        req.ip = Some("10.0.0.1".parse().unwrap());
        req.headers
            .insert("x-forwarded-for".into(), "203.0.113.50, 70.41.3.18".into());

        // Should use the first IP from X-Forwarded-For
        assert_eq!(key.extract(&req), Some("ip:203.0.113.50".to_string()));
    }

    #[test]
    fn test_path_key() {
        let key = PathKey::new();
        let mut req = MockRequest::default();
        req.path = "/api/users/123".into();

        assert_eq!(key.extract(&req), Some("path:/api/users/123".to_string()));
    }

    #[test]
    fn test_path_prefix_key() {
        let key = PathPrefixKey::new(2);
        let mut req = MockRequest::default();
        req.path = "/api/users/123/posts".into();

        assert_eq!(key.extract(&req), Some("path:/api/users".to_string()));
    }

    #[test]
    fn test_header_key() {
        let key = HeaderKey::api_key();
        let mut req = MockRequest::default();
        req.headers.insert("x-api-key".into(), "secret-key".into());

        assert_eq!(
            key.extract(&req),
            Some("header:x-api-key:secret-key".to_string())
        );
    }

    #[test]
    fn test_method_key() {
        let key = MethodKey::new();
        let mut req = MockRequest::default();
        req.method = "POST".into();

        assert_eq!(key.extract(&req), Some("method:POST".to_string()));
    }

    #[test]
    fn test_route_key() {
        let key = RouteKey::new("/users/{id}");
        let req = MockRequest::default();

        assert_eq!(key.extract(&req), Some("route:/users/{id}".to_string()));
    }
}
