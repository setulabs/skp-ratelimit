//! Storage backend trait and implementations.
//!
//! This module defines the `Storage` trait that all storage backends must implement,
//! along with built-in implementations for in-memory and Redis storage.

mod entry;
#[cfg(feature = "memory")]
mod memory_gc;
#[cfg(feature = "redis")]
mod redis_cluster;

pub use entry::StorageEntry;

#[cfg(feature = "memory")]
pub use memory_gc::{GcConfig, GcInterval, MemoryStorage};

// RedisStorage with connection pooling
#[cfg(feature = "redis")]
pub use redis_cluster::{RedisConfig, RedisStorage};

use std::future::Future;
use std::time::Duration;

use crate::error::Result;

/// Storage backend trait for rate limiting state.
///
/// All storage operations are async to support both local and distributed backends.
/// Implementations must be thread-safe (`Send + Sync`).
///
/// # Required Operations
///
/// - `get`: Retrieve an entry by key
/// - `set`: Store an entry with a TTL
/// - `delete`: Remove an entry
/// - `increment`: Atomically increment a counter
/// - `execute_atomic`: Execute an atomic read-modify-write operation
///
/// # Example
///
/// ```ignore
/// use oc_ratelimit_advanced::storage::{Storage, StorageEntry};
///
/// async fn example<S: Storage>(storage: &S) {
///     // Store an entry
///     let entry = StorageEntry::new(1, current_time_ms());
///     storage.set("key", entry, Duration::from_secs(60)).await?;
///
///     // Retrieve it
///     if let Some(entry) = storage.get("key").await? {
///         println!("Count: {}", entry.count);
///     }
/// }
/// ```
pub trait Storage: Send + Sync + 'static {
    /// Get an entry by key.
    ///
    /// Returns `None` if the key doesn't exist or has expired.
    fn get(&self, key: &str) -> impl Future<Output = Result<Option<StorageEntry>>> + Send;

    /// Set an entry with a TTL.
    ///
    /// The entry will be automatically removed after the TTL expires.
    fn set(
        &self,
        key: &str,
        entry: StorageEntry,
        ttl: Duration,
    ) -> impl Future<Output = Result<()>> + Send;

    /// Delete an entry.
    ///
    /// Returns success even if the key didn't exist.
    fn delete(&self, key: &str) -> impl Future<Output = Result<()>> + Send;

    /// Atomically increment a counter.
    ///
    /// If the key doesn't exist or belongs to a different window, it will be
    /// created with count 1 and the given window_start.
    ///
    /// Returns the count AFTER incrementing.
    fn increment(
        &self,
        key: &str,
        delta: u64,
        window_start: u64,
        ttl: Duration,
    ) -> impl Future<Output = Result<u64>> + Send;

    /// Execute an atomic read-modify-write operation.
    ///
    /// The operation function receives the current entry (if any) and returns
    /// the new entry to store along with a result value.
    ///
    /// This is the most flexible atomic operation and can be used to implement
    /// any algorithm's state updates.
    fn execute_atomic<F, T>(
        &self,
        key: &str,
        ttl: Duration,
        operation: F,
    ) -> impl Future<Output = Result<T>> + Send
    where
        F: FnOnce(Option<StorageEntry>) -> (StorageEntry, T) + Send,
        T: Send;

    /// Compare-and-swap operation.
    ///
    /// If the current value matches `expected`, it will be replaced with `new`.
    /// Returns `true` if the swap succeeded.
    fn compare_and_swap(
        &self,
        key: &str,
        expected: Option<&StorageEntry>,
        new: StorageEntry,
        ttl: Duration,
    ) -> impl Future<Output = Result<bool>> + Send;
}

impl<S: Storage + ?Sized> Storage for std::sync::Arc<S> {
    async fn get(&self, key: &str) -> Result<Option<StorageEntry>> {
        (**self).get(key).await
    }

    async fn set(&self, key: &str, entry: StorageEntry, ttl: Duration) -> Result<()> {
        (**self).set(key, entry, ttl).await
    }

    async fn delete(&self, key: &str) -> Result<()> {
        (**self).delete(key).await
    }

    async fn increment(
        &self,
        key: &str,
        delta: u64,
        window_start: u64,
        ttl: Duration,
    ) -> Result<u64> {
        (**self).increment(key, delta, window_start, ttl).await
    }

    async fn execute_atomic<F, T>(&self, key: &str, ttl: Duration, operation: F) -> Result<T>
    where
        F: FnOnce(Option<StorageEntry>) -> (StorageEntry, T) + Send,
        T: Send,
    {
        (**self).execute_atomic(key, ttl, operation).await
    }

    async fn compare_and_swap(
        &self,
        key: &str,
        expected: Option<&StorageEntry>,
        new: StorageEntry,
        ttl: Duration,
    ) -> Result<bool> {
        (**self).compare_and_swap(key, expected, new, ttl).await
    }
}

impl<S: Storage + ?Sized> Storage for Box<S> {
    async fn get(&self, key: &str) -> Result<Option<StorageEntry>> {
        (**self).get(key).await
    }

    async fn set(&self, key: &str, entry: StorageEntry, ttl: Duration) -> Result<()> {
        (**self).set(key, entry, ttl).await
    }

    async fn delete(&self, key: &str) -> Result<()> {
        (**self).delete(key).await
    }

    async fn increment(
        &self,
        key: &str,
        delta: u64,
        window_start: u64,
        ttl: Duration,
    ) -> Result<u64> {
        (**self).increment(key, delta, window_start, ttl).await
    }

    async fn execute_atomic<F, T>(&self, key: &str, ttl: Duration, operation: F) -> Result<T>
    where
        F: FnOnce(Option<StorageEntry>) -> (StorageEntry, T) + Send,
        T: Send,
    {
        (**self).execute_atomic(key, ttl, operation).await
    }

    async fn compare_and_swap(
        &self,
        key: &str,
        expected: Option<&StorageEntry>,
        new: StorageEntry,
        ttl: Duration,
    ) -> Result<bool> {
        (**self).compare_and_swap(key, expected, new, ttl).await
    }
}

/// Get the current timestamp in milliseconds since Unix epoch.
pub fn current_timestamp_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards")
        .as_millis() as u64
}
