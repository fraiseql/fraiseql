//! REST transport — resource-centric HTTP API auto-generated from compiled schema.
//!
//! Provides [`rest_router`] which builds an axum [`Router`] from a
//! [`CompiledSchema`]'s REST configuration.  The router is mounted at the
//! configured base path (default `/rest/v1`) and dispatches requests to the
//! appropriate query or mutation executor via [`RestHandler`].
//!
//! All modules are gated behind `#[cfg(feature = "rest")]` in `routes/mod.rs`.

pub mod bulk;
pub mod cache_control;
pub mod embedding;
pub mod handler;
pub mod idempotency;
pub mod openapi;
pub mod params;
pub mod resource;
pub mod response;
mod router;
pub mod sse;
pub mod streaming;

pub use router::rest_router;
