//! Error types for database operations.

use std::fmt;

/// Database operation errors.
#[derive(Debug, Clone)]
pub enum DatabaseError {
    /// Runtime initialization failed
    RuntimeInitialization(String),
    /// Pool creation failed
    PoolCreation(String),
    /// Connection acquisition failed
    ConnectionAcquisition(String),
    /// Query execution failed
    QueryExecution(String),
    /// Health check failed
    HealthCheck(String),
    /// Configuration error
    Configuration(String),
    /// SSL/TLS error
    Ssl(String),
}

impl fmt::Display for DatabaseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RuntimeInitialization(e) => write!(f, "Runtime initialization failed: {e}"),
            Self::PoolCreation(e) => write!(f, "Pool creation failed: {e}"),
            Self::ConnectionAcquisition(e) => write!(f, "Connection acquisition failed: {e}"),
            Self::QueryExecution(e) => write!(f, "Query execution failed: {e}"),
            Self::HealthCheck(e) => write!(f, "Health check failed: {e}"),
            Self::Configuration(e) => write!(f, "Configuration error: {e}"),
            Self::Ssl(e) => write!(f, "SSL/TLS error: {e}"),
        }
    }
}

impl std::error::Error for DatabaseError {}

/// Result type alias for database operations.
pub type DatabaseResult<T> = Result<T, DatabaseError>;
