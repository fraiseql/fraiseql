//! # fraiseql-webhooks
//!
//! **Inbound** webhook receiver for FraiseQL — receives, verifies, and routes HTTP
//! callbacks from third-party services into your database.
//!
//! ## Inbound vs. Outbound
//!
//! FraiseQL has two webhook-related crates with complementary roles:
//!
//! | Crate | Direction | Purpose |
//! |-------|-----------|---------|
//! | `fraiseql-webhooks` | **Inbound** | Receive callbacks from Stripe, GitHub, Shopify, … |
//! | `fraiseql-observers` | **Outbound** | Emit notifications when your data changes |
//!
//! Use `fraiseql-webhooks` when you want to react to events from external providers.
//! Use `fraiseql-observers` when you want to push events to subscribers.
//!
//! ## Supported Providers
//!
//! Built-in signature verification for:
//! - **Stripe** — HMAC-SHA256 on `Stripe-Signature` header with replay protection
//! - **GitHub** — HMAC-SHA256 on `X-Hub-Signature-256` header
//! - **Shopify** — HMAC-SHA256 on `X-Shopify-Hmac-Sha256` header
//! - **SendGrid** — ECDSA on `X-Twilio-Email-Event-Webhook-Signature`
//! - **Paddle** — RSA-SHA256 on `Paddle-Signature` header
//! - Custom providers via the [`traits::WebhookProvider`] trait
//!
//! ## Security Properties
//!
//! - **Constant-time comparison** — prevents timing attacks on HMAC tokens
//! - **Replay protection** — Stripe/Paddle timestamps validated within a 5-minute window
//! - **Idempotency** — duplicate delivery is detected and silently discarded
//! - **Transaction boundaries** — each webhook is processed inside a database transaction
//!
//! ## Quick Start
//!
//! ```no_run
//! // See docs/architecture/webhooks.md for the full integration guide.
//! ```
//!
//! ## See Also
//!
//! - [`docs/architecture/webhooks.md`](../../../../docs/architecture/webhooks.md) — full design doc
//! - `fraiseql-observers` — outbound change notifications
//!
//! ## Features

#![forbid(unsafe_code)]
// module_name_repetitions, must_use_candidate, uninlined_format_args:
// allowed at workspace level (Cargo.toml [workspace.lints.clippy]).
#![allow(clippy::missing_errors_doc)] // Reason: error types are self-documenting
#![allow(clippy::missing_panics_doc)] // Reason: panics eliminated by design
#![allow(clippy::doc_markdown)] // Reason: technical terms don't need backtick wrapping
#![allow(clippy::struct_field_names)] // Reason: field prefixes match domain terminology
#![allow(clippy::wildcard_imports)] // Reason: test modules use wildcard imports
#![allow(clippy::items_after_statements)] // Reason: helper structs near point of use in tests
#![allow(clippy::missing_const_for_fn)] // Reason: const fn not stable for all patterns used
#![allow(clippy::cast_possible_wrap)] // Reason: values are within i64 range by design
#![allow(clippy::redundant_clone)] // Reason: explicit clone at API boundaries for clarity
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
// Re-export testing mocks for unit tests and integration tests with `testing` feature
#[cfg(any(test, feature = "testing"))]
pub use testing::mocks;
pub use traits::{Clock, EventHandler, IdempotencyStore, SecretProvider, SignatureVerifier};
pub use transaction::{WebhookIsolation, execute_in_transaction};

/// Errors that can occur during webhook request processing.
#[derive(Debug, thiserror::Error)]
pub enum WebhookError {
    /// The incoming request did not include the expected signature header for the provider.
    #[error("Missing signature header")]
    MissingSignature,

    /// The signature header was present but could not be parsed according to the provider's format.
    /// The inner string contains a description of the parse failure.
    #[error("Invalid signature format: {0}")]
    InvalidSignature(String),

    /// The computed HMAC or asymmetric signature did not match the value in the request header.
    #[error("Signature verification failed")]
    SignatureVerificationFailed,

    /// The webhook request timestamp is outside the configured replay-protection tolerance window.
    #[error("Timestamp expired (received: {received}, now: {now}, tolerance: {tolerance}s)")]
    TimestampExpired {
        /// Unix timestamp (seconds) extracted from the request.
        received:  i64,
        /// Unix timestamp (seconds) at the time of verification.
        now:       i64,
        /// Maximum allowed age of a request in seconds before it is rejected.
        tolerance: u64,
    },

    /// The provider requires a timestamp header for replay protection, but none was present.
    #[error("Missing timestamp header")]
    MissingTimestamp,

    /// The secret named in the configuration could not be retrieved from the secret provider.
    /// The inner string is the secret name that was not found.
    #[error("Missing webhook secret: {0}")]
    MissingSecret(String),

    /// The request arrived for a provider name that is not registered in the `ProviderRegistry`.
    /// The inner string is the unrecognised provider name.
    #[error("Unknown webhook provider: {0}")]
    UnknownProvider(String),

    /// The event type extracted from the payload has no corresponding handler in the configuration.
    /// The inner string is the unrecognised event type.
    #[error("Unknown event type: {0}")]
    UnknownEvent(String),

    /// The request body could not be deserialised as a valid JSON payload.
    /// The inner string is the serde_json error message.
    #[error("Invalid payload: {0}")]
    InvalidPayload(String),

    /// The database function called by the event handler returned an error or panicked.
    /// The inner string contains the handler's error message.
    #[error("Handler execution failed: {0}")]
    HandlerFailed(String),

    /// A sqlx database operation failed during transaction management or idempotency checking.
    /// The inner string is the sqlx error message.
    #[error("Database error: {0}")]
    Database(String),

    /// Evaluation of a configured conditional expression failed.
    /// The inner string describes the evaluation error.
    #[error("Condition evaluation error: {0}")]
    Condition(String),

    /// A field mapping from the webhook payload to a function parameter failed.
    /// The inner string describes which mapping could not be applied.
    #[error("Mapping error: {0}")]
    Mapping(String),

    /// A webhook was received for a provider that has no entry in the active configuration.
    /// The inner string is the provider name.
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
