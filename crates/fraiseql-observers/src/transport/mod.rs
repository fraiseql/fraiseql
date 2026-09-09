//! Event transport abstraction layer
//!
//! This module provides a trait-based abstraction for event sourcing mechanisms,
//! enabling FraiseQL's observer system to work with multiple event transports:
//!
//! - **`PostgresNotify`**: PostgreSQL LISTEN/NOTIFY (low latency, ephemeral)
//! - **`MySQLBridge`**: MySQL polling-based bridge
//! - **`MSSQLBridge`**: SQL Server polling-based bridge
//! - **Nats**: NATS `JetStream` for distributed architectures
//! - **`InMemory`**: Testing and development
//!
//! # Architecture
//!
//! ```text
//! ObserverExecutor
//!     ↓
//! EventTransport trait (Arc<dyn>)
//!     ↓
//! ┌────────────────┬──────────────┬──────────────┬──────────────┬──────────────┐
//! │                │              │              │              │              │
//! PostgresNotify   MySQLBridge    MSSQLBridge    NatsTransport  InMemory
//! (postgres)       (mysql)        (mssql)        (nats)         (testing)
//! ```
//!
//! # Design Decisions
//!
//! - **`Arc<dyn EventTransport>`**: Runtime transport selection without monomorphization bloat
//! - **Stream-based API**: Natural tokio integration with backpressure
//! - **Transport-managed reconnection**: Transports handle retry/backoff internally
//! - **At-least-once delivery**: Transport ACKs after `ObserverExecutor` processes event

use std::pin::Pin;

use async_trait::async_trait;
use futures::Stream;

use crate::{
    error::Result,
    event::{EntityEvent, EventKind},
};

pub mod in_memory;
#[cfg(feature = "postgres")]
pub mod postgres_notify;

#[cfg(feature = "nats")]
pub mod nats;

#[cfg(all(feature = "postgres", feature = "nats"))]
pub mod bridge;

#[cfg(all(feature = "postgres", feature = "nats"))]
pub use bridge::{
    BridgeConfig, ChangeLogEntry, CheckpointStore, PostgresCheckpointStore, PostgresNatsBridge,
};
pub use in_memory::InMemoryTransport;
#[cfg(feature = "nats")]
pub use nats::{NatsConfig, NatsTransport};
#[cfg(feature = "postgres")]
pub use postgres_notify::PostgresNotifyTransport;

/// Event stream type (async stream of `EntityEvents`)
pub type EventStream = Pin<Box<dyn Stream<Item = Result<EntityEvent>> + Send>>;

/// Core event transport abstraction
///
/// Implementors must:
/// - Handle reconnection/backoff internally
/// - Not crash on transient failures
/// - Emit errors via stream, not panic
/// - ACK messages only after successful processing (at-least-once semantics)
// Reason: designed for use as dyn Trait (Arc<dyn EventTransport>); async_trait ensures Send bounds
// and dyn-compatibility async_trait: dyn-dispatch required; remove when RTN + Send is stable (RFC
// 3425)
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
            status:  HealthStatus::Healthy,
            message: None,
        })
    }
}

/// Transport type identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum TransportType {
    /// PostgreSQL LISTEN/NOTIFY
    PostgresNotify,
    /// NATS `JetStream`
    #[cfg(feature = "nats")]
    Nats,
    /// In-memory for testing
    InMemory,
}

/// Which tenants' events a subscription may receive.
///
/// Deliberately not the `Option<String>` this replaces. That field defaulted to
/// `None`, and `None` meant *every tenant* — so the permissive answer was the one you
/// got by not thinking about the question. #1113 is what that produces: the REST SSE
/// handler built its filter with `..Default::default()` and subscribed across the
/// whole deployment, on an endpoint that ends at one authenticated client.
///
/// A scope now has to be named, which also makes the audit question answerable by
/// grep: every consumer that opted out of tenant filtering says [`AllTenants`] out
/// loud.
///
/// [`AllTenants`]: TenantScope::AllTenants
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TenantScope {
    /// Every tenant's events.
    ///
    /// Correct only for a consumer that *is* the tenancy boundary rather than sitting
    /// inside one: the observer runtime, the retry loop, the PG→NATS bridge. Never
    /// correct for a stream that terminates at a single caller.
    AllTenants,
    /// Only events stamped with this tenant id ([`EntityEvent::tenant_id`]).
    ///
    /// An event carrying no tenant id does **not** match — a missing stamp is not a
    /// wildcard. This is the same fail-closed rule the GraphQL subscription gate
    /// applies in multi-tenant mode (`SubscriptionManager::event_matches`).
    Tenant(String),
}

