//! Tenant audit trail — append-only event log for tenant lifecycle operations.
//!
//! Events are recorded after each admin operation (create, update, suspend,
//! resume, delete). The log is append-only by API design: no update or delete
//! methods exist on the trait.
//!
//! The default [`InMemoryAuditLog`] implementation stores events in a `Vec`
//! behind a `RwLock`. A database-backed implementation can be added later by
//! implementing the [`TenantAuditLog`] trait.

use std::sync::Arc;

use async_trait::async_trait;
use serde::Serialize;
use tokio::sync::RwLock;

/// Tenant lifecycle event types.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TenantEventKind {
    /// Tenant was created (first registration).
    Created,
    /// Tenant configuration was updated (subsequent PUT).
    ConfigChanged,
    /// Tenant was suspended (data requests return 503).
    Suspended,
    /// Tenant was resumed (data requests restored).
    Resumed,
    /// Tenant was deleted.
    Deleted,
}

impl TenantEventKind {
    /// Returns the string label for this event kind.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Created => "created",
            Self::ConfigChanged => "config_changed",
            Self::Suspended => "suspended",
            Self::Resumed => "resumed",
            Self::Deleted => "deleted",
        }
    }
}

/// A single audit trail event.
#[derive(Debug, Clone, Serialize)]
pub struct TenantEvent {
    /// The tenant key this event relates to.
    pub tenant_key: String,
    /// The kind of lifecycle event.
    pub event: TenantEventKind,
    /// The actor who triggered the event (JWT `sub` claim or `"admin_token"`).
    pub actor: Option<String>,
    /// Event-specific metadata (e.g., quota changes, config diffs).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payload: Option<serde_json::Value>,
    /// When the event occurred (ISO 8601).
    pub occurred_at: String,
}

/// Trait for tenant audit log backends.
///
/// Implementations must be `Send + Sync` for use in async contexts.
/// The trait is append-only by design — no update or delete methods.
// Reason: async_trait required for dyn compatibility (used in AppState)
#[async_trait]
pub trait TenantAuditLog: Send + Sync {
    /// Record a tenant lifecycle event.
    ///
    /// # Errors
    ///
    /// Returns an error if the event cannot be persisted.
    async fn record(
        &self,
        tenant_key: &str,
        event: TenantEventKind,
        actor: Option<&str>,
        payload: Option<serde_json::Value>,
    ) -> fraiseql_error::Result<()>;

    /// Query events for a specific tenant, ordered by occurrence (newest first).
    ///
    /// # Errors
    ///
    /// Returns an error if the query fails.
    async fn events_for(
        &self,
        tenant_key: &str,
        limit: usize,
        offset: usize,
    ) -> fraiseql_error::Result<Vec<TenantEvent>>;
}

/// In-memory audit log for testing and lightweight deployments.
///
/// Events are stored in a `Vec` behind a `RwLock`. Not durable — events are
/// lost on server restart. Use a database-backed implementation for production.
pub struct InMemoryAuditLog {
    events: RwLock<Vec<TenantEvent>>,
}

impl InMemoryAuditLog {
    /// Create a new empty in-memory audit log.
    #[must_use]
    pub fn new() -> Self {
        Self {
            events: RwLock::new(Vec::new()),
        }
    }
}

impl Default for InMemoryAuditLog {
    fn default() -> Self {
        Self::new()
    }
}

// Reason: async_trait required for dyn compatibility
#[async_trait]
impl TenantAuditLog for InMemoryAuditLog {
    async fn record(
        &self,
        tenant_key: &str,
        event: TenantEventKind,
        actor: Option<&str>,
        payload: Option<serde_json::Value>,
    ) -> fraiseql_error::Result<()> {
        let entry = TenantEvent {
            tenant_key: tenant_key.to_string(),
            event,
            actor: actor.map(ToString::to_string),
            payload,
            occurred_at: chrono::Utc::now().to_rfc3339(),
        };
        self.events.write().await.push(entry);
        Ok(())
    }

    async fn events_for(
        &self,
        tenant_key: &str,
        limit: usize,
        offset: usize,
    ) -> fraiseql_error::Result<Vec<TenantEvent>> {
        let events = self.events.read().await;
        let matching: Vec<TenantEvent> = events
            .iter()
            .rev() // newest first
            .filter(|e| e.tenant_key == tenant_key)
            .skip(offset)
            .take(limit)
            .cloned()
            .collect();
        Ok(matching)
    }
}

/// Type-erased audit log handle for use in `AppState`.
pub type AuditLogHandle = Arc<dyn TenantAuditLog>;

/// SQL DDL for the tenant events table in the control plane database.
///
/// This migration creates an append-only audit trail for tenant lifecycle events.
/// The table is designed for the control plane database (not tenant databases).
pub const TENANT_EVENTS_DDL: &str = "\
CREATE TABLE IF NOT EXISTS _fraiseql_tenant_events (
    id           BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    tenant_key   TEXT NOT NULL,
    event        TEXT NOT NULL,
    actor        TEXT,
    payload      JSONB,
    occurred_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_tenant_events_key
    ON _fraiseql_tenant_events (tenant_key, occurred_at);
";
