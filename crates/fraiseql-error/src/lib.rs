//! Unified error types for FraiseQL runtime crates.
//!
//! All runtime crates depend on this crate for error handling.
//!
//! # Error bridging contract
//!
//! [`RuntimeError`] is the domain-level error enum that aggregates all business-logic errors
//! (auth, webhooks, files, notifications, etc.). It implements [`axum::response::IntoResponse`]
//! via `IntoResponse`, which converts it to an [`ErrorResponse`] JSON body with the
//! appropriate HTTP status code:
//!
//! ```text
//! RuntimeError (domain)
//!     ↓  IntoResponse (via fraiseql-error::http)
//! ErrorResponse { error, error_description, error_code, error_uri, details, retry_after }
//!     ↓  Json(response) + StatusCode
//! HTTP response body (application/json)
//! ```
//!
//! ## Mapping rules
//!
//! | `RuntimeError` variant            | HTTP status                  |
//! |-----------------------------------|------------------------------|
//! | `Auth(InsufficientPermissions)`   | 403 Forbidden                |
//! | `Auth(*)`                         | 401 Unauthorized             |
//! | `Webhook(InvalidSignature)`       | 401 Unauthorized             |
//! | `RateLimited`                     | 429 Too Many Requests        |
//! | `ServiceUnavailable`              | 503 Service Unavailable      |
//! | `NotFound`                        | 404 Not Found                |
//! | `Database`                        | 500 Internal Server Error    |
//! | `Config` / `Internal`             | 500 Internal Server Error    |
//!
//! ## Security note
//!
//! All variants that might leak internal details (database messages, config values,
//! provider endpoints) return **generic** descriptions in the HTTP response body.
//! Raw error details are available only in structured server logs.

mod auth;
mod config;
pub mod core_error;
mod file;
#[cfg(feature = "axum-compat")]
mod http;
mod integration;
mod notification;
mod observer;
mod webhook;

pub use auth::AuthError;
pub use config::ConfigError;
pub use core_error::{ErrorContext, FraiseQLError, Result, ValidationFieldError};
pub use file::FileError;
// Re-export for convenience — only available with the `axum-compat` feature
#[cfg(feature = "axum-compat")]
pub use http::{ErrorResponse, IntoHttpResponse};
pub use integration::IntegrationError;
pub use notification::NotificationError;
pub use observer::ObserverError;
pub use webhook::WebhookError;

/// Unified error type wrapping all domain errors.
///
/// `RuntimeError` aggregates every domain-level error that can surface during
/// request handling. It implements [`axum::response::IntoResponse`] so that
/// handlers can return `Result<_, RuntimeError>` directly; the conversion
/// produces an [`ErrorResponse`] JSON body with the appropriate HTTP status
/// code. Sensitive internal details (database messages, config values) are
/// stripped from the HTTP response and are only present in server-side logs.
#[derive(Debug, thiserror::Error)]
pub enum RuntimeError {
    /// A configuration error, such as an invalid or missing config file.
    #[error(transparent)]
    Config(#[from] ConfigError),

    /// An authentication or authorisation error (invalid token, expired session, etc.).
    #[error(transparent)]
    Auth(#[from] AuthError),

    /// A webhook validation error (bad signature, duplicate event, expired timestamp, etc.).
    #[error(transparent)]
    Webhook(#[from] WebhookError),

    /// A file-handling error (size limit exceeded, unsupported type, virus detected, etc.).
    #[error(transparent)]
    File(#[from] FileError),

    /// A notification delivery error (provider unavailable, circuit open, rate-limited, etc.).
    #[error(transparent)]
    Notification(#[from] NotificationError),

    /// An observer/event processing error (invalid condition, action failure, etc.).
    #[error(transparent)]
    Observer(#[from] ObserverError),

    /// An external integration error (search, cache, queue, or connection failure).
    #[error(transparent)]
    Integration(#[from] IntegrationError),

    /// A database-level error propagated from sqlx.
    ///
    /// The raw sqlx message is available for logging but is never exposed in
    /// HTTP responses (returns a generic "database error" description instead).
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    /// The caller has exceeded the configured request rate limit.
    ///
    /// `retry_after` is the number of seconds to wait before retrying, if known.
    #[error("Rate limit exceeded")]
    RateLimited {
        /// Number of seconds to wait before retrying, if known.
        retry_after: Option<u64>,
    },

    /// A downstream service or dependency is temporarily unavailable.
    ///
    /// `retry_after` is the number of seconds to wait before retrying, if known.
    #[error("Service unavailable: {reason}")]
    ServiceUnavailable {
        /// Human-readable reason for the outage (server-side logs only).
        reason:      String,
        /// Number of seconds to wait before retrying, if known.
        retry_after: Option<u64>,
    },

    /// The requested resource does not exist.
    #[error("Resource not found: {resource}")]
    NotFound {
        /// Description of the resource that was not found.
        resource: String,
    },

    /// An unexpected internal server error.
    ///
    /// Use this variant when no more specific variant applies. The `message`
    /// is recorded in server logs but is never forwarded to clients.
    #[error("Internal error: {message}")]
    Internal {
        /// Internal error message (not forwarded to clients).
        message: String,
        /// Optional chained error for structured logging.
        #[source]
        source:  Option<Box<dyn std::error::Error + Send + Sync>>,
    },
}

impl RuntimeError {
    /// Get the error code for this error
    pub const fn error_code(&self) -> &'static str {
        match self {
            Self::Config(e) => e.error_code(),
            Self::Auth(e) => e.error_code(),
            Self::Webhook(e) => e.error_code(),
            Self::File(e) => e.error_code(),
            Self::Notification(e) => e.error_code(),
            Self::Observer(e) => e.error_code(),
            Self::Integration(e) => e.error_code(),
            Self::Database(_) => "database_error",
            Self::RateLimited { .. } => "rate_limited",
            Self::ServiceUnavailable { .. } => "service_unavailable",
            Self::NotFound { .. } => "not_found",
            Self::Internal { .. } => "internal_error",
        }
    }

    /// Get documentation URL for this error
    pub fn docs_url(&self) -> String {
        format!("https://docs.fraiseql.dev/errors#{}", self.error_code())
    }
}
