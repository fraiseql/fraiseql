//! # fraiseql-webhooks
//!
//! Verifying and **processing** inbound webhooks from third-party services. This
//! crate provides constant-time signature verification and a genuinely-real
//! receiver pipeline ([`WebhookPipeline`]) with atomic idempotency and
//! transactional handler execution. It stops short of the HTTP layer: extracting
//! the request and routing it to a provider is the caller's (or the server's) job,
//! so the crate stays free of any web framework.
//!
//! ## What this crate provides
//!
//! - Per-provider signature verifiers ([`signature`]) returning `Ok(bool)` /
//!   [`signature::SignatureError`], registrable via [`traits::SignatureVerifier`] and resolved
//!   through a [`signature::ProviderRegistry`].
//! - A constant-time comparison helper ([`signature::constant_time_eq`]).
//! - [`WebhookPipeline`] — composes secret resolution → signature verification → atomic idempotency
//!   claim → transactional handler into one [`process`](WebhookPipeline::process) call (see
//!   "Security Properties").
//! - [`PostgresIdempotencyStore`] — a durable delivery ledger whose claim is an `INSERT … ON
//!   CONFLICT DO NOTHING` issued on the handler's transaction.
//! - [`StaticSecretProvider`] — an in-memory [`SecretProvider`] for callers that load signing
//!   secrets at startup.
//!
//! ## Not included — you wire these yourself
//!
//! - **No HTTP receiver / routing** — extracting the body, signature header, and provider from a
//!   request and dispatching to [`WebhookPipeline::process`] is the caller's job (the server mounts
//!   the route).
//! - **No built-in event handler** — mapping a verified event to *your* database function is
//!   inherently app-specific; you implement [`traits::EventHandler`] (the pipeline runs it inside
//!   the delivery's transaction).
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
//! For the signature verifiers:
//!
//! - **Constant-time comparison** — [`signature::constant_time_eq`] uses the `subtle` crate so HMAC
//!   comparison does not leak timing information.
//! - **Replay protection** — providers that sign a timestamp (Stripe, Paddle, Slack, Discord,
//!   SendGrid) reject requests whose timestamp falls outside the configured tolerance window (5
//!   minutes by default).
//!
//! For [`WebhookPipeline`]:
//!
//! - **Verify before any database work** — a forged or malformed signature is rejected before a
//!   connection is taken, so an attacker cannot drive load onto the pool.
//! - **Idempotency** — a duplicate delivery is detected and silently discarded. The claim is an
//!   atomic `INSERT … ON CONFLICT DO NOTHING` on the handler's own transaction, so concurrent
//!   duplicate deliveries serialise on the unique-key row lock and exactly one is processed.
//! - **Transactional handler execution** — the idempotency claim and the handler commit or roll
//!   back together. A handler failure rolls the claim back, so the sender's retry reprocesses the
//!   event rather than it being lost as "seen but unhandled" (no lost / double-processed events).
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

pub mod idempotency;
pub mod pipeline;
pub mod secret;
pub mod signature;
pub mod testing;
pub mod traits;
pub mod transaction;

// Re-exports
pub use idempotency::PostgresIdempotencyStore;
pub use pipeline::{Delivery, Disposition, WebhookPipeline, verify_signature};
pub use secret::StaticSecretProvider;
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

    /// Signature verification failed: the delivery's signature did not match
    /// (mismatch) or could not be parsed (bad format, expired timestamp). The
    /// inner string is the reason. Constructed by the receiver pipeline; the
    /// sender failed to authenticate, so this maps to HTTP 401.
    #[error("Webhook signature verification failed: {0}")]
    SignatureInvalid(String),

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

/// Composition into the canonical [`fraiseql_error::FraiseQLError`], routed by
/// variant so the resulting HTTP status reflects *who* is at fault
/// (M-webhook-error-status).
///
/// The webhook subsystem owns this conversion (sqlx pattern) so that
/// `fraiseql-error` can stay a leaf crate in the workspace dependency graph —
/// `fraiseql-error` cannot downcast a boxed `WebhookError`, so the per-variant
/// decision must live here.
///
/// Previously every variant boxed into `FraiseQLError::Webhook`, which maps to
/// HTTP 400 — so a transient database blip during webhook handling told the
/// sender "permanent client error, do not retry", losing the event. Now:
///
/// - [`WebhookError::Database`] → `FraiseQLError::Database` (5xx, retryable): a backend blip must
///   let the sender re-deliver.
/// - [`WebhookError::MissingSecret`] → `FraiseQLError::Configuration` (5xx): a server-side
///   misconfiguration, not the sender's fault.
/// - [`WebhookError::SignatureInvalid`] → `FraiseQLError::Authentication` (401): the sender failed
///   to authenticate; a retry with the same (forged) signature will not succeed.
/// - [`WebhookError::InvalidPayload`] → `FraiseQLError::Webhook` (400): the sender's payload is
///   genuinely malformed; a 4xx is correct.
impl From<WebhookError> for fraiseql_error::FraiseQLError {
    fn from(e: WebhookError) -> Self {
        match e {
            WebhookError::Database(msg) => Self::database(format!("webhook: {msg}")),
            WebhookError::MissingSecret(name) => Self::Configuration {
                message: format!("webhook secret not found: {name}"),
            },
            WebhookError::SignatureInvalid(reason) => Self::Authentication {
                message: format!("webhook signature verification failed: {reason}"),
            },
            other @ WebhookError::InvalidPayload(_) => Self::Webhook(Box::new(other)),
        }
    }
}

#[cfg(test)]
mod error_status_tests {
    use fraiseql_error::FraiseQLError;

    use super::WebhookError;

    #[test]
    fn database_error_maps_to_retryable_5xx() {
        let err: FraiseQLError = WebhookError::Database("connection reset".into()).into();
        assert_eq!(
            err.status_code(),
            500,
            "a webhook DB blip must be a retryable 5xx, not a permanent 400"
        );
    }

    #[test]
    fn missing_secret_maps_to_5xx() {
        let err: FraiseQLError = WebhookError::MissingSecret("stripe".into()).into();
        assert_eq!(err.status_code(), 500, "a server-side config error is not the sender's fault");
    }

    #[test]
    fn invalid_payload_maps_to_4xx() {
        let err: FraiseQLError = WebhookError::InvalidPayload("bad json".into()).into();
        assert_eq!(err.status_code(), 400, "a malformed sender payload is a genuine 4xx");
    }

    #[test]
    fn signature_invalid_maps_to_401() {
        let err: FraiseQLError = WebhookError::SignatureInvalid("signature mismatch".into()).into();
        assert_eq!(
            err.status_code(),
            401,
            "a forged signature is an authentication failure, not a 400/500"
        );
    }
}
