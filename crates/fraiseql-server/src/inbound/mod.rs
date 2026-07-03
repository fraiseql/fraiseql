//! Inbound ingestion as a source (continues #431).
//!
//! The symmetric mirror of the outbound observer→signed-webhook path. Where the
//! outbound path turns a database change into a durable change-log event that an
//! observer delivers as a signed webhook, the inbound path turns an external
//! message into a normalized [`InboundMessage`](fraiseql_functions::InboundMessage)
//! on a durable spine that `after:ingest[:<source>]` functions consume.
//!
//! ```text
//!   external message ──► adapter ──► normalize ──► spine ──► after:ingest fns
//!   (webhook / email)    (push|pull)  (InboundMessage)  (durable)
//! ```
//!
//! One primitive, many adapters. This phase mounts the `fraiseql-webhooks`
//! receiver as the first push [`Source`](fraiseql_functions::Source); the
//! poll-IMAP email adapter is a pull source added in a later phase.
//!
//! ## Modules
//!
//! - [`spine`] — the durable inbound-message store (persist + dedup, at-least-once).
//! - [`webhook`] — the `fraiseql-webhooks` push adapter (`POST /webhooks/{provider}`).

#[cfg(feature = "inbound-email")]
pub mod email;
pub mod spine;
pub mod webhook;

pub use spine::{Emitted, PostgresInboundSpine, emit_in_tx};
pub use webhook::{WebhookInboundState, WebhookSource, webhook_router};
