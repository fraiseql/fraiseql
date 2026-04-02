use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Serialize;

use crate::{AuthError, FileError, RuntimeError, WebhookError};

/// Standardised JSON error body returned by all FraiseQL HTTP endpoints.
///
/// The shape follows the OAuth 2.0 error response convention so that clients
/// can handle errors uniformly regardless of which handler produced them.
///
/// Fields that are `None` are omitted from the serialised JSON to keep
/// responses compact.
#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    /// Short machine-readable error category (e.g. `"authentication_error"`).
    pub error:             String,
    /// Human-readable description safe to display to end-users.
    pub error_description: String,
    /// Stable, fine-grained error code for programmatic handling (e.g.
    /// `"token_expired"`).
    pub error_code:        String,
    /// URL to the documentation page for this error code, if available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_uri:         Option<String>,
    /// Additional structured details about the error (omitted when `None`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details:           Option<serde_json::Value>,
    /// Number of seconds the client should wait before retrying, if applicable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_after:       Option<u64>,
}

impl ErrorResponse {
    /// Construct a minimal error response with the three required fields.
    ///
    /// `error_uri` is populated automatically from `code` using the FraiseQL
    /// documentation base URL. `details` and `retry_after` are `None`.
    pub fn new(
        error: impl Into<String>,
        description: impl Into<String>,
        code: impl Into<String>,
    ) -> Self {
        let code = code.into();
        Self {
            error:             error.into(),
            error_description: description.into(),
            error_uri:         Some(format!("https://docs.fraiseql.dev/errors#{}", code)),
            error_code:        code,
            details:           None,
            retry_after:       None,
        }
    }

    /// Attach arbitrary structured details to the response and return `self`.
    pub fn with_details(mut self, details: serde_json::Value) -> Self {
        self.details = Some(details);
        self
    }

    /// Set the `retry_after` field (in seconds) and return `self`.
    pub const fn with_retry_after(mut self, seconds: u64) -> Self {
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
                // SECURITY: Config errors may contain connection strings or secrets.
                // Return a generic message; details are in server logs only.
                ErrorResponse::new(
                    "configuration_error",
                    "A configuration error occurred",
                    error_code,
                ),
            ),

            RuntimeError::Auth(e) => {
                let (status, msg) = match e {
                    AuthError::InsufficientPermissions { .. } => {
                        (StatusCode::FORBIDDEN, "Insufficient permissions")
                    },
                    AuthError::AccountLocked { .. } => (StatusCode::FORBIDDEN, "Account locked"),
                    AuthError::InvalidCredentials => {
                        (StatusCode::UNAUTHORIZED, "Invalid credentials")
                    },
                    AuthError::TokenExpired => (StatusCode::UNAUTHORIZED, "Token expired"),
                    // SECURITY: InvalidToken, ProviderError messages may contain internal details
                    // (JWT parsing reasons, provider endpoint URLs). Return generic message.
                    AuthError::InvalidToken { .. } | AuthError::ProviderError { .. } => {
                        (StatusCode::UNAUTHORIZED, "Authentication failed")
                    },
                    AuthError::InvalidState => (StatusCode::UNAUTHORIZED, "Invalid OAuth state"),
                    AuthError::UserDenied => {
                        (StatusCode::UNAUTHORIZED, "User denied authorization")
                    },
                    AuthError::SessionNotFound | AuthError::SessionExpired => {
                        (StatusCode::UNAUTHORIZED, "Session not found or expired")
                    },
                    AuthError::RefreshTokenInvalid => {
                        (StatusCode::UNAUTHORIZED, "Refresh token invalid or expired")
                    },
                };
                (status, ErrorResponse::new("authentication_error", msg, error_code))
            },

            RuntimeError::Webhook(e) => {
                let (status, msg) = match e {
                    WebhookError::InvalidSignature => {
                        (StatusCode::UNAUTHORIZED, "Invalid webhook signature")
                    },
                    WebhookError::DuplicateEvent { .. } => (StatusCode::OK, "Duplicate event"),
                    WebhookError::TimestampExpired { .. } => {
                        (StatusCode::BAD_REQUEST, "Webhook timestamp expired — check your clock")
                    },
                    WebhookError::TimestampFuture { .. } => {
                        (StatusCode::BAD_REQUEST, "Webhook timestamp is in the future")
                    },
                    WebhookError::MissingSignature { .. } => {
                        (StatusCode::BAD_REQUEST, "Missing webhook signature header")
                    },
                    WebhookError::UnknownEvent { .. } => {
                        (StatusCode::BAD_REQUEST, "Unknown webhook event type")
                    },
                    WebhookError::ProviderNotConfigured { .. } => {
                        (StatusCode::BAD_REQUEST, "Webhook provider not configured")
                    },
                    // SECURITY: PayloadError and IdempotencyError messages may contain
                    // internal parsing details. Return generic messages.
                    WebhookError::PayloadError { .. } | WebhookError::IdempotencyError { .. } => {
                        (StatusCode::BAD_REQUEST, "Webhook processing failed")
                    },
                };
                (status, ErrorResponse::new("webhook_error", msg, error_code))
            },

