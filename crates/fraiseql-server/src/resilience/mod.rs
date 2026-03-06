//! Resilience primitives for the FraiseQL server.
//!
//! Contains backpressure control that limits concurrent requests to prevent
//! resource exhaustion under high load.

pub mod backpressure;
