//! Configuration structures for the observer system.
//!
//! This module provides configuration for:
//! - Observer definitions, actions and retry policy
//! - Transport selection (PostgreSQL LISTEN/NOTIFY, NATS, in-memory)
//! - NATS `JetStream` settings (retention, deduplication, etc.)
//!
//! # Configuration Sources
//!
//! Configuration can be loaded from:
//! 1. TOML files (base configuration)
//! 2. Environment variables (overrides)
//!
//! Environment variable precedence: `FRAISEQL_*` > TOML > defaults

pub mod email;
pub mod redis;
pub mod runtime;
pub mod transport;

#[cfg(test)]
mod tests;

pub use email::{EmailSmtpConfig, SmtpTlsMode};
pub use redis::RedisConfig;
pub use runtime::{ActionConfig, BackoffStrategy, FailurePolicy, ObserverDefinition, RetryConfig};
pub use transport::{JetStreamConfig, NatsTransportConfig, TransportConfig, TransportKind};
