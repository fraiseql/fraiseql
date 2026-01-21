use axum::{
    response::{IntoResponse, Response},
    http::StatusCode,
    Json,
};
use serde::Serialize;

use crate::{RuntimeError, AuthError, WebhookError, FileError};

/// Error response format (consistent across all endpoints)
#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: String,
    pub error_description: String,
    pub error_code: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_uri: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_after: Option<u64>,
}

impl ErrorResponse {
    pub fn new(error: impl Into<String>, description: impl Into<String>, code: impl Into<String>) -> Self {
        let code = code.into();
        Self {
            error: error.into(),
            error_description: description.into(),
            error_uri: Some(format!("https://docs.fraiseql.dev/errors#{}", code)),
            error_code: code,
            details: None,
            retry_after: None,
        }
    }

    pub fn with_details(mut self, details: serde_json::Value) -> Self {
        self.details = Some(details);
        self
    }

    pub fn with_retry_after(mut self, seconds: u64) -> Self {
        self.retry_after = Some(seconds);
        self
    }
}

impl IntoResponse for RuntimeError {
    fn into_response(self) -> Response {
        let error_code = self.error_code();

        let (status, response) = match &self {
            RuntimeError::Config(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                ErrorResponse::new("configuration_error", self.to_string(), error_code)
            ),

            RuntimeError::Auth(e) => {
                let status = match e {
                    AuthError::InsufficientPermissions { .. } => StatusCode::FORBIDDEN,
                    AuthError::AccountLocked { .. } => StatusCode::FORBIDDEN,
                    _ => StatusCode::UNAUTHORIZED,
                };
                (status, ErrorResponse::new("authentication_error", self.to_string(), error_code))
            },

            RuntimeError::Webhook(e) => {
                let status = match e {
                    WebhookError::InvalidSignature => StatusCode::UNAUTHORIZED,
                    WebhookError::MissingSignature { .. } => StatusCode::BAD_REQUEST,
                    WebhookError::DuplicateEvent { .. } => StatusCode::OK,
                    _ => StatusCode::BAD_REQUEST,
                };
                (status, ErrorResponse::new("webhook_error", self.to_string(), error_code))
            },

            RuntimeError::File(e) => {
                let status = match e {
                    FileError::TooLarge { .. } => StatusCode::PAYLOAD_TOO_LARGE,
                    FileError::InvalidType { .. } | FileError::MimeMismatch { .. } => {
                        StatusCode::UNSUPPORTED_MEDIA_TYPE
                    }
                    FileError::NotFound { .. } => StatusCode::NOT_FOUND,
                    FileError::VirusDetected { .. } => StatusCode::UNPROCESSABLE_ENTITY,
                    FileError::QuotaExceeded => StatusCode::INSUFFICIENT_STORAGE,
                    _ => StatusCode::BAD_REQUEST,
                };
                (status, ErrorResponse::new("file_error", self.to_string(), error_code))
            },

            RuntimeError::Notification(e) => {
                use crate::NotificationError::*;
                let status = match e {
                    CircuitOpen { .. } | ProviderUnavailable { .. } => {
                        StatusCode::SERVICE_UNAVAILABLE
                    }
                    ProviderRateLimited { .. } => StatusCode::TOO_MANY_REQUESTS,
                    InvalidInput { .. } => StatusCode::BAD_REQUEST,
                    _ => StatusCode::INTERNAL_SERVER_ERROR,
                };
                (status, ErrorResponse::new("notification_error", self.to_string(), error_code))
            },

            RuntimeError::RateLimited { retry_after } => {
                let mut resp = ErrorResponse::new(
                    "rate_limited",
                    "Rate limit exceeded",
                    error_code
                );
                if let Some(secs) = retry_after {
                    resp = resp.with_retry_after(*secs);
                }
                (StatusCode::TOO_MANY_REQUESTS, resp)
            },

            RuntimeError::ServiceUnavailable { retry_after, .. } => {
                let mut resp = ErrorResponse::new(
                    "service_unavailable",
                    self.to_string(),
                    error_code
                );
                if let Some(secs) = retry_after {
                    resp = resp.with_retry_after(*secs);
                }
                (StatusCode::SERVICE_UNAVAILABLE, resp)
            },

            RuntimeError::NotFound { .. } => (
                StatusCode::NOT_FOUND,
                ErrorResponse::new("not_found", self.to_string(), error_code)
            ),

            RuntimeError::Database(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                ErrorResponse::new("database_error", "A database error occurred", error_code)
            ),

            _ => (
                StatusCode::INTERNAL_SERVER_ERROR,
                ErrorResponse::new("internal_error", "An internal error occurred", error_code)
            ),
        };

        // Add Retry-After header for rate limits
        let mut resp = (status, Json(response)).into_response();
        if let Some(retry_after) = self.retry_after_header() {
            resp.headers_mut().insert(
                "Retry-After",
                retry_after.parse().unwrap()
            );
        }

        resp
    }
}

impl RuntimeError {
    fn retry_after_header(&self) -> Option<String> {
        match self {
            Self::RateLimited { retry_after: Some(secs) } => Some(secs.to_string()),
            Self::ServiceUnavailable { retry_after: Some(secs), .. } => Some(secs.to_string()),
            _ => None,
        }
    }
}

/// Trait to enable `?` operator in handlers
pub trait IntoHttpResponse {
    fn into_http_response(self) -> Response;
}

impl<T> IntoHttpResponse for Result<T, RuntimeError>
where
    T: IntoResponse,
{
    fn into_http_response(self) -> Response {
        match self {
            Ok(value) => value.into_response(),
            Err(err) => err.into_response(),
        }
    }
}
