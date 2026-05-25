use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Serialize;

use crate::{FileError, FraiseQLError};

/// Standardised JSON error body returned by all FraiseQL HTTP endpoints.
///
/// The shape follows the OAuth 2.0 error response convention so that clients
/// can handle errors uniformly regardless of which handler produced them.
///
/// Fields that are `None` are omitted from the serialised JSON to keep
/// responses compact.
#[derive(Debug, Serialize)]
#[non_exhaustive]
#[doc(hidden)] // Internal-pub: used by fraiseql-error's IntoResponse impl; adopters consume the JSON wire format, not the Rust type. Intentionally NOT re-exported from the umbrella crate.
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
            error_uri:         Some(format!("https://docs.fraiseql.dev/errors#{code}")),
            error_code:        code,
            details:           None,
            retry_after:       None,
        }
    }

    /// Attach arbitrary structured details to the response and return `self`.
    #[must_use]
    pub fn with_details(mut self, details: serde_json::Value) -> Self {
        self.details = Some(details);
        self
    }

    /// Set the `retry_after` field (in seconds) and return `self`.
    #[must_use]
    pub const fn with_retry_after(mut self, seconds: u64) -> Self {
        self.retry_after = Some(seconds);
        self
    }
}

impl IntoResponse for FraiseQLError {
    // Reason: the trailing `_` arm in the inner match intentionally duplicates
    // the `Internal` arm (silencing `match_same_arms`) and is currently
    // unreachable within `fraiseql-error` because every existing variant is
    // explicitly enumerated (silencing `unreachable_patterns`). The wildcard
    // becomes reachable as soon as a future variant is added to the
    // `#[non_exhaustive]` enum and gives that variant the safe generic
    // response. See IMPROVEMENTS.md F055.
    #[allow(clippy::match_same_arms, unreachable_patterns)]
    fn into_response(self) -> Response {
        let error_code = self.error_code();
        let http_status =
            StatusCode::from_u16(self.status_code()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);

        // SECURITY: messages here are deliberately generic — raw error
        // details (database messages, config values, provider endpoints,
        // internal paths) are kept server-side and surfaced only in
        // structured logs via the `Display`/`source` chain.
        let (error_category, description, retry_after) = match &self {
            FraiseQLError::Parse { .. }
            | FraiseQLError::Validation { .. }
            | FraiseQLError::UnknownField { .. }
            | FraiseQLError::UnknownType { .. } => ("validation_error", self.to_string(), None),
            FraiseQLError::Authentication { .. } | FraiseQLError::Auth(_) => {
                ("authentication_error", "Authentication failed".to_string(), None)
            },
            FraiseQLError::Authorization { .. } => {
                ("authorization_error", "Insufficient permissions".to_string(), None)
            },
            FraiseQLError::NotFound { .. } => ("not_found", "Resource not found".to_string(), None),
            FraiseQLError::Conflict { .. } => ("conflict", self.to_string(), None),
            FraiseQLError::RateLimited {
                retry_after_secs, ..
            } => ("rate_limited", "Rate limit exceeded".to_string(), Some(*retry_after_secs)),
            FraiseQLError::Timeout { .. } | FraiseQLError::Cancelled { .. } => {
                ("timeout", "Request timed out".to_string(), None)
            },
            FraiseQLError::ServiceUnavailable { retry_after, .. } => (
                "service_unavailable",
                "Service temporarily unavailable".to_string(),
                *retry_after,
            ),
            FraiseQLError::Unsupported { .. } => ("unsupported", self.to_string(), None),
            FraiseQLError::Webhook(_) => {
                ("webhook_error", "Webhook processing failed".to_string(), None)
            },
            FraiseQLError::File(e) => file_error_response(e),
            // SECURITY: database/config/storage/internal/observer details
            // must not leak to clients.
            FraiseQLError::Database { .. } => {
                ("database_error", "A database error occurred".to_string(), None)
            },
            FraiseQLError::ConnectionPool { .. } => {
                ("internal_error", "Connection pool exhausted".to_string(), None)
            },
            FraiseQLError::Configuration { .. } => {
                ("configuration_error", "A configuration error occurred".to_string(), None)
            },
            FraiseQLError::Observer(_) => {
                ("observer_error", "Observer processing failed".to_string(), None)
            },
            FraiseQLError::Internal { .. } => {
                ("internal_error", "An internal error occurred".to_string(), None)
            },
            // SECURITY: `FraiseQLError` is `#[non_exhaustive]`. A future
            // variant added without updating this match will silently fall
            // through to this generic 500 Internal Server Error response
            // instead of leaking implementation details (or, worse, returning
            // a misleading 2xx). `status_code()` and `error_code()` have
            // matching fallbacks — keep all three in lockstep.
            _ => ("internal_error", "An internal error occurred".to_string(), None),
        };

        let mut body = ErrorResponse::new(error_category, description, error_code);
        if let Some(secs) = retry_after {
            body = body.with_retry_after(secs);
        }

        let mut resp = (http_status, Json(body)).into_response();
        if let Some(secs) = retry_after {
            if let Ok(value) = secs.to_string().parse() {
                resp.headers_mut().insert("Retry-After", value);
            }
        }
        resp
    }
}

