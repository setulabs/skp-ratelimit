//! In-memory storage with automatic garbage collection.
//!
//! This storage backend uses `DashMap` for thread-safe concurrent access
//! and includes configurable garbage collection to prevent memory growth.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use dashmap::DashMap;
use parking_lot::Mutex;
use tokio::sync::Notify;

use crate::error::Result;
use crate::storage::{current_timestamp_ms, Storage, StorageEntry};

/// Garbage collection interval configuration.
#[derive(Debug, Clone)]
pub enum GcInterval {
    /// Run GC every N requests.
    Requests(u64),
    /// Run GC at fixed time intervals.
    Duration(Duration),
    /// Disable automatic GC.
    Manual,
}

impl Default for GcInterval {
    fn default() -> Self {
        Self::Requests(10000)
    }
}

/// Garbage collection configuration.
#[derive(Debug, Clone)]
pub struct GcConfig {
    /// When to trigger GC.
    pub interval: GcInterval,
    /// Maximum age of entries before cleanup (default: 1 hour).
    pub max_age: Duration,
}

impl Default for GcConfig {
    fn default() -> Self {
        Self {
            interval: GcInterval::default(),
            max_age: Duration::from_secs(3600),
        }
    }
}

impl GcConfig {
    /// Create config with request-based GC.
    pub fn on_requests(count: u64) -> Self {
        Self {
            interval: GcInterval::Requests(count),
            ..Default::default()
        }
    }

    /// Create config with time-based GC.
    pub fn on_duration(interval: Duration) -> Self {
        Self {
            interval: GcInterval::Duration(interval),
            ..Default::default()
        }
    }

    /// Create config with manual GC only.
    pub fn manual() -> Self {
        Self {
            interval: GcInterval::Manual,
            ..Default::default()
        }
    }

    /// Set the maximum age for entries.
    pub fn with_max_age(mut self, max_age: Duration) -> Self {
        self.max_age = max_age;
        self
    }
}

/// Internal entry with expiration tracking.
#[derive(Debug, Clone)]
struct InternalEntry {
    entry: StorageEntry,
    expires_at: u64,
}

/// In-memory storage with garbage collection.
///
/// Uses `DashMap` for thread-safe concurrent access and includes
/// configurable garbage collection to prevent unbounded memory growth.
///
/// # Example
///
/// ```ignore
/// use oc_ratelimit_advanced::storage::{MemoryStorage, GcConfig};
/// use std::time::Duration;
///
/// // Default GC (every 10000 requests)
/// let storage = MemoryStorage::new();
///
/// // Custom GC interval
/// let storage = MemoryStorage::with_gc(GcConfig::on_duration(Duration::from_secs(60)));
///
/// // Manual GC only
/// let storage = MemoryStorage::with_gc(GcConfig::manual());
/// storage.run_gc().await;
/// ```
pub struct MemoryStorage {
    data: DashMap<String, InternalEntry>,
    gc_config: GcConfig,
    request_count: AtomicU64,
    last_gc: AtomicU64,
    gc_lock: Mutex<()>,
    shutdown: Arc<Notify>,
}

impl std::fmt::Debug for MemoryStorage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MemoryStorage")
            .field("entries", &self.data.len())
            .field("gc_config", &self.gc_config)
            .finish()
    }
}

impl Default for MemoryStorage {
    fn default() -> Self {
        Self::new()
    }
}

impl MemoryStorage {
    /// Create a new memory storage with default GC configuration.
    pub fn new() -> Self {
        Self::with_gc(GcConfig::default())
    }

    /// Create a new memory storage with custom GC configuration.
    pub fn with_gc(gc_config: GcConfig) -> Self {
        let storage = Self {
            data: DashMap::new(),
            gc_config: gc_config.clone(),
            request_count: AtomicU64::new(0),
            last_gc: AtomicU64::new(current_timestamp_ms()),
            gc_lock: Mutex::new(()),
            shutdown: Arc::new(Notify::new()),
        };

        // Start background GC task if duration-based
        if let GcInterval::Duration(interval) = gc_config.interval {
            storage.start_gc_task(interval);
        }

        storage
    }

