//! Redis storage backend for distributed rate limiting.
//!
//! Uses connection pooling for high performance.

use std::time::Duration;

use deadpool_redis::{Config, Pool, Runtime, Connection, redis::{cmd, AsyncCommands}};

use crate::error::{ConnectionError, Result, StorageError};
use crate::storage::{Storage, StorageEntry};

/// Redis storage configuration.
#[derive(Debug, Clone)]
pub struct RedisConfig {
    /// Redis connection URL (e.g., "redis://localhost:6379")
    pub url: String,
    /// Connection pool size
    pub pool_size: usize,
    /// Key prefix for rate limit keys
    pub key_prefix: String,
    /// Connection timeout
    pub connection_timeout: Duration,
}

impl Default for RedisConfig {
    fn default() -> Self {
        Self {
            url: "redis://localhost:6379".to_string(),
            pool_size: 10,
            key_prefix: "rl:".to_string(),
            connection_timeout: Duration::from_secs(5),
        }
    }
}

impl RedisConfig {
    /// Create a new Redis configuration.
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            ..Default::default()
        }
    }

    /// Set the key prefix.
    pub fn with_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.key_prefix = prefix.into();
        self
    }

    /// Set the pool size.
    pub fn with_pool_size(mut self, size: usize) -> Self {
        self.pool_size = size;
        self
    }
}

/// Redis storage backend for distributed rate limiting.
///
/// Uses connection pooling for high performance.
///
/// # Example
///
/// ```ignore
/// use skp_ratelimit::storage::{RedisStorage, RedisConfig};
///
/// let config = RedisConfig::new("redis://localhost:6379")
///     .with_prefix("myapp:rl:")
///     .with_pool_size(20);
///
/// let storage = RedisStorage::new(config).await?;
/// ```
pub struct RedisStorage {
    pool: Pool,
    key_prefix: String,
}

impl std::fmt::Debug for RedisStorage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RedisStorage")
            .field("key_prefix", &self.key_prefix)
            .finish()
    }
}

impl RedisStorage {
    /// Create a new Redis storage from configuration.
    pub async fn new(config: RedisConfig) -> Result<Self> {
        let cfg = Config::from_url(&config.url);
        let pool = cfg
            .create_pool(Some(Runtime::Tokio1))
            .map_err(|e| ConnectionError::ConnectionFailed(e.to_string()))?;

        // Test connection
        let mut conn = pool
            .get()
            .await
            .map_err(|e| ConnectionError::ConnectionFailed(e.to_string()))?;
        let _: () = cmd("PING")
            .query_async(&mut *conn)
            .await
            .map_err(|e| ConnectionError::ConnectionFailed(e.to_string()))?;

        Ok(Self {
            pool,
            key_prefix: config.key_prefix,
        })
    }

    /// Create a new Redis storage from a URL.
    pub async fn from_url(url: impl Into<String>) -> Result<Self> {
        Self::new(RedisConfig::new(url)).await
    }

    /// Get the full key with prefix.
    fn full_key(&self, key: &str) -> String {
        format!("{}{}", self.key_prefix, key)
    }

    /// Get a connection from the pool.
    async fn get_conn(&self) -> Result<Connection> {
        self.pool
            .get()
            .await
            .map_err(|_| StorageError::PoolExhausted.into())
    }
}

impl Storage for RedisStorage {
    async fn get(&self, key: &str) -> Result<Option<StorageEntry>> {
        let mut conn = self.get_conn().await?;
        let full_key = self.full_key(key);

        let result: Option<String> = conn
            .get(&full_key)
            .await
            .map_err(|e| StorageError::operation_failed(e.to_string(), true))?;

        match result {
            Some(json) => {
                let entry: StorageEntry = serde_json::from_str(&json)
                    .map_err(|e| StorageError::Serialization(e.to_string()))?;
                Ok(Some(entry))
            }
            None => Ok(None),
        }
    }

    async fn set(&self, key: &str, entry: StorageEntry, ttl: Duration) -> Result<()> {
        let mut conn = self.get_conn().await?;
        let full_key = self.full_key(key);
        let ttl_secs = ttl.as_secs();

        let json = serde_json::to_string(&entry)
            .map_err(|e| StorageError::Serialization(e.to_string()))?;

        conn.set_ex::<_, _, ()>(&full_key, json, ttl_secs)
            .await
            .map_err(|e| StorageError::operation_failed(e.to_string(), true))?;

        Ok(())
    }

