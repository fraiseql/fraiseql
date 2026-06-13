//! # fraiseql-webhooks
//!
//! **Building blocks** for verifying inbound webhook signatures from third-party
//! services. This crate provides constant-time signature verification and a
//! [`traits::SignatureVerifier`] abstraction; it does **not** ship a turnkey
//! inbound receiver pipeline.
//!
//! ## What this crate provides
//!
//! - Per-provider signature verifiers ([`signature`]) returning `Ok(bool)` /
//!   [`signature::SignatureError`].
//! - The [`traits::SignatureVerifier`] trait so callers can register custom providers and resolve
//!   them through a [`signature::ProviderRegistry`].
//! - A constant-time comparison helper ([`signature::constant_time_eq`]).
//!
//! ## Not included — you must wire these yourself
//!
//! This crate stops at signature verification. A complete inbound receiver
//! needs the following, and **none of it lives here**:
//!
//! - **No HTTP receiver / routing** — extracting the body, header, and provider from a request and
//!   dispatching to a handler is the caller's job.
//! - **No idempotency store** — duplicate-delivery detection is *not* performed; the caller must
//!   deduplicate by provider event id.
//! - **No transaction management** — this crate runs no database transactions around handler
//!   execution.
//! - **No handler execution / payload routing** — mapping an event type to a database function and
//!   invoking it is out of scope.
//!
//! ## Inbound vs. Outbound
//!
//! FraiseQL has two webhook-related crates with complementary roles:
//!
//! | Crate | Direction | Purpose |
//! |-------|-----------|---------|
//! | `fraiseql-webhooks` | **Inbound** | Verify signatures on callbacks from Stripe, GitHub, Shopify, … |
//! | `fraiseql-observers` | **Outbound** | Emit notifications when your data changes |
//!
//! Use `fraiseql-webhooks` when you need to authenticate signed callbacks from
//! external providers. Use `fraiseql-observers` when you want to push events to
//! subscribers.
//!
//! ## Supported Providers
//!
//! Built-in signature verification for:
//! - **Stripe** — HMAC-SHA256 on `Stripe-Signature` header with replay protection
//! - **GitHub** — HMAC-SHA256 on `X-Hub-Signature-256` header
//! - **Shopify** — HMAC-SHA256 on `X-Shopify-Hmac-Sha256` header
//! - **SendGrid** — ECDSA on `X-Twilio-Email-Event-Webhook-Signature`
//! - **Paddle** — HMAC-SHA256 on `Paddle-Signature` header with replay protection
//! - Discord, GitLab, Slack, Twilio, Postmark, Lemon Squeezy, and a generic HMAC verifier
//! - Custom providers via the [`traits::SignatureVerifier`] trait
//!
//! ## Security Properties
//!
//! These hold for the verifiers shipped here:
//!
//! - **Constant-time comparison** — [`signature::constant_time_eq`] uses the `subtle` crate so HMAC
//!   comparison does not leak timing information.
//! - **Replay protection** — providers that sign a timestamp (Stripe, Paddle, Slack, Discord,
//!   SendGrid) reject requests whose timestamp falls outside the configured tolerance window (5
//!   minutes by default).
//!
//! Properties such as idempotency and transactional handler execution are **not**
//! provided — see "Not included" above.
//!
//! ## See Also
//!
//! - `fraiseql-observers` — outbound change notifications
//!
//! ## Features

#![forbid(unsafe_code)]
// module_name_repetitions, must_use_candidate, uninlined_format_args:
// allowed at workspace level (Cargo.toml [workspace.lints.clippy]).
#![allow(clippy::doc_markdown)] // Reason: technical terms don't need backtick wrapping
#![allow(clippy::struct_field_names)] // Reason: field prefixes match domain terminology
#![allow(clippy::wildcard_imports)] // Reason: test modules use wildcard imports
#![allow(clippy::items_after_statements)] // Reason: helper structs near point of use in tests
#![allow(clippy::missing_const_for_fn)] // Reason: const fn not stable for all patterns used
#![allow(clippy::cast_possible_wrap)] // Reason: values are within i64 range by design
#![allow(clippy::redundant_clone)] // Reason: explicit clone at API boundaries for clarity
//! - **Multi-provider support**: Stripe, GitHub, Shopify, Paddle, and more
//! - **Signature verification**: Constant-time comparison for security
//! - **Replay protection**: Timestamp-window validation for providers that sign a timestamp
//! - **Custom providers**: Register your own verifier via [`traits::SignatureVerifier`]

pub mod signature;
pub mod testing;
pub mod traits;
pub mod transaction;

// Re-exports
pub use signature::SignatureError;
// Re-export testing mocks for unit tests and integration tests with `testing` feature
#[cfg(any(test, feature = "testing"))]
pub use testing::mocks;
pub use traits::{Clock, EventHandler, IdempotencyStore, SecretProvider, SignatureVerifier};
pub use transaction::{WebhookIsolation, execute_in_transaction};

/// Errors that can occur while verifying a webhook and running caller-supplied seams.
///
/// The signature verifiers in [`signature`] surface their own
/// [`signature::SignatureError`]; this type carries the errors produced by the
/// building-block seams in this crate ([`traits::SecretProvider`],
/// [`traits::EventHandler`], and [`transaction::execute_in_transaction`]).
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum WebhookError {
    /// The secret named in the configuration could not be retrieved from the secret provider.
    /// The inner string is the secret name that was not found.
    #[error("Missing webhook secret: {0}")]
    MissingSecret(String),

    /// A JSON value could not be deserialised. The inner string is the serde_json error message.
    #[error("Invalid payload: {0}")]
    InvalidPayload(String),

    /// A sqlx database operation failed inside a caller-driven transaction.
    /// The inner string is the sqlx error message.
    #[error("Database error: {0}")]
    Database(String),
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

/// Lossless composition into the canonical [`fraiseql_error::FraiseQLError`].
///
/// The webhook subsystem owns this conversion (sqlx pattern) so that
/// `fraiseql-error` can stay a leaf crate in the workspace dependency graph.
/// The boxed payload preserves the full [`WebhookError`] vocabulary via the
/// `Display`/`source` chain.
impl From<WebhookError> for fraiseql_error::FraiseQLError {
    fn from(e: WebhookError) -> Self {
        Self::Webhook(Box::new(e))
    }
}
