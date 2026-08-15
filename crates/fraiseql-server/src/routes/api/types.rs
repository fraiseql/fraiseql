//! Shared types for API responses and errors.

use std::fmt;

use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};

/// Standard API error response.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ApiError {
    /// Human-readable error message.
    pub error:   String,
    /// Machine-readable error code (e.g. `"NOT_FOUND"`, `"VALIDATION_ERROR"`).
    pub code:    String,
    /// Optional additional context about the error.
    pub details: Option<String>,
}

impl ApiError {
    /// Create a new API error with error message and code.
    pub fn new(error: impl Into<String>, code: impl Into<String>) -> Self {
        Self {
            error:   error.into(),
            code:    code.into(),
            details: None,
        }
    }

    /// Add details to the error.
    pub fn with_details(mut self, details: impl Into<String>) -> Self {
        self.details = Some(details.into());
        self
    }

    /// Create a parse error.
    pub fn parse_error(msg: impl fmt::Display) -> Self {
        Self::new(format!("Parse error: {}", msg), "PARSE_ERROR")
    }

    /// Create a validation error.
    pub fn validation_error(msg: impl fmt::Display) -> Self {
        Self::new(format!("Validation error: {}", msg), "VALIDATION_ERROR")
    }

    /// Create an internal server error.
    pub fn internal_error(msg: impl fmt::Display) -> Self {
        Self::new(format!("Internal server error: {}", msg), "INTERNAL_ERROR")
    }

    /// Create an unauthorized error.
    #[must_use]
    pub fn unauthorized() -> Self {
        Self::new("Unauthorized", "UNAUTHORIZED")
    }

    /// Create a not found error.
    pub fn not_found(msg: impl fmt::Display) -> Self {
        Self::new(format!("Not found: {}", msg), "NOT_FOUND")
    }
}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code, self.error)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = match self.code.as_str() {
            "UNAUTHORIZED" => StatusCode::UNAUTHORIZED,
            "FORBIDDEN" => StatusCode::FORBIDDEN,
            "NOT_FOUND" => StatusCode::NOT_FOUND,
            "VALIDATION_ERROR" | "PARSE_ERROR" => StatusCode::BAD_REQUEST,
            "UNSUPPORTED_OPERATION" => StatusCode::NOT_IMPLEMENTED,
            "SERVICE_UNAVAILABLE" => StatusCode::SERVICE_UNAVAILABLE,
            // The server cancelled the work, so the caller learns "too slow", not
            // "broken" (#962: the admin SQL console's statement timeout).
            "TIMEOUT" => StatusCode::REQUEST_TIMEOUT,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };

        (status, Json(self)).into_response()
    }
}

/// Standard API success response wrapper.
#[derive(Debug, Serialize, Deserialize)]
pub struct ApiResponse<T> {
    /// Always `"success"` for successful responses.
    pub status: String,
    /// The response payload.
    pub data:   T,
}

impl<T: Serialize> ApiResponse<T> {
    /// Create a successful response.
    pub fn success(data: T) -> Json<Self> {
        Json(Self {
            status: "success".to_string(),
            data,
        })
    }
}