    async fn delete(&self, key: &str) -> Result<()> {
        let mut conn = self.get_conn().await?;
        let full_key = self.full_key(key);

        conn.del::<_, ()>(&full_key)
            .await
            .map_err(|e| StorageError::operation_failed(e.to_string(), true))?;

        Ok(())
    }

    async fn increment(
        &self,
        key: &str,
        delta: u64,
        window_start: u64,
        ttl: Duration,
    ) -> Result<u64> {
        let mut conn = self.get_conn().await?;
        let full_key = self.full_key(key);
        let ttl_secs = ttl.as_secs();

        // Get current value
        let current: Option<String> = conn
            .get(&full_key)
            .await
            .map_err(|e| StorageError::operation_failed(e.to_string(), true))?;

        let new_count = match current {
            Some(json) => {
                if let Ok(entry) = serde_json::from_str::<StorageEntry>(&json) {
                    if entry.window_start == window_start {
                        entry.count + delta
                    } else {
                        delta
                    }
                } else {
                    delta
                }
            }
            None => delta,
        };

        let now = crate::storage::current_timestamp_ms();
        let new_entry = StorageEntry {
            count: new_count,
            window_start,
            last_update: now,
            ..Default::default()
        };

        let json = serde_json::to_string(&new_entry)
            .map_err(|e| StorageError::Serialization(e.to_string()))?;

        conn.set_ex::<_, _, ()>(&full_key, json, ttl_secs)
            .await
            .map_err(|e| StorageError::operation_failed(e.to_string(), true))?;

        Ok(new_count)
    }

    async fn execute_atomic<F, T>(&self, key: &str, ttl: Duration, operation: F) -> Result<T>
    where
        F: FnOnce(Option<StorageEntry>) -> (StorageEntry, T) + Send,
        T: Send,
    {
        let mut conn = self.get_conn().await?;
        let full_key = self.full_key(key);
        let ttl_secs = ttl.as_secs();

        // Get current value
        let current: Option<String> = conn
            .get(&full_key)
            .await
            .map_err(|e| StorageError::operation_failed(e.to_string(), true))?;

        let entry = match current {
            Some(json) => Some(
                serde_json::from_str(&json)
                    .map_err(|e| StorageError::Serialization(e.to_string()))?,
            ),
            None => None,
        };

        // Execute the operation
        let (new_entry, result) = operation(entry);

        // Store the new entry
        let json = serde_json::to_string(&new_entry)
            .map_err(|e| StorageError::Serialization(e.to_string()))?;

        conn.set_ex::<_, _, ()>(&full_key, json, ttl_secs)
            .await
            .map_err(|e| StorageError::operation_failed(e.to_string(), true))?;

        Ok(result)
    }

    async fn compare_and_swap(
        &self,
        key: &str,
        expected: Option<&StorageEntry>,
        new: StorageEntry,
        ttl: Duration,
    ) -> Result<bool> {
        let mut conn = self.get_conn().await?;
        let full_key = self.full_key(key);
        let ttl_secs = ttl.as_secs();

        // Get current value
        let current: Option<String> = conn
            .get(&full_key)
            .await
            .map_err(|e| StorageError::operation_failed(e.to_string(), true))?;

        let current_entry: Option<StorageEntry> = match current {
            Some(json) => Some(
                serde_json::from_str(&json)
                    .map_err(|e| StorageError::Serialization(e.to_string()))?,
            ),
            None => None,
        };

        // Check if expected matches current
        let matches = match (expected, &current_entry) {
            (None, None) => true,
            (Some(exp), Some(cur)) => exp == cur,
            _ => false,
        };

        if !matches {
            return Ok(false);
        }

        // Set the new value
        let json = serde_json::to_string(&new)
            .map_err(|e| StorageError::Serialization(e.to_string()))?;

        conn.set_ex::<_, _, ()>(&full_key, json, ttl_secs)
            .await
            .map_err(|e| StorageError::operation_failed(e.to_string(), true))?;

        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_redis_config() {
        let config = RedisConfig::new("redis://localhost:6380")
            .with_prefix("test:")
            .with_pool_size(5);

        assert_eq!(config.url, "redis://localhost:6380");
        assert_eq!(config.key_prefix, "test:");
        assert_eq!(config.pool_size, 5);
    }
}
