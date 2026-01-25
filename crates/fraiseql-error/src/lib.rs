//! Unified error types for FraiseQL runtime crates.
//!
//! All runtime crates depend on this crate for error handling.

// Error variants and fields are self-documenting via their #[error(...)] messages
#![allow(missing_docs)]

mod auth;
mod config;
mod file;
mod http;
mod integration;
mod notification;
mod observer;
mod webhook;

pub use auth::AuthError;
pub use config::ConfigError;
pub use file::FileError;
// Re-export for convenience
pub use http::{ErrorResponse, IntoHttpResponse};
pub use integration::IntegrationError;
pub use notification::NotificationError;
pub use observer::ObserverError;
pub use webhook::WebhookError;

/// Unified error type wrapping all domain errors
#[derive(Debug, thiserror::Error)]
pub enum RuntimeError {
    #[error(transparent)]
    Config(#[from] ConfigError),

    #[error(transparent)]
    Auth(#[from] AuthError),

    #[error(transparent)]
    Webhook(#[from] WebhookError),

    #[error(transparent)]
    File(#[from] FileError),

    #[error(transparent)]
    Notification(#[from] NotificationError),

    #[error(transparent)]
    Observer(#[from] ObserverError),

    #[error(transparent)]
    Integration(#[from] IntegrationError),

    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Rate limit exceeded")]
    RateLimited { retry_after: Option<u64> },

    #[error("Service unavailable: {reason}")]
    ServiceUnavailable {
        reason:      String,
        retry_after: Option<u64>,
    },

    #[error("Resource not found: {resource}")]
    NotFound { resource: String },

    #[error("Internal error: {message}")]
    Internal {
        message: String,
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
