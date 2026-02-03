//! Shared types for API responses and errors.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};
use std::fmt;

/// Standard API error response.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ApiError {
    pub error: String,
    pub code: String,
    pub details: Option<String>,
}

impl ApiError {
    /// Create a new API error with error message and code.
    pub fn new(error: impl Into<String>, code: impl Into<String>) -> Self {
        Self {
            error: error.into(),
            code: code.into(),
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
            "NOT_FOUND" => StatusCode::NOT_FOUND,
            "VALIDATION_ERROR" => StatusCode::BAD_REQUEST,
            "PARSE_ERROR" => StatusCode::BAD_REQUEST,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };

        (status, Json(self)).into_response()
    }
}

/// Standard API success response wrapper.
#[derive(Debug, Serialize, Deserialize)]
pub struct ApiResponse<T> {
    pub status: String,
    pub data: T,
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
