//! Security-specific error types for comprehensive error handling.

use std::fmt;

/// Main security error type
#[derive(Debug)]
pub enum SecurityError {
    /// Rate limiting errors
    RateLimitExceeded {
        /// Seconds to wait before retrying
        retry_after: u64,
        /// Maximum allowed requests
        limit: usize,
        /// Time window in seconds
        window_secs: u64,
    },

    /// Query validation errors
    QueryTooDeep {
        /// Actual query depth
        depth: usize,
        /// Maximum allowed depth
        max_depth: usize,
    },

    QueryTooComplex {
        /// Actual query complexity score
        complexity: usize,
        /// Maximum allowed complexity
        max_complexity: usize,
    },

    QueryTooLarge {
        /// Actual query size in bytes
        size: usize,
        /// Maximum allowed size in bytes
        max_size: usize,
    },

    /// CORS errors
    OriginNotAllowed(String),
    MethodNotAllowed(String),
    HeaderNotAllowed(String),

    /// CSRF errors
    InvalidCSRFToken(String),
    CSRFSessionMismatch,

    /// Audit logging errors
    AuditLogFailure(String),

    /// Configuration errors
    SecurityConfigError(String),
}

pub type Result<T> = std::result::Result<T, SecurityError>;

impl fmt::Display for SecurityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RateLimitExceeded {
                retry_after,
                limit,
                window_secs,
            } => {
                write!(
                    f,
                    "Rate limit exceeded. Limit: {limit} per {window_secs} seconds. Retry after: {retry_after} seconds"
                )
            }
            Self::QueryTooDeep { depth, max_depth } => {
                write!(f, "Query too deep: {depth} levels (max: {max_depth})")
            }
            Self::QueryTooComplex {
                complexity,
                max_complexity,
            } => {
                write!(f, "Query too complex: {complexity} (max: {max_complexity})")
            }
            Self::QueryTooLarge { size, max_size } => {
                write!(f, "Query too large: {size} bytes (max: {max_size})")
            }
            Self::OriginNotAllowed(origin) => {
                write!(f, "CORS origin not allowed: {origin}")
            }
            Self::MethodNotAllowed(method) => {
                write!(f, "CORS method not allowed: {method}")
            }
            Self::HeaderNotAllowed(header) => {
                write!(f, "CORS header not allowed: {header}")
            }
            Self::InvalidCSRFToken(reason) => {
                write!(f, "Invalid CSRF token: {reason}")
            }
            Self::CSRFSessionMismatch => {
                write!(f, "CSRF token session mismatch")
            }
            Self::AuditLogFailure(reason) => {
                write!(f, "Audit logging failed: {reason}")
            }
            Self::SecurityConfigError(reason) => {
                write!(f, "Security configuration error: {reason}")
            }
        }
    }
}

impl std::error::Error for SecurityError {}

impl From<tokio_postgres::Error> for SecurityError {
    fn from(error: tokio_postgres::Error) -> Self {
        Self::AuditLogFailure(error.to_string())
    }
}

impl From<deadpool::managed::PoolError<tokio_postgres::Error>> for SecurityError {
    fn from(error: deadpool::managed::PoolError<tokio_postgres::Error>) -> Self {
        Self::AuditLogFailure(error.to_string())
    }
}

#[cfg(feature = "python")]
impl From<SecurityError> for pyo3::PyErr {
    fn from(error: SecurityError) -> Self {
        use pyo3::exceptions::*;

        match error {
            SecurityError::RateLimitExceeded { .. } => PyException::new_err(error.to_string()),
            SecurityError::QueryTooDeep { .. }
            | SecurityError::QueryTooComplex { .. }
            | SecurityError::QueryTooLarge { .. } => PyValueError::new_err(error.to_string()),
            SecurityError::OriginNotAllowed(_)
            | SecurityError::MethodNotAllowed(_)
            | SecurityError::HeaderNotAllowed(_) => PyPermissionError::new_err(error.to_string()),
            _ => PyRuntimeError::new_err(error.to_string()),
        }
    }
}
