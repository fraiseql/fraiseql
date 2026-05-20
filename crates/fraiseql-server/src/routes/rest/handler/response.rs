//! REST response and error types.
//!
//! [`RestResponse`] wraps HTTP status, headers, and optional body.
//! [`RestError`] maps [`FraiseQLError`] to HTTP status codes and structured
//! error responses.

use axum::http::StatusCode;
use fraiseql_error::FraiseQLError;
use serde_json::json;

/// HTTP response with status, headers, and optional body.
pub struct RestResponse {
    /// HTTP status code.
    pub status: StatusCode,
    /// Response headers.
    pub headers: axum::http::HeaderMap,
    /// Response body (None for 204 No Content).
    pub body: Option<serde_json::Value>,
}

/// REST-specific error with HTTP status code.
#[derive(Debug)]
pub struct RestError {
    /// HTTP status code.
    pub status: StatusCode,
    /// Error code string.
    pub code: &'static str,
    /// Human-readable error message.
    pub message: String,
    /// Structured details for field-level errors.
    pub details: Option<serde_json::Value>,
}

impl RestError {
    /// 400 Bad Request.
    pub fn bad_request(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            code: "BAD_REQUEST",
            message: message.into(),
            details: None,
        }
    }

    /// 403 Forbidden.
    #[must_use]
    pub fn forbidden() -> Self {
        Self {
            status: StatusCode::FORBIDDEN,
            code: "FORBIDDEN",
            message: "Access denied".to_string(),
            details: None,
        }
    }

    /// 404 Not Found.
    pub fn not_found(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            code: "NOT_FOUND",
            message: message.into(),
            details: None,
        }
    }

    /// 422 Unprocessable Entity.
    pub fn unprocessable_entity(message: impl Into<String>, details: serde_json::Value) -> Self {
        Self {
            status: StatusCode::UNPROCESSABLE_ENTITY,
            code: "UNPROCESSABLE_ENTITY",
            message: message.into(),
            details: Some(details),
        }
    }

    /// 500 Internal Server Error.
    pub fn internal(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            code: "INTERNAL_SERVER_ERROR",
            message: message.into(),
            details: None,
        }
    }

    /// Convert to a JSON error body.
    #[must_use]
    pub fn to_json(&self) -> serde_json::Value {
        let mut error = json!({
            "error": {
                "code": self.code,
                "message": self.message,
            }
        });
        if let Some(ref details) = self.details {
            error["error"]["details"] = details.clone();
        }
        error
    }
}

impl From<FraiseQLError> for RestError {
    fn from(err: FraiseQLError) -> Self {
        match &err {
            FraiseQLError::NotFound { .. } => Self::not_found(err.to_string()),
            FraiseQLError::Validation { .. }
            | FraiseQLError::UnknownField { .. }
            | FraiseQLError::UnknownType { .. } => Self::bad_request(err.to_string()),
            FraiseQLError::Authorization { .. } => Self::forbidden(),
            FraiseQLError::Authentication { .. } => Self {
                status: StatusCode::UNAUTHORIZED,
                code: "UNAUTHENTICATED",
                message: "Authentication required".to_string(),
                details: None,
            },
            _ => Self::internal(err.to_string()),
        }
    }
}