/// Return the (category, message, `retry_after`) tuple for a [`FileError`].
///
/// `TooLarge` is the only variant whose message is safe to forward to the
/// client (the size limits help the caller correct the request).
// Reason: `FileError` is `#[non_exhaustive]`; the trailing `_` arm is the
// security fallback for any future variant added without updating this match.
// The in-crate match currently enumerates every variant, so the wildcard is
// unreachable today (`unreachable_patterns`) and structurally duplicates the
// generic arms (`match_same_arms`). Same defence-in-depth rationale as the
// `FraiseQLError::IntoResponse` and `status_code` matches in this file.
#[allow(clippy::match_same_arms, unreachable_patterns)]
fn file_error_response(e: &FileError) -> (&'static str, String, Option<u64>) {
    match e {
        FileError::TooLarge { size, max } => (
            "file_error",
            format!("File too large: {size} bytes exceeds maximum {max}"),
            None,
        ),
        FileError::InvalidType { .. } | FileError::MimeMismatch { .. } => {
            ("file_error", "Unsupported file type".to_string(), None)
        },
        FileError::NotFound { .. } => ("file_error", "File not found".to_string(), None),
        FileError::VirusDetected { .. } => {
            ("file_error", "File failed security scan".to_string(), None)
        },
        FileError::QuotaExceeded => ("file_error", "Storage quota exceeded".to_string(), None),
        FileError::Storage { .. } | FileError::Processing { .. } => {
            ("file_error", "File operation failed".to_string(), None)
        },
        // F050 backend-classification variants. Bodies are deliberately generic
        // (the typed status-code routing happens in `FraiseQLError::status_code`).
        FileError::PermissionDenied { .. } => ("file_error", "Permission denied".to_string(), None),
        FileError::IoError { .. } | FileError::Backend { .. } => {
            ("file_error", "Storage backend error".to_string(), None)
        },
        FileError::InvalidKey { .. } => ("file_error", "Invalid storage key".to_string(), None),
        FileError::NotImplemented { .. } | FileError::Unsupported { .. } => {
            ("file_error", "Operation not supported".to_string(), None)
        },
        FileError::SizeLimitExceeded { .. } => {
            ("file_error", "Upload exceeds size limit".to_string(), None)
        },
        FileError::MimeTypeNotAllowed { .. } => {
            ("file_error", "Content type not allowed".to_string(), None)
        },
        // SECURITY: `FileError` is `#[non_exhaustive]`. A future variant added
        // without updating this match falls through to the generic file-error
        // response.
        _ => ("file_error", "File operation failed".to_string(), None),
    }
}

/// Convenience trait that allows returning `Result<T, FraiseQLError>` from axum
/// handlers by converting it directly into an HTTP [`Response`].
///
/// Import this trait and call `.into_http_response()` on any
/// `Result<impl IntoResponse, FraiseQLError>` value.
pub trait IntoHttpResponse {
    /// Convert this result into an axum [`Response`].
    ///
    /// On success the inner value is serialised via its own [`IntoResponse`]
    /// implementation. On error the [`FraiseQLError`] is converted to a JSON
    /// error body with the appropriate HTTP status code.
    fn into_http_response(self) -> Response;
}

impl<T> IntoHttpResponse for std::result::Result<T, FraiseQLError>
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
