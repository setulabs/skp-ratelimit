//! Concurrent request limiter.
//!
//! Unlike rate limiters that limit requests over time, this limits
//! the number of simultaneous in-flight requests.

use std::sync::Arc;
use std::time::Duration;

use dashmap::DashMap;
use tokio::sync::Semaphore;

/// Concurrent request limiter.
///
/// Limits the number of simultaneous in-flight requests per key.
/// Unlike rate limiting, this tracks active requests that haven't completed yet.
///
/// # Example
///
/// ```ignore
/// use oc_ratelimit_advanced::ConcurrentLimiter;
///
/// let limiter = ConcurrentLimiter::new(10); // Max 10 concurrent requests
///
/// // Acquire a permit
/// if let Some(permit) = limiter.try_acquire("user:123") {
///     // Process request...
///     // Permit is automatically released when dropped
/// }
/// ```
pub struct ConcurrentLimiter {
    max_concurrent: u32,
    semaphores: Arc<DashMap<String, Arc<Semaphore>>>,
    counts: Arc<DashMap<String, u32>>,
}

impl std::fmt::Debug for ConcurrentLimiter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConcurrentLimiter")
            .field("max_concurrent", &self.max_concurrent)
            .field("active_keys", &self.semaphores.len())
            .finish()
    }
}

impl Clone for ConcurrentLimiter {
    fn clone(&self) -> Self {
        Self {
            max_concurrent: self.max_concurrent,
            semaphores: self.semaphores.clone(),
            counts: self.counts.clone(),
        }
    }
}

impl ConcurrentLimiter {
    /// Create a new concurrent limiter.
    pub fn new(max_concurrent: u32) -> Self {
        Self {
            max_concurrent,
            semaphores: Arc::new(DashMap::new()),
            counts: Arc::new(DashMap::new()),
        }
    }

    /// Try to acquire a permit for the given key.
    ///
    /// Returns `Some(ConcurrentPermit)` if successful, `None` if at limit.
    /// The permit automatically releases when dropped.
    pub fn try_acquire(&self, key: &str) -> Option<ConcurrentPermit> {
        let semaphore = self
            .semaphores
            .entry(key.to_string())
            .or_insert_with(|| Arc::new(Semaphore::new(self.max_concurrent as usize)))
            .clone();

        // Try to acquire without blocking
        match semaphore.clone().try_acquire_owned() {
            Ok(permit) => {
                // Increment count
                *self.counts.entry(key.to_string()).or_insert(0) += 1;

                Some(ConcurrentPermit {
                    _permit: permit,
                    key: key.to_string(),
                    counts: self.counts.clone(),
                })
            }
            Err(_) => None,
        }
    }

    /// Acquire a permit, waiting if necessary.
    pub async fn acquire(&self, key: &str) -> ConcurrentPermit {
        let semaphore = self
            .semaphores
            .entry(key.to_string())
            .or_insert_with(|| Arc::new(Semaphore::new(self.max_concurrent as usize)))
            .clone();

        let permit = semaphore.acquire_owned().await.expect("Semaphore closed");

        *self.counts.entry(key.to_string()).or_insert(0) += 1;

        ConcurrentPermit {
            _permit: permit,
            key: key.to_string(),
            counts: self.counts.clone(),
        }
    }

    /// Acquire a permit with a timeout.
    pub async fn acquire_timeout(
        &self,
        key: &str,
        timeout: Duration,
    ) -> Option<ConcurrentPermit> {
        tokio::time::timeout(timeout, self.acquire(key))
            .await
            .ok()
    }

    /// Get the current count of active requests for a key.
    pub fn current_count(&self, key: &str) -> u32 {
        self.counts.get(key).map(|c| *c).unwrap_or(0)
    }

    /// Get the maximum concurrent requests allowed.
    pub fn max_concurrent(&self) -> u32 {
        self.max_concurrent
    }

    /// Get remaining slots for a key.
    pub fn remaining(&self, key: &str) -> u32 {
        self.max_concurrent.saturating_sub(self.current_count(key))
    }
}

/// A permit for a concurrent request.
///
/// While held, this counts against the concurrent limit.
/// Automatically releases when dropped.
pub struct ConcurrentPermit {
    _permit: tokio::sync::OwnedSemaphorePermit,
    key: String,
    counts: Arc<DashMap<String, u32>>,
}

impl Drop for ConcurrentPermit {
    fn drop(&mut self) {
        if let Some(mut count) = self.counts.get_mut(&self.key) {
            *count = count.saturating_sub(1);
        }
    }
}

impl std::fmt::Debug for ConcurrentPermit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConcurrentPermit")
            .field("key", &self.key)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_concurrent_basic() {
        let limiter = ConcurrentLimiter::new(2);

        let permit1 = limiter.try_acquire("user:1");
        assert!(permit1.is_some());
        assert_eq!(limiter.current_count("user:1"), 1);

        let permit2 = limiter.try_acquire("user:1");
        assert!(permit2.is_some());
        assert_eq!(limiter.current_count("user:1"), 2);

        // Third should fail
        let permit3 = limiter.try_acquire("user:1");
        assert!(permit3.is_none());

        // Different key should work
        let permit_other = limiter.try_acquire("user:2");
        assert!(permit_other.is_some());
    }

    #[tokio::test]
    async fn test_concurrent_release() {
        let limiter = ConcurrentLimiter::new(1);

        {
            let _permit = limiter.try_acquire("user:1");
            assert!(limiter.try_acquire("user:1").is_none());
        }

        // After drop, should be able to acquire again
        let permit = limiter.try_acquire("user:1");
        assert!(permit.is_some());
    }

    #[tokio::test]
    async fn test_concurrent_async_acquire() {
        let limiter = Arc::new(ConcurrentLimiter::new(1));

        let permit = limiter.try_acquire("user:1").unwrap();

        let limiter_clone = limiter.clone();
        let handle = tokio::spawn(async move {
            limiter_clone.acquire("user:1").await
        });

        // Short delay then release
        tokio::time::sleep(Duration::from_millis(10)).await;
        drop(permit);

        // Waiting acquire should complete
        let _permit2 = handle.await.unwrap();
    }
}