            RuntimeError::File(e) => {
                let (status, msg) = match e {
                    FileError::TooLarge { size, max } => (
                        StatusCode::PAYLOAD_TOO_LARGE,
                        // Safe to expose size info — helps client fix the request.
                        format!("File too large: {} bytes exceeds maximum {}", size, max),
                    ),
                    FileError::InvalidType { .. } | FileError::MimeMismatch { .. } => {
                        (StatusCode::UNSUPPORTED_MEDIA_TYPE, "Unsupported file type".to_string())
                    },
                    FileError::NotFound { .. } => {
                        // SECURITY: Do not expose internal file paths.
                        (StatusCode::NOT_FOUND, "File not found".to_string())
                    },
                    FileError::VirusDetected { .. } => {
                        (StatusCode::UNPROCESSABLE_ENTITY, "File failed security scan".to_string())
                    },
                    FileError::QuotaExceeded => {
                        (StatusCode::INSUFFICIENT_STORAGE, "Storage quota exceeded".to_string())
                    },
                    _ => (StatusCode::BAD_REQUEST, "File operation failed".to_string()),
                };
                (status, ErrorResponse::new("file_error", msg, error_code))
            },

            RuntimeError::Notification(e) => {
                use crate::NotificationError::{
                    CircuitOpen, InvalidInput, ProviderRateLimited, ProviderUnavailable,
                };
                let status = match e {
                    CircuitOpen { .. } | ProviderUnavailable { .. } => {
                        StatusCode::SERVICE_UNAVAILABLE
                    },
                    ProviderRateLimited { .. } => StatusCode::TOO_MANY_REQUESTS,
                    InvalidInput { .. } => StatusCode::BAD_REQUEST,
                    _ => StatusCode::INTERNAL_SERVER_ERROR,
                };
                // SECURITY: Provider details (API keys, endpoints) must not appear in responses.
                let msg = match e {
                    InvalidInput { .. } => self.to_string(),
                    _ => "Notification service unavailable".to_string(),
                };
                (status, ErrorResponse::new("notification_error", msg, error_code))
            },

            RuntimeError::RateLimited { retry_after } => {
                let mut resp =
                    ErrorResponse::new("rate_limited", "Rate limit exceeded", error_code);
                if let Some(secs) = retry_after {
                    resp = resp.with_retry_after(*secs);
                }
                (StatusCode::TOO_MANY_REQUESTS, resp)
            },

            RuntimeError::ServiceUnavailable { retry_after, .. } => {
                // SECURITY: ServiceUnavailable may contain internal service names or endpoints.
                // Return a generic message; details are in server logs only.
                let mut resp = ErrorResponse::new(
                    "service_unavailable",
                    "Service temporarily unavailable",
                    error_code,
                );
                if let Some(secs) = retry_after {
                    resp = resp.with_retry_after(*secs);
                }
                (StatusCode::SERVICE_UNAVAILABLE, resp)
            },

            RuntimeError::NotFound { .. } => (
                StatusCode::NOT_FOUND,
                // SECURITY: Do not expose internal resource names or IDs.
                ErrorResponse::new("not_found", "Resource not found", error_code),
            ),

            RuntimeError::Database(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                ErrorResponse::new("database_error", "A database error occurred", error_code),
            ),

            _ => (
                StatusCode::INTERNAL_SERVER_ERROR,
                ErrorResponse::new("internal_error", "An internal error occurred", error_code),
            ),
        };

        // Add Retry-After header for rate limits
        let mut resp = (status, Json(response)).into_response();
        if let Some(retry_after) = self.retry_after_header() {
            if let Ok(value) = retry_after.parse() {
                resp.headers_mut().insert("Retry-After", value);
            }
        }

        resp
    }
}

impl RuntimeError {
    fn retry_after_header(&self) -> Option<String> {
        match self {
            Self::RateLimited {
                retry_after: Some(secs),
            }
            | Self::ServiceUnavailable {
                retry_after: Some(secs),
                ..
            } => Some(secs.to_string()),
            _ => None,
        }
    }
}

/// Convenience trait that allows returning `Result<T, RuntimeError>` from axum
/// handlers by converting it directly into an HTTP [`Response`].
///
/// Import this trait and call `.into_http_response()` on any
/// `Result<impl IntoResponse, RuntimeError>` value.
pub trait IntoHttpResponse {
    /// Convert this result into an axum [`Response`].
    ///
    /// On success the inner value is serialised via its own [`IntoResponse`]
    /// implementation. On error the [`RuntimeError`] is converted to a JSON
    /// error body with the appropriate HTTP status code.
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
