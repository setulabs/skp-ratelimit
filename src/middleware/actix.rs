//! Actix-web middleware for rate limiting.
//!
//! Provides middleware for integrating rate limiting into Actix-web applications.
//!
//! # Example
//!
//! ```ignore
//! use actix_web::{web, App, HttpServer};
//! use oc_ratelimit_advanced::{
//!     middleware::actix::RateLimiter,
//!     GCRA, Quota, MemoryStorage,
//! };
//!
//! #[actix_web::main]
//! async fn main() {
//!     let storage = MemoryStorage::new();
//!
//!     HttpServer::new(move || {
//!         App::new()
//!             .wrap(RateLimiter::new(storage.clone(), GCRA::new(), Quota::per_second(10)))
//!             .route("/api/data", web::get().to(handler))
//!     })
//!     .bind("127.0.0.1:8080")?
//!     .run()
//!     .await
//! }
//! ```

use std::future::{ready, Future, Ready};
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use actix_service::{Service, Transform};
use actix_web::{
    body::EitherBody,
    dev::{ServiceRequest, ServiceResponse},
    http::StatusCode,
    Error, HttpResponse,
};

use crate::algorithm::Algorithm;
use crate::decision::Decision;
use crate::quota::Quota;
use crate::storage::Storage;

/// Rate limiter middleware for Actix-web.
pub struct RateLimiter<S, A> {
    storage: Arc<S>,
    algorithm: A,
    quota: Quota,
}

impl<S, A> RateLimiter<S, A>
where
    S: Storage + Clone,
    A: Algorithm + Clone,
{
    /// Create a new rate limiter middleware.
    pub fn new(storage: S, algorithm: A, quota: Quota) -> Self {
        Self {
            storage: Arc::new(storage),
            algorithm,
            quota,
        }
    }
}

impl<S, A> Clone for RateLimiter<S, A>
where
    A: Clone,
{
    fn clone(&self) -> Self {
        Self {
            storage: self.storage.clone(),
            algorithm: self.algorithm.clone(),
            quota: self.quota.clone(),
        }
    }
}

impl<S, A, Svc, B> Transform<Svc, ServiceRequest> for RateLimiter<S, A>
where
    S: Storage + Send + Sync + 'static,
    A: Algorithm + Clone + Send + Sync + 'static,
    Svc: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    Svc::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<EitherBody<B>>;
    type Error = Error;
    type Transform = RateLimiterMiddleware<S, A, Svc>;
    type InitError = ();
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: Svc) -> Self::Future {
        ready(Ok(RateLimiterMiddleware {
            service,
            storage: self.storage.clone(),
            algorithm: self.algorithm.clone(),
            quota: self.quota.clone(),
        }))
    }
}

/// The actual middleware service.
pub struct RateLimiterMiddleware<S, A, Svc> {
    service: Svc,
    storage: Arc<S>,
    algorithm: A,
    quota: Quota,
}

impl<S, A, Svc, B> Service<ServiceRequest> for RateLimiterMiddleware<S, A, Svc>
where
    S: Storage + Send + Sync + 'static,
    A: Algorithm + Clone + Send + Sync + 'static,
    Svc: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    Svc::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<EitherBody<B>>;
    type Error = Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>>>>;

    fn poll_ready(&self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.service.poll_ready(cx)
    }

    fn call(&self, req: ServiceRequest) -> Self::Future {
        let storage = self.storage.clone();
        let algorithm = self.algorithm.clone();
        let quota = self.quota.clone();

        // Extract key from request
        let key = extract_key(&req);

        // We need to capture the service call
        let fut = self.service.call(req);

        Box::pin(async move {
            // Check rate limit
            let decision = algorithm
                .check_and_record(&*storage, &key, &quota)
                .await
                .unwrap_or_else(|_| {
                    // Fail open on errors
                    Decision::allowed(crate::decision::RateLimitInfo::new(
                        quota.max_requests(),
                        quota.max_requests(),
                        std::time::Instant::now() + quota.window(),
                        std::time::Instant::now(),
                    ))
                });

            if decision.is_denied() {
                let info = decision.info();
                let retry_after = info
                    .retry_after
                    .map(|d| d.as_secs().to_string())
                    .unwrap_or_else(|| "60".to_string());

                let body = format!(
                    r#"{{"error":"Too Many Requests","retry_after":{},"remaining":{},"limit":{}}}"#,
                    retry_after, info.remaining, info.limit
                );

                let response = HttpResponse::build(StatusCode::TOO_MANY_REQUESTS)
                    .insert_header(("Content-Type", "application/json"))
                    .insert_header(("X-RateLimit-Limit", info.limit.to_string()))
                    .insert_header(("X-RateLimit-Remaining", info.remaining.to_string()))
                    .insert_header(("X-RateLimit-Reset", info.reset_seconds().to_string()))
                    .insert_header(("Retry-After", retry_after))
                    .body(body);

                // Re-construct the request to get the ServiceResponse
                // This is a workaround since we've already consumed the request
                return Err(actix_web::error::InternalError::from_response(
                    "Rate limited",
                    response,
                )
                .into());
            }

            // Proceed with the request and add headers
            let res = fut.await?;
            Ok(res.map_into_left_body())
        })
    }
}

/// Extract a rate limiting key from the request.
fn extract_key(req: &ServiceRequest) -> String {
    // Try to get client IP from various headers
    if let Some(forwarded) = req.headers().get("x-forwarded-for") {
        if let Ok(value) = forwarded.to_str() {
            if let Some(ip) = value.split(',').next() {
                return format!("ip:{}", ip.trim());
            }
        }
    }

    if let Some(real_ip) = req.headers().get("x-real-ip") {
        if let Ok(value) = real_ip.to_str() {
            return format!("ip:{}", value);
        }
    }

    // Fall back to connection info
    if let Some(peer) = req.connection_info().peer_addr() {
        return format!("ip:{}", peer);
    }

    // Ultimate fallback
    format!("path:{}", req.path())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rate_limiter_creation() {
        use crate::algorithm::GCRA;
        use crate::storage::MemoryStorage;

        let storage = MemoryStorage::new();
        let limiter = RateLimiter::new(storage, GCRA::new(), Quota::per_second(10));

        assert_eq!(limiter.quota.max_requests(), 10);
    }
}
