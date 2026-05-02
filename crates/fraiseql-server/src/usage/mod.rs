//! Usage aggregation subsystem.
//!
//! Subscribes to `fraiseql::mutation_audit` tracing events (emitted by the
//! runtime after every successful mutation) and maintains in-memory per-tenant,
//! per-period, per-entity-type counters.
//!
//! # Architecture
//!
//! ```text
//! fraiseql-core runtime
//!   └── tracing::info!(target: "fraiseql::mutation_audit", ...)
//!         │
//!         ▼
//! MutationAuditLayer (tracing_subscriber::Layer)
//!   └── UsageAggregator (DashMap<(tenant, period, entity), AtomicU64>)
//!         │
//!         ▼
//! HTTP query endpoint
//!   GET /api/v1/admin/usage?tenant_id=…&period=…
//! ```
//!
//! # Limitations (v1)
//!
//! - **In-memory only**: counters reset to zero on process restart.
//! - **Unbounded**: no eviction; see [`aggregator`] module docs for growth bounds.

pub mod aggregator;
pub mod events;
pub mod layer;
