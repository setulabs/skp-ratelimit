//! Axum middleware for rate limiting.
//!
//! Provides Tower-compatible layers for integrating rate limiting into Axum applications.
//!
//! # Example
//!
//! ```ignore
//! use axum::{Router, routing::get};
//! use oc_ratelimit_advanced::{
//!     middleware::RateLimitLayer,
//!     GCRA, Quota, MemoryStorage,
//!     key::IpKey,
//! };
//!
//! let storage = MemoryStorage::new();
//!
//! let app = Router::new()
//!     .route("/api/data", get(handler))
//!     .layer(RateLimitLayer::new(
//!         storage,
//!         GCRA::new(),
//!         Quota::per_second(10),
//!         IpKey::new(),
//!     ));
//! ```

mod layer;

#[cfg(feature = "actix")]
pub mod actix;

pub use layer::RateLimitLayer;
