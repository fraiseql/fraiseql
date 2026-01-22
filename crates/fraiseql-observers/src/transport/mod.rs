//! Event transport abstraction layer
//!
//! This module provides a trait-based abstraction for event sourcing mechanisms,
//! enabling FraiseQL's observer system to work with multiple event transports:
//!
//! - **`PostgresNotify`**: Existing PostgreSQL LISTEN/NOTIFY (low latency, ephemeral)
//! - **Nats**: NATS `JetStream` for distributed architectures (Phase 2)
//! - **`InMemory`**: Testing and development
//!
//! # Architecture
//!
//! ```text
//! ObserverExecutor
//!     ↓
//! EventTransport trait (Arc<dyn>)
//!     ↓
//! ┌────────────────┬──────────────┬──────────────┐
//! │                │              │              │
//! PostgresNotify   NatsTransport  InMemory
//! (existing)       (Phase 2)      (testing)
//! ```
//!
//! # Design Decisions
//!
//! - **Arc<dyn EventTransport>**: Runtime transport selection without monomorphization bloat
//! - **Stream-based API**: Natural tokio integration with backpressure
//! - **Transport-managed reconnection**: Transports handle retry/backoff internally
//! - **At-least-once delivery**: Transport ACKs after `ObserverExecutor` processes event

use crate::error::Result;
use crate::event::EntityEvent;
use async_trait::async_trait;
use futures::Stream;
use std::pin::Pin;

pub mod postgres_notify;
pub mod in_memory;

#[cfg(feature = "nats")]
pub mod nats;

pub use postgres_notify::PostgresNotifyTransport;
pub use in_memory::InMemoryTransport;

#[cfg(feature = "nats")]
pub use nats::{NatsConfig, NatsTransport};

/// Event stream type (async stream of `EntityEvents`)
pub type EventStream = Pin<Box<dyn Stream<Item = Result<EntityEvent>> + Send>>;

/// Core event transport abstraction
///
/// Implementors must:
/// - Handle reconnection/backoff internally
/// - Not crash on transient failures
/// - Emit errors via stream, not panic
/// - ACK messages only after successful processing (at-least-once semantics)
#[async_trait]
pub trait EventTransport: Send + Sync {
    /// Subscribe to events matching filter (returns async stream)
    ///
    /// # Guarantees
    /// - Transports must handle reconnection/backoff internally
    /// - Must not crash executor loop on transient failures
    /// - Stream ends on fatal errors (consumers restart loop)
    ///
    /// # ACK Semantics
    /// - `NatsTransport` ACKs only after `ObserverExecutor::process_event()` returns `Ok()`
    /// - At-least-once delivery preserved (redelivery on processing failure)
    /// - If processing fails, message is NOT `ACKed` and will be redelivered
    /// - Idempotent consumers required (duplicates possible on retry)
    async fn subscribe(&self, filter: EventFilter) -> Result<EventStream>;

    /// Publish event (for observers that trigger new events)
    async fn publish(&self, event: EntityEvent) -> Result<()>;

    /// Transport type identifier
    fn transport_type(&self) -> TransportType;

    /// Health check (optional, default implementation)
    async fn health_check(&self) -> Result<TransportHealth> {
        Ok(TransportHealth {
            status: HealthStatus::Healthy,
            message: None,
        })
    }
}

/// Transport type identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportType {
    /// PostgreSQL LISTEN/NOTIFY (existing)
    PostgresNotify,
    /// NATS `JetStream` (Phase 2)
    #[cfg(feature = "nats")]
    Nats,
    /// In-memory for testing
    InMemory,
}

/// Event filter for subscription
///
/// Future extension: can filter by entity type, operation, tenant, etc.
#[derive(Debug, Clone, Default)]
pub struct EventFilter {
    /// Filter by entity type (None = all types)
    pub entity_type: Option<String>,
    /// Filter by operation (INSERT/UPDATE/DELETE)
    pub operation: Option<String>,
    /// Filter by tenant ID
    pub tenant_id: Option<String>,
}

/// Transport health status
#[derive(Debug, Clone)]
pub struct TransportHealth {
    /// Health status
    pub status: HealthStatus,
    /// Optional message (for degraded/unhealthy states)
    pub message: Option<String>,
}

/// Health status enum
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HealthStatus {
    /// Transport healthy
    Healthy,
    /// Transport degraded (e.g., retrying connection)
    Degraded,
    /// Transport unhealthy (fatal error)
    Unhealthy,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_filter_default() {
        let filter = EventFilter::default();
        assert!(filter.entity_type.is_none());
        assert!(filter.operation.is_none());
        assert!(filter.tenant_id.is_none());
    }

    #[test]
    fn test_health_status_equality() {
        assert_eq!(HealthStatus::Healthy, HealthStatus::Healthy);
        assert_ne!(HealthStatus::Healthy, HealthStatus::Degraded);
    }

    #[test]
    fn test_transport_type_equality() {
        assert_eq!(TransportType::PostgresNotify, TransportType::PostgresNotify);
        assert_eq!(TransportType::InMemory, TransportType::InMemory);
        assert_ne!(TransportType::PostgresNotify, TransportType::InMemory);
    }
}
