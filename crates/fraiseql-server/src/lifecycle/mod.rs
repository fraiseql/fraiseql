//! Server lifecycle management.
//!
//! Provides health and readiness check endpoints and graceful-shutdown
//! coordination so the server can drain in-flight requests before terminating.

pub mod health;
pub mod shutdown;
