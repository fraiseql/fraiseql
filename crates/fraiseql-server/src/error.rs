//! GraphQL error response handling.
//!
//! Implements GraphQL spec-compliant error responses with:
//! - Error codes for client-side handling
//! - Location tracking in queries
//! - Extensions for custom error data

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;

/// GraphQL error code enumeration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ErrorCode {
    /// Validation error.
    ValidationError,
    /// Parse error.
    ParseError,
    /// Request error.
    RequestError,
    /// Authentication required.
    Unauthenticated,
    /// Access denied.
    Forbidden,
    /// Internal server error.
    InternalServerError,
    /// Database error.
    DatabaseError,
    /// Timeout error.
    Timeout,
    /// Rate limit exceeded.
    RateLimitExceeded,
    /// Not found.
    NotFound,
    /// Conflict.
    Conflict,
}

impl ErrorCode {
    /// Get HTTP status code for this error.
    #[must_use] 
    pub fn status_code(self) -> StatusCode {
        match self {
            Self::ValidationError | Self::ParseError | Self::RequestError => StatusCode::BAD_REQUEST,
            Self::Unauthenticated => StatusCode::UNAUTHORIZED,
            Self::Forbidden => StatusCode::FORBIDDEN,
            Self::NotFound => StatusCode::NOT_FOUND,
            Self::Conflict => StatusCode::CONFLICT,
            Self::RateLimitExceeded => StatusCode::TOO_MANY_REQUESTS,
            Self::Timeout => StatusCode::REQUEST_TIMEOUT,
            Self::InternalServerError | Self::DatabaseError => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

/// Error location in GraphQL query.
#[derive(Debug, Clone, Serialize)]
pub struct ErrorLocation {
    /// Line number (1-indexed).
    pub line: usize,
    /// Column number (1-indexed).
    pub column: usize,
}

/// GraphQL error following spec.
#[derive(Debug, Clone, Serialize)]
pub struct GraphQLError {
    /// Error message.
    pub message: String,

    /// Error code for client handling.
    pub code: ErrorCode,

    /// Location in query where error occurred.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub locations: Option<Vec<ErrorLocation>>,

    /// Path to field that caused error.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<Vec<String>>,

    /// Additional error information.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extensions: Option<ErrorExtensions>,
}

/// Additional error context and debugging information.
#[derive(Debug, Clone, Serialize)]
pub struct ErrorExtensions {
    /// Error category.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,

    /// HTTP status code.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<u16>,

    /// Request ID for tracking.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
}

/// GraphQL response with errors.
#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    /// Errors that occurred.
    pub errors: Vec<GraphQLError>,
}

impl GraphQLError {
    /// Create a new GraphQL error.
    pub fn new(message: impl Into<String>, code: ErrorCode) -> Self {
        Self {
            message: message.into(),
            code,
            locations: None,
            path: None,
            extensions: None,
        }
    }

    /// Add location to error.
    #[must_use] 
    pub fn with_location(mut self, line: usize, column: usize) -> Self {
        self.locations = Some(vec![ErrorLocation { line, column }]);
        self
    }

    /// Add path to error.
    #[must_use] 
    pub fn with_path(mut self, path: Vec<String>) -> Self {
        self.path = Some(path);
        self
    }

    /// Add extensions to error.
    #[must_use] 
    pub fn with_extensions(mut self, extensions: ErrorExtensions) -> Self {
        self.extensions = Some(extensions);
        self
    }

    /// Validation error.
    pub fn validation(message: impl Into<String>) -> Self {
        Self::new(message, ErrorCode::ValidationError)
    }

    /// Parse error.
    pub fn parse(message: impl Into<String>) -> Self {
        Self::new(message, ErrorCode::ParseError)
    }

    /// Request error.
    pub fn request(message: impl Into<String>) -> Self {
        Self::new(message, ErrorCode::RequestError)
    }

    /// Database error.
    pub fn database(message: impl Into<String>) -> Self {
        Self::new(message, ErrorCode::DatabaseError)
    }

    /// Internal server error.
    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(message, ErrorCode::InternalServerError)
    }

    /// Execution error.
    #[must_use] 
    pub fn execution(message: &str) -> Self {
        Self::new(message, ErrorCode::InternalServerError)
    }

    /// Unauthenticated error.
    #[must_use] 
    pub fn unauthenticated() -> Self {
        Self::new("Authentication required", ErrorCode::Unauthenticated)
    }

    /// Forbidden error.
    #[must_use] 
    pub fn forbidden() -> Self {
        Self::new("Access denied", ErrorCode::Forbidden)
    }

    /// Not found error.
    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(message, ErrorCode::NotFound)
    }
}

impl ErrorResponse {
    /// Create new error response.
    #[must_use] 
    pub fn new(errors: Vec<GraphQLError>) -> Self {
        Self { errors }
    }

    /// Create from single error.
    #[must_use] 
    pub fn from_error(error: GraphQLError) -> Self {
        Self {
            errors: vec![error],
        }
    }
}

impl IntoResponse for ErrorResponse {
    fn into_response(self) -> Response {
        let status = self
            .errors
            .first()
            .map_or(StatusCode::INTERNAL_SERVER_ERROR, |e| e.code.status_code());

        (status, Json(self)).into_response()
    }
}

impl From<GraphQLError> for ErrorResponse {
    fn from(error: GraphQLError) -> Self {
        Self::from_error(error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_serialization() {
        let error = GraphQLError::validation("Invalid query")
            .with_location(1, 5)
            .with_path(vec!["user".to_string(), "id".to_string()]);

        let json = serde_json::to_string(&error).unwrap();
        assert!(json.contains("Invalid query"));
        assert!(json.contains("VALIDATION_ERROR"));
        assert!(json.contains("\"line\":1"));
    }

    #[test]
    fn test_error_response_serialization() {
        let response = ErrorResponse::new(vec![
            GraphQLError::validation("Field not found"),
            GraphQLError::database("Connection timeout"),
        ]);

        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("Field not found"));
        assert!(json.contains("Connection timeout"));
    }

    #[test]
    fn test_error_code_status_codes() {
        assert_eq!(
            ErrorCode::ValidationError.status_code(),
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            ErrorCode::Unauthenticated.status_code(),
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            ErrorCode::Forbidden.status_code(),
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            ErrorCode::DatabaseError.status_code(),
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }

    #[test]
    fn test_error_extensions() {
        let extensions = ErrorExtensions {
            category: Some("VALIDATION".to_string()),
            status: Some(400),
            request_id: Some("req-123".to_string()),
        };

        let error = GraphQLError::validation("Invalid").with_extensions(extensions);
        let json = serde_json::to_string(&error).unwrap();
        assert!(json.contains("VALIDATION"));
        assert!(json.contains("req-123"));
    }
}