    /// Start background GC task.
    fn start_gc_task(&self, interval: Duration) {
        let data = self.data.clone();
        let max_age = self.gc_config.max_age;
        let shutdown = self.shutdown.clone();

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = tokio::time::sleep(interval) => {
                        run_gc_on_map(&data, max_age);
                    }
                    _ = shutdown.notified() => {
                        break;
                    }
                }
            }
        });
    }

    /// Manually trigger garbage collection.
    pub async fn run_gc(&self) {
        run_gc_on_map(&self.data, self.gc_config.max_age);
    }

    /// Get the number of entries currently stored.
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// Check if the storage is empty.
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// Clear all entries.
    pub fn clear(&self) {
        self.data.clear();
    }

    /// Check if GC should run and run it if needed.
    fn maybe_run_gc(&self) {
        if let GcInterval::Requests(threshold) = self.gc_config.interval {
            let count = self.request_count.fetch_add(1, Ordering::Relaxed);
            if count % threshold == 0 && count > 0 {
                // Try to acquire GC lock (non-blocking)
                if let Some(_guard) = self.gc_lock.try_lock() {
                    run_gc_on_map(&self.data, self.gc_config.max_age);
                }
            }
        }
    }
}

impl Drop for MemoryStorage {
    fn drop(&mut self) {
        self.shutdown.notify_waiters();
    }
}

/// Run garbage collection on a DashMap.
fn run_gc_on_map(data: &DashMap<String, InternalEntry>, max_age: Duration) {
    let now = current_timestamp_ms();
    let max_age_ms = max_age.as_millis() as u64;
    let cutoff = now.saturating_sub(max_age_ms);

    data.retain(|_, entry| {
        // Keep if not expired and not too old
        entry.expires_at > now || entry.entry.last_update > cutoff
    });
}

impl Storage for MemoryStorage {
    async fn get(&self, key: &str) -> Result<Option<StorageEntry>> {
        self.maybe_run_gc();

        let now = current_timestamp_ms();
        if let Some(internal) = self.data.get(key) {
            if internal.expires_at > now {
                return Ok(Some(internal.entry.clone()));
            }
            // Entry expired, remove it
            drop(internal);
            self.data.remove(key);
        }
        Ok(None)
    }

    async fn set(&self, key: &str, entry: StorageEntry, ttl: Duration) -> Result<()> {
        self.maybe_run_gc();

        let expires_at = current_timestamp_ms() + ttl.as_millis() as u64;
        self.data.insert(
            key.to_string(),
            InternalEntry { entry, expires_at },
        );
        Ok(())
    }

    async fn delete(&self, key: &str) -> Result<()> {
        self.data.remove(key);
        Ok(())
    }

    async fn increment(
        &self,
        key: &str,
        delta: u64,
        window_start: u64,
        ttl: Duration,
    ) -> Result<u64> {
        self.maybe_run_gc();

        let expires_at = current_timestamp_ms() + ttl.as_millis() as u64;
        let now = current_timestamp_ms();

        let new_count = self.data
            .entry(key.to_string())
            .and_modify(|internal| {
                // Check if we're in a new window
                if internal.entry.window_start != window_start {
                    // Store old count as prev_count for sliding window
                    internal.entry.prev_count = Some(internal.entry.count);
                    internal.entry.count = delta;
                    internal.entry.window_start = window_start;
                } else {
                    internal.entry.count += delta;
                }
                internal.entry.last_update = now;
                internal.expires_at = expires_at;
            })
            .or_insert_with(|| InternalEntry {
                entry: StorageEntry::new(delta, window_start).set_last_update(now),
                expires_at,
            })
            .entry
            .count;

        Ok(new_count)
    }

    async fn execute_atomic<F, T>(&self, key: &str, ttl: Duration, operation: F) -> Result<T>
    where
        F: FnOnce(Option<StorageEntry>) -> (StorageEntry, T) + Send,
        T: Send,
    {
        self.maybe_run_gc();

        let expires_at = current_timestamp_ms() + ttl.as_millis() as u64;
        let now = current_timestamp_ms();

        // Get current entry
        let current = self.data.get(key).and_then(|internal| {
            if internal.expires_at > now {
                Some(internal.entry.clone())
            } else {
                None
            }
        });

        // Execute operation
        let (new_entry, result) = operation(current);

        // Store new entry
        self.data.insert(
            key.to_string(),
            InternalEntry {
                entry: new_entry,
                expires_at,
            },
        );

        Ok(result)
    }

