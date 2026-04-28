//! Realtime broadcast observer — entity change streams over `WebSocket`.
//!
//! This module implements a `WebSocket` server at `/realtime/v1` that pushes
//! mutation events to connected clients with per-client RLS filtering and
//! subscription management.
//!
//! # Complementary to GraphQL Subscriptions
//!
//! This is **not** a replacement for the existing `subscriptions/` module.
//! GraphQL subscriptions use the `graphql-ws`/`graphql-transport-ws` protocol
//! (query-based: client sends a GraphQL subscription operation). This module
//! implements entity-level change streams (subscribe to entity name, receive
//! all mutations). Different protocols, different use cases.

pub mod connections;
pub mod protocol;
pub mod server;
pub mod subscriptions;

#[cfg(test)]
#[allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#[allow(clippy::missing_panics_doc)] // Reason: test code
mod tests;
