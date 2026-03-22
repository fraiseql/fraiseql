//! Resilience primitives for the FraiseQL server.
//!
//! Contains backpressure control that limits concurrent requests to prevent
//! resource exhaustion under high load.
//!
//! # Wiring `backpressure::AdmissionController`
//!
//! `backpressure::AdmissionController` is available but not yet wired into
//! the default middleware stack.  To enable it, add an `admission_control`
//! field to [`crate::server_config::ServerConfig`] and wire it in
//! `crate::server::routing` by calling
//! `backpressure::AdmissionController::try_acquire` (or
//! `backpressure::AdmissionController::acquire_timeout`) at the top of the
//! GraphQL handler.  The controller can be stored in the router's extension
//! map via `axum::Router::layer` with a custom Tower middleware.

pub mod backpressure;
