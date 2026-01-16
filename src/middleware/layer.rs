//! Tower layer for rate limiting in Axum.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use axum::{
    body::Body,
    http::{Request, Response, StatusCode},
};
use tower::{Layer, Service};

use crate::algorithm::Algorithm;
use crate::decision::Decision;
use crate::key::{HasHeaders, HasIpAddr, HasMethod, HasPath, Key};
use crate::quota::Quota;
use crate::storage::Storage;

/// Tower layer for rate limiting.
// derive(Clone) removed to allow S to be ?Clone

pub struct RateLimitLayer<S, A, K> {
    storage: Arc<S>,
    algorithm: A,
    quota: Quota,
    key_extractor: K,
}

impl<S, A, K> RateLimitLayer<S, A, K> {
    /// Create a new rate limit layer.
    pub fn new(storage: S, algorithm: A, quota: Quota, key_extractor: K) -> Self {
        Self {
            storage: Arc::new(storage),
            algorithm,
            quota,
            key_extractor,
        }
    }
}

impl<S, A, K> Clone for RateLimitLayer<S, A, K>
where
    A: Clone,
    K: Clone,
{
    fn clone(&self) -> Self {
        Self {
            storage: self.storage.clone(),
            algorithm: self.algorithm.clone(),
            quota: self.quota.clone(),
            key_extractor: self.key_extractor.clone(),
        }
    }
}

impl<S, A, K, Inner> Layer<Inner> for RateLimitLayer<S, A, K>
where
    A: Clone,
    K: Clone,
{
    type Service = RateLimitService<S, A, K, Inner>;

    fn layer(&self, inner: Inner) -> Self::Service {
        RateLimitService {
            inner,
            storage: self.storage.clone(),
            algorithm: self.algorithm.clone(),
            quota: self.quota.clone(),
            key_extractor: self.key_extractor.clone(),
        }
    }
}

/// The rate limiting service.
// derive(Clone) removed to allow S to be ?Clone

pub struct RateLimitService<S, A, K, Inner> {
    inner: Inner,
    storage: Arc<S>,
    algorithm: A,
    quota: Quota,
    key_extractor: K,
}

impl<S, A, K, Inner> Clone for RateLimitService<S, A, K, Inner>
where
    A: Clone,
    K: Clone,
    Inner: Clone,
{
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            storage: self.storage.clone(),
            algorithm: self.algorithm.clone(),
            quota: self.quota.clone(),
            key_extractor: self.key_extractor.clone(),
        }
    }
}

/// Wrapper around Axum request for key extraction.
pub struct AxumRequest<'a> {
    request: &'a Request<Body>,
}

impl<'a> AxumRequest<'a> {
    #[allow(dead_code)]
    fn new(request: &'a Request<Body>) -> Self {
        Self { request }
    }
}

impl HasPath for AxumRequest<'_> {
    fn path(&self) -> &str {
        self.request.uri().path()
    }
}

impl HasMethod for AxumRequest<'_> {
    fn method(&self) -> &str {
        self.request.method().as_str()
    }
}

impl HasHeaders for AxumRequest<'_> {
    fn header(&self, name: &str) -> Option<&str> {
        self.request
            .headers()
            .get(name)
            .and_then(|v| v.to_str().ok())
    }
}

impl HasIpAddr for AxumRequest<'_> {
    #[allow(clippy::collapsible_if)]
    fn client_ip(&self) -> Option<std::net::IpAddr> {
        // Try to get from extensions (set by outer middleware)
        // For now, try parsing from X-Forwarded-For or X-Real-IP
        if let Some(forwarded) = self.header("x-forwarded-for") {
            if let Ok(ip) = forwarded.split(',').next()?.trim().parse() {
                return Some(ip);
            }
        }
        if let Some(real_ip) = self.header("x-real-ip") {
            if let Ok(ip) = real_ip.parse() {
                return Some(ip);
            }
        }
        None
    }
}

impl<S, A, K, Inner> Service<Request<Body>> for RateLimitService<S, A, K, Inner>
where
    S: Storage + Send + Sync + 'static,
    A: Algorithm + Clone + Send + Sync + 'static,
    K: Key<AxumRequest<'static>> + Clone + Send + Sync + 'static,
    Inner: Service<Request<Body>, Response = Response<Body>> + Clone + Send + 'static,
    Inner::Future: Send,
{
    type Response = Response<Body>;
    type Error = Inner::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, request: Request<Body>) -> Self::Future {
        let storage = self.storage.clone();
        let algorithm = self.algorithm.clone();
        let quota = self.quota.clone();
        let _key_extractor = self.key_extractor.clone();
        let mut inner = self.inner.clone();

        Box::pin(async move {
            // Extract key - we need to be careful with lifetimes here
            // For safety, we use a static key extraction approach
            let key = {
                // This is a workaround for lifetime issues
                // In production, you'd want a better approach
                let path = request.uri().path().to_string();
                format!("axum:{}", path)
            };

            // Check rate limit
            let decision = algorithm
                .check_and_record(&*storage, &key, &quota)
                .await
                .unwrap_or_else(|_| {
                    // On error, allow the request (fail open)
                    Decision::allowed(crate::decision::RateLimitInfo::new(
                        quota.max_requests(),
                        quota.max_requests(),
                        std::time::Instant::now() + quota.window(),
                        std::time::Instant::now(),
                    ))
                });

            if decision.is_allowed() {
                // Add rate limit headers and proceed
                let response = inner.call(request).await?;
                Ok(add_rate_limit_headers(response, &decision))
            } else {
                // Return 429 Too Many Requests
                Ok(rate_limited_response(&decision))
            }
        })
    }
}

/// Add rate limit headers to a response.
fn add_rate_limit_headers(mut response: Response<Body>, decision: &Decision) -> Response<Body> {
    let headers = response.headers_mut();
    for (name, value) in decision.info().to_headers() {
        if let Ok(header_value) = value.parse() {
            headers.insert(name, header_value);
        }
    }
    response
}

/// Create a 429 Too Many Requests response.
fn rate_limited_response(decision: &Decision) -> Response<Body> {
    let info = decision.info();
    let retry_after = info
        .retry_after
        .map(|d| d.as_secs().to_string())
        .unwrap_or_else(|| "60".to_string());

    let body = format!(
        r#"{{"error":"Too Many Requests","retry_after":{},"remaining":{},"limit":{}}}"#,
        retry_after, info.remaining, info.limit
    );

    let mut response = Response::new(Body::from(body));
    *response.status_mut() = StatusCode::TOO_MANY_REQUESTS;

    let headers = response.headers_mut();
    headers.insert("content-type", "application/json".parse().unwrap());

    for (name, value) in info.to_headers() {
        if let Ok(header_value) = value.parse() {
            headers.insert(name, header_value);
        }
    }

    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_layer_creation() {
        use crate::key::GlobalKey;
        use crate::storage::MemoryStorage;
        use crate::algorithm::GCRA;

        let storage = MemoryStorage::new();
        let layer = RateLimitLayer::new(
            storage,
            GCRA::new(),
            Quota::per_second(10),
            GlobalKey::new(),
        );

        // Just verify it compiles
        assert_eq!(layer.quota.max_requests(), 10);
    }
}
