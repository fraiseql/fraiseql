//! Configuration structures for the observer system.
//!
//! This module provides configuration for:
//! - Observer runtime (channel capacity, concurrency, etc.)
//! - Transport selection (PostgreSQL LISTEN/NOTIFY, NATS, in-memory)
//! - NATS `JetStream` settings (retention, deduplication, etc.)
//! - Bridge configuration (PostgreSQL → NATS)
//!
//! # Configuration Sources
//!
//! Configuration can be loaded from:
//! 1. TOML files (base configuration)
//! 2. Environment variables (overrides)
//!
//! Environment variable precedence: `FRAISEQL_*` > TOML > defaults

pub mod clickhouse;
pub mod job_queue;
pub mod performance;
pub mod redis;
pub mod runtime;
pub mod transport;

#[cfg(test)]
mod tests;

pub use clickhouse::ClickHouseConfig;
pub use job_queue::JobQueueConfig;
pub use performance::PerformanceConfig;
pub use redis::RedisConfig;
pub use runtime::{
    ActionConfig, BackoffStrategy, FailurePolicy, MultiListenerConfig, ObserverDefinition,
    ObserverRuntimeConfig, OverflowPolicy, RetryConfig,
};
pub use transport::{
    BridgeTransportConfig, JetStreamConfig, NatsTransportConfig, TransportConfig, TransportKind,
};
