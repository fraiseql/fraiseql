//! Realtime broadcast observer — entity change streams over `WebSocket`.
//!
//! This module implements a `WebSocket` server at `/realtime/v1` that pushes
//! mutation events to connected clients with per-client RLS filtering and
//! subscription management.
//!
//! # Two `WebSocket` systems — when to use each
//!
//! FraiseQL ships two independent `WebSocket` systems that coexist on the same
//! server without shared state or naming conflicts.
//!
//! | | **`realtime/`** (this module) | **`subscriptions/`** |
//! |---|---|---|
//! | **Endpoint** | `/realtime/v1` | configurable (default `/ws`) |
//! | **Protocol** | Custom JSON (`connected`, `subscribe`, `change`, …) | `graphql-transport-ws` / `graphql-ws` |
//! | **Client model** | Subscribe to an *entity name*, receive all mutations | Send a GraphQL subscription *operation*, receive `next` payloads |
//! | **Primary use case** | Live UI updates (feeds, dashboards, notifications) | Real-time computed results from a subscription resolver |
//! | **RLS** | Enforced per-event via `RlsEvaluator` (server-side row check) | Enforced at query planning time by the GraphQL runtime |
//!
//! **Use `realtime/`** when you want to react to raw entity mutations (e.g.
//! "show me every new `Order` row").
//! **Use `subscriptions/`** when you want the full GraphQL field-selection and
//! resolver pipeline on a streaming query (e.g. `subscription { newOrders { id, total } }`).
//!
//! Both systems are mounted by the Phase 8 `ServerSubsystems` builder and can
//! be enabled independently via `schema.compiled.json`.

pub mod connections;
pub mod context_hash;
pub mod delivery;
pub mod observer;
pub mod protocol;
pub mod routes;
pub mod server;
pub mod subscriptions;

#[cfg(test)]
#[allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#[allow(clippy::missing_panics_doc)] // Reason: test code
mod tests;
