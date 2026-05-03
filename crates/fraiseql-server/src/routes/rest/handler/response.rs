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
            _ => Self::internal(err.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rest_error_bad_request() {
        let err = RestError::bad_request("test message");
        assert_eq!(err.status, StatusCode::BAD_REQUEST);
        assert_eq!(err.code, "BAD_REQUEST");
        assert_eq!(err.message, "test message");
    }

    #[test]
    fn rest_error_forbidden() {
        let err = RestError::forbidden();
        assert_eq!(err.status, StatusCode::FORBIDDEN);
        assert_eq!(err.code, "FORBIDDEN");
    }

    #[test]
    fn rest_error_not_found() {
        let err = RestError::not_found("resource not found");
        assert_eq!(err.status, StatusCode::NOT_FOUND);
        assert_eq!(err.code, "NOT_FOUND");
    }

    #[test]
    fn rest_error_unprocessable_entity() {
        let details = json!({"field": "name"});
        let err = RestError::unprocessable_entity("invalid entity", details.clone());
        assert_eq!(err.status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(err.details, Some(details));
    }

    #[test]
    fn rest_error_internal() {
        let err = RestError::internal("internal error");
        assert_eq!(err.status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(err.code, "INTERNAL_SERVER_ERROR");
    }

    #[test]
    fn rest_error_to_json() {
        let err = RestError::bad_request("test error");
        let json = err.to_json();
        assert_eq!(json["error"]["code"], "BAD_REQUEST");
        assert_eq!(json["error"]["message"], "test error");
    }

    #[test]
    fn rest_error_to_json_with_details() {
        let details = json!({"field": "email"});
        let err = RestError::unprocessable_entity("validation error", details.clone());
        let json = err.to_json();
        assert_eq!(json["error"]["details"], details);
    }
}
