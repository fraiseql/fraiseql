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

/// Sanitized server configuration for API exposure.
///
/// Phase 4.2: Configuration access with secret redaction
///
/// Removes sensitive fields like database URLs, API keys, and tokens
/// while preserving operational settings for client consumption.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SanitizedConfig {
    /// Server port
    pub port: u16,

    /// Server host address
    pub host: String,

    /// Number of worker threads
    pub workers: Option<usize>,

    /// Whether TLS is enabled
    pub tls_enabled: bool,

    /// Indicates configuration has been sanitized
    pub sanitized: bool,
}

impl SanitizedConfig {
    /// Create sanitized configuration from ServerConfig.
    ///
    /// Removes sensitive fields:
    /// - TLS private keys and certificates (replaced with boolean flag)
    /// - Database connection strings (not included)
    /// - API keys and tokens (not included)
    pub fn from_config(config: &crate::config::ServerConfig) -> Self {
        Self {
            port: config.port,
            host: config.host.clone(),
            workers: config.workers,
            tls_enabled: config.tls.is_some(),
            sanitized: true,
        }
    }

    /// Verify configuration has been properly sanitized.
    pub fn is_sanitized(&self) -> bool {
        self.sanitized
    }
}
