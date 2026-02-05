//! # FraiseQL Webhook Runtime
//!
//! Webhook processing with signature verification, idempotency, and event routing.
//!
//! ## Features
//!
//! - **15+ provider support**: Stripe, GitHub, Shopify, and more
//! - **Signature verification**: Constant-time comparison for security
//! - **Idempotency**: Prevent duplicate event processing
//! - **Event routing**: Map webhook events to database functions
//! - **Transaction boundaries**: Correct isolation levels for data consistency

pub mod config;
pub mod signature;
pub mod testing;
pub mod traits;
pub mod transaction;

// Re-exports
pub use config::{WebhookConfig, WebhookEventConfig};
pub use signature::SignatureError;
// Re-export testing mocks for tests
#[cfg(test)]
pub use testing::mocks;
// Also export mocks for integration tests (tests/ directory)
#[cfg(not(test))]
pub use testing::mocks;
pub use traits::{Clock, EventHandler, IdempotencyStore, SecretProvider, SignatureVerifier};
pub use transaction::{WebhookIsolation, execute_in_transaction};

/// Webhook-specific errors
#[derive(Debug, thiserror::Error)]
pub enum WebhookError {
    #[error("Missing signature header")]
    MissingSignature,

    #[error("Invalid signature format: {0}")]
    InvalidSignature(String),

    #[error("Signature verification failed")]
    SignatureVerificationFailed,

    #[error("Timestamp expired (received: {received}, now: {now}, tolerance: {tolerance}s)")]
    TimestampExpired {
        received:  i64,
        now:       i64,
        tolerance: u64,
    },

    #[error("Missing timestamp header")]
    MissingTimestamp,

    #[error("Missing webhook secret: {0}")]
    MissingSecret(String),

    #[error("Unknown webhook provider: {0}")]
    UnknownProvider(String),

    #[error("Unknown event type: {0}")]
    UnknownEvent(String),

    #[error("Invalid payload: {0}")]
    InvalidPayload(String),

    #[error("Handler execution failed: {0}")]
    HandlerFailed(String),

    #[error("Database error: {0}")]
    Database(String),

    #[error("Condition evaluation error: {0}")]
    Condition(String),

    #[error("Mapping error: {0}")]
    Mapping(String),

    #[error("Provider not configured: {0}")]
    ProviderNotConfigured(String),
}

impl From<sqlx::Error> for WebhookError {
    fn from(err: sqlx::Error) -> Self {
        Self::Database(err.to_string())
    }
}

impl From<serde_json::Error> for WebhookError {
    fn from(err: serde_json::Error) -> Self {
        Self::InvalidPayload(err.to_string())
    }
}

/// Result type for webhook operations
pub type Result<T> = std::result::Result<T, WebhookError>;
