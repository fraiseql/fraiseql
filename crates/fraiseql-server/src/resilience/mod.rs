//! Resilience primitives for the FraiseQL server.
//!
//! Contains backpressure control that limits concurrent requests to prevent
//! resource exhaustion under high load.
//!
//! # Wiring `AdmissionController`
//!
//! `admission_middleware` is mounted by `crate::server::routing` whenever
//! `[admission_control]` is present in the server config: it acquires a permit
//! before the request runs and holds it until the response is produced.
//!
//! This module doc previously said the controller was "available but not yet wired
//! into the default middleware stack", which was accurate — and directly contradicted
//! the boot log, which announced "Admission controller enabled" while
//! `ServerConfig::admission_control`'s own documentation promised that requests over
//! the limit "receive 503 Service Unavailable immediately instead of stalling under
//! load". The controller was constructed and inserted into the request extension map,
//! which inserts a value and gates nothing; no handler, middleware or tower layer ever
//! read it back out (#860).

pub mod backpressure;

mod middleware;

pub use middleware::admission_middleware;

#[cfg(test)]
mod tests;
