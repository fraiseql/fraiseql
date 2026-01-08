//! Storage layer error types

use std::fmt;

/// Errors that can occur in the storage layer
#[derive(Debug, Clone)]
pub enum StorageError {
    /// Database connection failed
    ConnectionError(String),

    /// SQL query execution failed
    QueryError(String),

    /// Transaction operation failed
    TransactionError(String),

    /// Query parameter binding failed
    ParameterError(String),

    /// Query execution timeout
    TimeoutError,

    /// Resource not found
    NotFound,

    /// Invalid configuration
    ConfigError(String),

    /// Database-specific error
    DatabaseError(String),
}

impl fmt::Display for StorageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StorageError::ConnectionError(msg) => write!(f, "Connection error: {}", msg),
            StorageError::QueryError(msg) => write!(f, "Query error: {}", msg),
            StorageError::TransactionError(msg) => write!(f, "Transaction error: {}", msg),
            StorageError::ParameterError(msg) => write!(f, "Parameter error: {}", msg),
            StorageError::TimeoutError => write!(f, "Query timeout"),
            StorageError::NotFound => write!(f, "Resource not found"),
            StorageError::ConfigError(msg) => write!(f, "Config error: {}", msg),
            StorageError::DatabaseError(msg) => write!(f, "Database error: {}", msg),
        }
    }
}

impl std::error::Error for StorageError {}

/// Convert StorageError to ApiError for public API
impl From<StorageError> for crate::api::error::ApiError {
    fn from(err: StorageError) -> Self {
        crate::api::error::ApiError::InternalError(err.to_string())
    }
}
