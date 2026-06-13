//! REST response and error types.
//!
//! [`RestResponse`] wraps HTTP status, headers, and optional body.
//! [`RestError`] maps [`FraiseQLError`] to HTTP status codes and structured
//! error responses.

use axum::http::StatusCode;
use fraiseql_error::FraiseQLError;
use serde_json::json;

use crate::error::{ClientInputSqlState, classify_client_input_sqlstate};

/// HTTP response with status, headers, and optional body.
pub struct RestResponse {
    /// HTTP status code.
    pub status:  StatusCode,
    /// Response headers.
    pub headers: axum::http::HeaderMap,
    /// Response body (None for 204 No Content).
    pub body:    Option<serde_json::Value>,
}

/// REST-specific error with HTTP status code.
#[derive(Debug)]
pub struct RestError {
    /// HTTP status code.
    pub status:  StatusCode,
    /// Error code string.
    pub code:    &'static str,
    /// Human-readable error message.
    pub message: String,
    /// Structured details for field-level errors.
    pub details: Option<serde_json::Value>,
}

impl RestError {
    /// 400 Bad Request.
    pub fn bad_request(message: impl Into<String>) -> Self {
        Self {
            status:  StatusCode::BAD_REQUEST,
            code:    "BAD_REQUEST",
            message: message.into(),
            details: None,
        }
    }

    /// 400 Bad Request — client-input data exception (SQLSTATE class 22, #413).
    pub fn bad_user_input(message: impl Into<String>) -> Self {
        Self {
            status:  StatusCode::BAD_REQUEST,
            code:    "BAD_USER_INPUT",
            message: message.into(),
            details: None,
        }
    }

    /// 400 Bad Request — integrity-constraint violation (SQLSTATE class 23, #413).
    pub fn constraint_violation(message: impl Into<String>) -> Self {
        Self {
            status:  StatusCode::BAD_REQUEST,
            code:    "CONSTRAINT_VIOLATION",
            message: message.into(),
            details: None,
        }
    }

    /// 403 Forbidden.
    #[must_use]
    pub fn forbidden() -> Self {
        Self {
            status:  StatusCode::FORBIDDEN,
            code:    "FORBIDDEN",
            message: "Access denied".to_string(),
            details: None,
        }
    }

    /// 404 Not Found.
    pub fn not_found(message: impl Into<String>) -> Self {
        Self {
            status:  StatusCode::NOT_FOUND,
            code:    "NOT_FOUND",
            message: message.into(),
            details: None,
        }
    }

    /// 422 Unprocessable Entity.
    pub fn unprocessable_entity(message: impl Into<String>, details: serde_json::Value) -> Self {
        Self {
            status:  StatusCode::UNPROCESSABLE_ENTITY,
            code:    "UNPROCESSABLE_ENTITY",
            message: message.into(),
            details: Some(details),
        }
    }

    /// 500 Internal Server Error.
    pub fn internal(message: impl Into<String>) -> Self {
        Self {
            status:  StatusCode::INTERNAL_SERVER_ERROR,
            code:    "INTERNAL_SERVER_ERROR",
            message: message.into(),
            details: None,
        }
    }

    /// Build a `RestError` whose HTTP status is the canonical
    /// [`FraiseQLError::status_code`] — the single source of truth shared with the
    /// GraphQL mapper (L-error-map-triplication). The `code` string is derived from
    /// that status. Used for every variant that does not need a bespoke message
    /// (e.g. `Conflict` → 409, `RateLimited` → 429, `ServiceUnavailable` → 503,
    /// `Unsupported` → 501), so no variant silently collapses to 500
    /// (M-rest-error-mapper). 5xx messages are scrubbed by the error sanitizer
    /// downstream.
    fn from_canonical_status(err: &FraiseQLError) -> Self {
        let status =
            StatusCode::from_u16(err.status_code()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
        let code = match status {
            StatusCode::CONFLICT => "CONFLICT",
            StatusCode::REQUEST_TIMEOUT => "REQUEST_TIMEOUT",
            StatusCode::TOO_MANY_REQUESTS => "RATE_LIMITED",
            StatusCode::NOT_IMPLEMENTED => "NOT_IMPLEMENTED",
            StatusCode::SERVICE_UNAVAILABLE => "SERVICE_UNAVAILABLE",
            StatusCode::BAD_REQUEST => "BAD_REQUEST",
            StatusCode::NOT_FOUND => "NOT_FOUND",
            StatusCode::UNAUTHORIZED => "UNAUTHENTICATED",
            StatusCode::FORBIDDEN => "FORBIDDEN",
            _ => "INTERNAL_SERVER_ERROR",
        };
        Self {
            status,
            code,
            message: err.to_string(),
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
                status:  StatusCode::UNAUTHORIZED,
                code:    "UNAUTHENTICATED",
                message: "Authentication required".to_string(),
                details: None,
            },
            // Client-input DB faults (SQLSTATE 22xxx/23xxx) are 400, not 500 (#413);
            // genuine server faults (other classes, no SQLSTATE, connection pool) fall
            // through to the canonical 500. Mirrors the GraphQL mapper. This is the one
            // documented place where the REST status intentionally diverges from
            // `status_code()` (which maps all `Database` to 500).
            FraiseQLError::Database { sql_state, .. } => {
                match classify_client_input_sqlstate(sql_state.as_deref()) {
                    Some(ClientInputSqlState::DataException) => {
                        Self::bad_user_input(err.to_string())
                    },
                    Some(ClientInputSqlState::IntegrityConstraint) => {
                        Self::constraint_violation(err.to_string())
                    },
                    None => Self::from_canonical_status(&err),
                }
            },
            // Every other variant derives its HTTP status from the canonical
            // `status_code()` — so `Conflict` → 409, `RateLimited` → 429,
            // `Timeout`/`Cancelled` → 408, `ServiceUnavailable` → 503,
            // `Unsupported` → 501, and any future variant get the correct status
            // instead of silently collapsing to 500 (M-rest-error-mapper).
            _ => Self::from_canonical_status(&err),
        }
    }
}
