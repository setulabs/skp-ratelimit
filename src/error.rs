//! Error types for rate limiting operations.
//!
//! This module provides a comprehensive error hierarchy for all rate limiting
//! operations, including storage errors, configuration errors, and key extraction errors.

use std::time::Duration;
use thiserror::Error;

/// Result type for rate limiting operations.
pub type Result<T> = std::result::Result<T, RateLimitError>;

/// Main error type for rate limiting operations.
#[derive(Debug, Error)]
pub enum RateLimitError {
    /// Storage backend error.
    #[error("Storage error: {0}")]
    Storage(#[from] StorageError),

    /// Configuration error.
    #[error("Configuration error: {0}")]
    Config(#[from] ConfigError),

    /// Key extraction error.
    #[error("Key extraction failed: {0}")]
    KeyExtraction(String),

    /// Connection error (e.g., Redis connection failed).
    #[error("Connection error: {0}")]
    Connection(#[from] ConnectionError),

    /// Internal error.
    #[error("Internal error: {0}")]
    Internal(String),

    /// Rate limit exceeded with retry information.
    #[error("Rate limit exceeded, retry after {retry_after:?}")]
    RateLimitExceeded {
        /// How long to wait before retrying.
        retry_after: Option<Duration>,
        /// Current remaining quota.
        remaining: u64,
        /// Maximum quota limit.
        limit: u64,
    },
}

/// Storage-related errors.
#[derive(Debug, Error)]
pub enum StorageError {
    /// Generic storage operation failed.
    #[error("{message}")]
    OperationFailed {
        /// Error message.
        message: String,
        /// Whether the operation can be retried.
        retryable: bool,
    },

    /// Key not found.
    #[error("Key not found: {0}")]
    KeyNotFound(String),

    /// Serialization/deserialization error.
    #[error("Serialization error: {0}")]
    Serialization(String),

    /// Atomic operation failed (CAS conflict).
    #[error("Atomic operation failed, state was modified concurrently")]
    AtomicConflict,

    /// Connection pool exhausted.
    #[error("Connection pool exhausted")]
    PoolExhausted,
}

impl StorageError {
    /// Create a new operation failed error.
    pub fn operation_failed(message: impl Into<String>, retryable: bool) -> Self {
        Self::OperationFailed {
            message: message.into(),
            retryable,
        }
    }

    /// Check if this error is retryable.
    pub fn is_retryable(&self) -> bool {
        match self {
            Self::OperationFailed { retryable, .. } => *retryable,
            Self::AtomicConflict => true,
            Self::PoolExhausted => true,
            _ => false,
        }
    }
}

/// Configuration-related errors.
#[derive(Debug, Error)]
pub enum ConfigError {
    /// Invalid quota configuration.
    #[error("Invalid quota: {0}")]
    InvalidQuota(String),

    /// Invalid algorithm configuration.
    #[error("Invalid algorithm configuration: {0}")]
    InvalidAlgorithm(String),

    /// Invalid storage configuration.
    #[error("Invalid storage configuration: {0}")]
    InvalidStorage(String),

    /// Missing required configuration.
    #[error("Missing required configuration: {0}")]
    MissingRequired(String),
}

/// Connection-related errors.
#[derive(Debug, Error)]
pub enum ConnectionError {
    /// Failed to connect.
    #[error("Failed to connect: {0}")]
    ConnectionFailed(String),

    /// Connection timeout.
    #[error("Connection timeout after {0:?}")]
    Timeout(Duration),

    /// Connection closed unexpectedly.
    #[error("Connection closed unexpectedly")]
    Closed,

    /// Authentication failed.
    #[error("Authentication failed: {0}")]
    AuthFailed(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_storage_error_retryable() {
        let err = StorageError::operation_failed("test", true);
        assert!(err.is_retryable());

        let err = StorageError::operation_failed("test", false);
        assert!(!err.is_retryable());

        let err = StorageError::AtomicConflict;
        assert!(err.is_retryable());

        let err = StorageError::KeyNotFound("key".into());
        assert!(!err.is_retryable());
    }

    #[test]
    fn test_error_display() {
        let err = RateLimitError::KeyExtraction("missing header".into());
        assert_eq!(err.to_string(), "Key extraction failed: missing header");

        let err = RateLimitError::RateLimitExceeded {
            retry_after: Some(Duration::from_secs(10)),
            remaining: 0,
            limit: 100,
        };
        assert!(err.to_string().contains("retry after"));
    }
}
