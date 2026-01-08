//! Cache layer error types

use std::fmt;

/// Errors that can occur in the cache layer
#[derive(Debug, Clone)]
pub enum CacheError {
    /// Cache backend connection failed
    ConnectionError(String),

    /// Serialization/deserialization failed
    SerializationError(String),

    /// Key not found in cache
    KeyNotFound,

    /// Internal storage error
    StorageError(String),

    /// Invalid configuration
    ConfigError(String),

    /// Cache operation timeout
    TimeoutError,
}

impl fmt::Display for CacheError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CacheError::ConnectionError(msg) => write!(f, "Cache connection error: {}", msg),
            CacheError::SerializationError(msg) => write!(f, "Serialization error: {}", msg),
            CacheError::KeyNotFound => write!(f, "Cache key not found"),
            CacheError::StorageError(msg) => write!(f, "Cache storage error: {}", msg),
            CacheError::ConfigError(msg) => write!(f, "Cache config error: {}", msg),
            CacheError::TimeoutError => write!(f, "Cache operation timeout"),
        }
    }
}

impl std::error::Error for CacheError {}

/// Convert CacheError to ApiError for public API
impl From<CacheError> for crate::api::error::ApiError {
    fn from(err: CacheError) -> Self {
        crate::api::error::ApiError::InternalError(err.to_string())
    }
}
