//! Public error types for FraiseQL API

use std::fmt;

/// Public API error type
///
/// All errors from FraiseQL public API are mapped to this type.
/// Python code receives these as exceptions.
#[derive(Debug, Clone)]
pub enum ApiError {
    /// Error during query execution
    QueryError(String),
    /// Error during mutation execution
    MutationError(String),
    /// Authentication failed
    AuthenticationError(String),
    /// Authorization denied
    AuthorizationError(String),
    /// Input validation failed
    ValidationError(String),
    /// Internal engine error
    InternalError(String),
}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ApiError::QueryError(msg) => write!(f, "Query error: {}", msg),
            ApiError::MutationError(msg) => write!(f, "Mutation error: {}", msg),
            ApiError::AuthenticationError(msg) => write!(f, "Authentication error: {}", msg),
            ApiError::AuthorizationError(msg) => write!(f, "Authorization error: {}", msg),
            ApiError::ValidationError(msg) => write!(f, "Validation error: {}", msg),
            ApiError::InternalError(msg) => write!(f, "Internal error: {}", msg),
        }
    }
}

impl std::error::Error for ApiError {}

/// Convert API error to PyErr for Python FFI
impl From<ApiError> for pyo3::PyErr {
    fn from(err: ApiError) -> Self {
        pyo3::exceptions::PyException::new_err(err.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = ApiError::QueryError("test error".to_string());
        assert_eq!(err.to_string(), "Query error: test error");
    }

    #[test]
    fn test_error_types() {
        let _query_err = ApiError::QueryError("q".to_string());
        let _mutation_err = ApiError::MutationError("m".to_string());
        let _auth_err = ApiError::AuthenticationError("a".to_string());
        let _authz_err = ApiError::AuthorizationError("az".to_string());
        let _validation_err = ApiError::ValidationError("v".to_string());
        let _internal_err = ApiError::InternalError("i".to_string());
    }
}