    async fn compare_and_swap(
        &self,
        key: &str,
        expected: Option<&StorageEntry>,
        new: StorageEntry,
        ttl: Duration,
    ) -> Result<bool> {
        self.maybe_run_gc();

        let expires_at = current_timestamp_ms() + ttl.as_millis() as u64;
        let now = current_timestamp_ms();

        // Get current entry
        let current = self.data.get(key).and_then(|internal| {
            if internal.expires_at > now {
                Some(internal.entry.clone())
            } else {
                None
            }
        });

        // Check if expected matches current
        let matches = match (expected, &current) {
            (None, None) => true,
            (Some(exp), Some(cur)) => exp == cur,
            _ => false,
        };

        if matches {
            self.data.insert(
                key.to_string(),
                InternalEntry {
                    entry: new,
                    expires_at,
                },
            );
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_memory_storage_basic() {
        let storage = MemoryStorage::new();
        
        let entry = StorageEntry::new(5, 1000);
        storage.set("key1", entry.clone(), Duration::from_secs(60)).await.unwrap();
        
        let result = storage.get("key1").await.unwrap();
        assert_eq!(result, Some(entry));
    }

    #[tokio::test]
    async fn test_memory_storage_expiration() {
        let storage = MemoryStorage::new();
        
        let entry = StorageEntry::new(5, 1000);
        storage.set("key1", entry, Duration::from_millis(10)).await.unwrap();
        
        // Wait for expiration
        tokio::time::sleep(Duration::from_millis(20)).await;
        
        let result = storage.get("key1").await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_memory_storage_increment() {
        let storage = MemoryStorage::new();
        
        let count = storage.increment("key1", 1, 1000, Duration::from_secs(60)).await.unwrap();
        assert_eq!(count, 1);
        
        let count = storage.increment("key1", 1, 1000, Duration::from_secs(60)).await.unwrap();
        assert_eq!(count, 2);
        
        // New window
        let count = storage.increment("key1", 1, 2000, Duration::from_secs(60)).await.unwrap();
        assert_eq!(count, 1);
        
        // Check prev_count is stored
        let entry = storage.get("key1").await.unwrap().unwrap();
        assert_eq!(entry.prev_count, Some(2));
    }

    #[tokio::test]
    async fn test_memory_storage_execute_atomic() {
        let storage = MemoryStorage::new();
        
        let result = storage
            .execute_atomic("key1", Duration::from_secs(60), |current| {
                let count = current.map(|e| e.count).unwrap_or(0);
                let new_entry = StorageEntry::new(count + 1, 1000);
                (new_entry, count + 1)
            })
            .await
            .unwrap();
        
        assert_eq!(result, 1);
        
        let result = storage
            .execute_atomic("key1", Duration::from_secs(60), |current| {
                let count = current.map(|e| e.count).unwrap_or(0);
                let new_entry = StorageEntry::new(count + 1, 1000);
                (new_entry, count + 1)
            })
            .await
            .unwrap();
        
        assert_eq!(result, 2);
    }

    #[tokio::test]
    async fn test_memory_storage_cas() {
        let storage = MemoryStorage::new();
        
        // CAS on non-existent key
        let entry = StorageEntry::new(1, 1000);
        let success = storage
            .compare_and_swap("key1", None, entry.clone(), Duration::from_secs(60))
            .await
            .unwrap();
        assert!(success);
        
        // CAS with wrong expected value
        let wrong = StorageEntry::new(999, 1000);
        let entry2 = StorageEntry::new(2, 1000);
        let success = storage
            .compare_and_swap("key1", Some(&wrong), entry2.clone(), Duration::from_secs(60))
            .await
            .unwrap();
        assert!(!success);
        
        // CAS with correct expected value
        let success = storage
            .compare_and_swap("key1", Some(&entry), entry2.clone(), Duration::from_secs(60))
            .await
            .unwrap();
        assert!(success);
    }

    #[tokio::test]
    async fn test_gc_config() {
        let config = GcConfig::on_requests(1000)
            .with_max_age(Duration::from_secs(3600));
        
        assert!(matches!(config.interval, GcInterval::Requests(1000)));
        assert_eq!(config.max_age, Duration::from_secs(3600));
    }
}