/// Event filter for subscription.
///
/// Has no `Default`: `..Default::default()` is exactly how #1113 was written, and a
/// security filter whose omitted fields mean "everything" cannot be a struct you can
/// half-fill. Start from [`EventFilter::all_tenants`] or [`EventFilter::for_tenant`]
/// and narrow.
///
/// # Examples
///
/// ```
/// use fraiseql_observers::{event::EventKind, transport::EventFilter};
///
/// // One tenant's Order deletions.
/// let filter = EventFilter::for_tenant("acme")
///     .with_entity_type("Order")
///     .with_operation(EventKind::Deleted);
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventFilter {
    /// Filter by entity type (`None` = all types).
    pub entity_type: Option<String>,
    /// Filter by operation (`None` = all operations).
    ///
    /// An [`EventKind`] rather than a `String`: the comparison used to be against
    /// `"INSERT"`/`"UPDATE"`/`"DELETE"`/`"CUSTOM"` spelled out at the comparison site,
    /// so a caller writing `"insert"` got a filter that silently matched nothing.
    pub operation:   Option<EventKind>,
    /// Which tenants' events this subscription may receive.
    pub tenant:      TenantScope,
}

impl EventFilter {
    /// A filter over every tenant's events.
    ///
    /// See [`TenantScope::AllTenants`] for when that is the right answer.
    #[must_use]
    pub const fn all_tenants() -> Self {
        Self {
            entity_type: None,
            operation:   None,
            tenant:      TenantScope::AllTenants,
        }
    }

    /// A filter over an already-decided tenant scope.
    ///
    /// The constructor to reach for when the scope is *computed* — resolved from a
    /// request's principal, say. Keeps struct literals out of call sites, which is
    /// where #1113 came from: `EventFilter { entity_type, ..Default::default() }`
    /// filled in the tenant with the permissive answer and read as complete.
    #[must_use]
    pub const fn scoped_to(tenant: TenantScope) -> Self {
        Self {
            entity_type: None,
            operation: None,
            tenant,
        }
    }

    /// A filter scoped to one tenant's events.
    #[must_use]
    pub fn for_tenant(tenant_id: impl Into<String>) -> Self {
        Self {
            entity_type: None,
            operation:   None,
            tenant:      TenantScope::Tenant(tenant_id.into()),
        }
    }

    /// Narrow to a single entity type.
    #[must_use]
    pub fn with_entity_type(mut self, entity_type: impl Into<String>) -> Self {
        self.entity_type = Some(entity_type.into());
        self
    }

    /// Narrow to a single operation.
    #[must_use]
    pub const fn with_operation(mut self, operation: EventKind) -> Self {
        self.operation = Some(operation);
        self
    }

    /// Whether `event` is inside this filter.
    ///
    /// The single definition of the rule. It had lived inside `NatsTransport::subscribe`,
    /// and the other two transports took `_filter` and returned an unfiltered stream —
    /// so whether a subscription was filtered at all depended on which transport the
    /// deployment happened to be running (#1113).
    #[must_use]
    pub fn matches(&self, event: &EntityEvent) -> bool {
        if let Some(ref entity_type) = self.entity_type {
            if &event.entity_type != entity_type {
                return false;
            }
        }

        if let Some(operation) = self.operation {
            if event.event_type != operation {
                return false;
            }
        }

        match self.tenant {
            TenantScope::AllTenants => true,
            TenantScope::Tenant(ref wanted) => event.tenant_id.as_deref() == Some(wanted.as_str()),
        }
    }
}

/// Transport health status
#[derive(Debug, Clone)]
pub struct TransportHealth {
    /// Health status
    pub status:  HealthStatus,
    /// Optional message (for degraded/unhealthy states)
    pub message: Option<String>,
}

/// Health status enum
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum HealthStatus {
    /// Transport healthy
    Healthy,
    /// Transport degraded (e.g., retrying connection)
    Degraded,
    /// Transport unhealthy (fatal error)
    Unhealthy,
}

#[cfg(test)]
mod tests;
