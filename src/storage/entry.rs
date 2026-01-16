//! Storage entry type for rate limiting state.

use serde::{Deserialize, Serialize};

/// Entry stored in the storage backend.
///
/// This struct contains all state needed by rate limiting algorithms,
/// designed to be flexible enough for any algorithm type.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StorageEntry {
    /// Request count (for window-based algorithms).
    pub count: u64,

    /// Window start timestamp (Unix milliseconds).
    pub window_start: u64,

    /// Theoretical Arrival Time for GCRA (Unix milliseconds).
    pub tat: Option<u64>,

    /// Available tokens (for token bucket algorithm).
    pub tokens: Option<f64>,

    /// Last update timestamp (Unix milliseconds).
    pub last_update: u64,

    /// Previous window count (for sliding window).
    pub prev_count: Option<u64>,

    /// Request timestamps (for sliding log algorithm).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamps: Option<Vec<u64>>,

    /// Optional metadata (algorithm-specific).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Vec<u8>>,
}

impl StorageEntry {
    /// Create a new storage entry for window-based algorithms.
    pub fn new(count: u64, window_start: u64) -> Self {
        Self {
            count,
            window_start,
            tat: None,
            tokens: None,
            last_update: window_start,
            prev_count: None,
            timestamps: None,
            metadata: None,
        }
    }

    /// Create a storage entry for GCRA algorithm.
    pub fn with_tat(tat: u64) -> Self {
        Self {
            count: 0,
            window_start: tat,
            tat: Some(tat),
            tokens: None,
            last_update: tat,
            prev_count: None,
            timestamps: None,
            metadata: None,
        }
    }

    /// Create a storage entry for token bucket.
    pub fn with_tokens(tokens: f64, last_update: u64) -> Self {
        Self {
            count: 0,
            window_start: last_update,
            tat: None,
            tokens: Some(tokens),
            last_update,
            prev_count: None,
            timestamps: None,
            metadata: None,
        }
    }

    /// Create a storage entry for sliding log.
    pub fn with_timestamps(timestamps: Vec<u64>) -> Self {
        let now = timestamps.last().copied().unwrap_or(0);
        Self {
            count: timestamps.len() as u64,
            window_start: now,
            tat: None,
            tokens: None,
            last_update: now,
            prev_count: None,
            timestamps: Some(timestamps),
            metadata: None,
        }
    }

    /// Set the TAT value.
    pub fn set_tat(mut self, tat: u64) -> Self {
        self.tat = Some(tat);
        self
    }

    /// Set the token count.
    pub fn set_tokens(mut self, tokens: f64) -> Self {
        self.tokens = Some(tokens);
        self
    }

    /// Set the last update timestamp.
    pub fn set_last_update(mut self, last_update: u64) -> Self {
        self.last_update = last_update;
        self
    }

    /// Set previous window count.
    pub fn set_prev_count(mut self, count: u64) -> Self {
        self.prev_count = Some(count);
        self
    }

    /// Set metadata.
    pub fn set_metadata(mut self, metadata: Vec<u8>) -> Self {
        self.metadata = Some(metadata);
        self
    }

    /// Get tokens, defaulting to 0.0 if not set.
    pub fn tokens_or_default(&self) -> f64 {
        self.tokens.unwrap_or(0.0)
    }

    /// Get TAT, defaulting to 0 if not set.
    pub fn tat_or_default(&self) -> u64 {
        self.tat.unwrap_or(0)
    }
}

impl Default for StorageEntry {
    fn default() -> Self {
        Self {
            count: 0,
            window_start: 0,
            tat: None,
            tokens: None,
            last_update: 0,
            prev_count: None,
            timestamps: None,
            metadata: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_entry_new() {
        let entry = StorageEntry::new(5, 1000);
        assert_eq!(entry.count, 5);
        assert_eq!(entry.window_start, 1000);
        assert!(entry.tat.is_none());
        assert!(entry.tokens.is_none());
    }

    #[test]
    fn test_entry_with_tat() {
        let entry = StorageEntry::with_tat(5000);
        assert_eq!(entry.tat, Some(5000));
        assert_eq!(entry.tat_or_default(), 5000);
    }

    #[test]
    fn test_entry_with_tokens() {
        let entry = StorageEntry::with_tokens(10.5, 2000);
        assert_eq!(entry.tokens, Some(10.5));
        assert_eq!(entry.tokens_or_default(), 10.5);
        assert_eq!(entry.last_update, 2000);
    }

    #[test]
    fn test_entry_with_timestamps() {
        let timestamps = vec![1000, 2000, 3000];
        let entry = StorageEntry::with_timestamps(timestamps.clone());
        assert_eq!(entry.timestamps, Some(timestamps));
        assert_eq!(entry.count, 3);
    }

    #[test]
    fn test_entry_serialization() {
        let entry = StorageEntry::new(10, 1000).set_tokens(5.5).set_tat(2000);
        let json = serde_json::to_string(&entry).unwrap();
        let deserialized: StorageEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(entry, deserialized);
    }
}
