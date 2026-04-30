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
    pub tenant_key:  String,
    /// The kind of lifecycle event.
    pub event:       TenantEventKind,
    /// The actor who triggered the event (JWT `sub` claim or `"admin_token"`).
    pub actor:       Option<String>,
    /// Event-specific metadata (e.g., quota changes, config diffs).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payload:     Option<serde_json::Value>,
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
            tenant_key:  tenant_key.to_string(),
            event,
            actor:       actor.map(ToString::to_string),
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

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable
    #![allow(clippy::missing_panics_doc)] // Reason: test helpers
    #![allow(clippy::missing_errors_doc)] // Reason: test helpers
    #![allow(missing_docs)] // Reason: test code

    use super::*;

    #[tokio::test]
    async fn record_and_retrieve_event() {
        let log = InMemoryAuditLog::new();
        log.record("tenant-abc", TenantEventKind::Created, Some("admin"), None)
            .await
            .unwrap();

        let events = log.events_for("tenant-abc", 10, 0).await.unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].tenant_key, "tenant-abc");
        assert_eq!(events[0].event, TenantEventKind::Created);
        assert_eq!(events[0].actor.as_deref(), Some("admin"));
    }

    #[tokio::test]
    async fn events_filtered_by_tenant_key() {
        let log = InMemoryAuditLog::new();
        log.record("tenant-a", TenantEventKind::Created, None, None)
            .await
            .unwrap();
        log.record("tenant-b", TenantEventKind::Created, None, None)
            .await
            .unwrap();
        log.record("tenant-a", TenantEventKind::Suspended, None, None)
            .await
            .unwrap();

        let events_a = log.events_for("tenant-a", 10, 0).await.unwrap();
        assert_eq!(events_a.len(), 2);

        let events_b = log.events_for("tenant-b", 10, 0).await.unwrap();
        assert_eq!(events_b.len(), 1);
    }

    #[tokio::test]
    async fn events_returned_newest_first() {
        let log = InMemoryAuditLog::new();
        log.record("t", TenantEventKind::Created, None, None)
            .await
            .unwrap();
        log.record("t", TenantEventKind::Suspended, None, None)
            .await
            .unwrap();
        log.record("t", TenantEventKind::Resumed, None, None)
            .await
            .unwrap();

        let events = log.events_for("t", 10, 0).await.unwrap();
        assert_eq!(events[0].event, TenantEventKind::Resumed);
        assert_eq!(events[1].event, TenantEventKind::Suspended);
        assert_eq!(events[2].event, TenantEventKind::Created);
    }

    #[tokio::test]
    async fn pagination_with_limit_and_offset() {
        let log = InMemoryAuditLog::new();
        for _ in 0..5 {
            log.record("t", TenantEventKind::Created, None, None)
                .await
                .unwrap();
        }

        let page1 = log.events_for("t", 2, 0).await.unwrap();
        assert_eq!(page1.len(), 2);

        let page2 = log.events_for("t", 2, 2).await.unwrap();
        assert_eq!(page2.len(), 2);

        let page3 = log.events_for("t", 2, 4).await.unwrap();
        assert_eq!(page3.len(), 1);
    }

    #[tokio::test]
    async fn config_changed_event_with_payload() {
        let log = InMemoryAuditLog::new();
        let payload = serde_json::json!({
            "max_concurrent": {"old": 5, "new": 10}
        });
        log.record(
            "tenant-abc",
            TenantEventKind::ConfigChanged,
            Some("user-42"),
            Some(payload.clone()),
        )
        .await
        .unwrap();

        let events = log.events_for("tenant-abc", 10, 0).await.unwrap();
        assert_eq!(events[0].payload.as_ref(), Some(&payload));
    }

    #[tokio::test]
    async fn append_only_no_update_or_delete() {
        // Verify by API: there are no update/delete methods on TenantAuditLog.
        // This test records multiple events and confirms all are preserved.
        let log = InMemoryAuditLog::new();
        log.record("t", TenantEventKind::Created, None, None)
            .await
            .unwrap();
        log.record("t", TenantEventKind::Suspended, None, None)
            .await
            .unwrap();
        log.record("t", TenantEventKind::Deleted, None, None)
            .await
            .unwrap();

        let events = log.events_for("t", 100, 0).await.unwrap();
        assert_eq!(events.len(), 3, "all events must be preserved (append-only)");
    }

    #[test]
    fn event_kind_as_str() {
        assert_eq!(TenantEventKind::Created.as_str(), "created");
        assert_eq!(TenantEventKind::ConfigChanged.as_str(), "config_changed");
        assert_eq!(TenantEventKind::Suspended.as_str(), "suspended");
        assert_eq!(TenantEventKind::Resumed.as_str(), "resumed");
        assert_eq!(TenantEventKind::Deleted.as_str(), "deleted");
    }

    #[test]
    fn event_kind_serializes_to_snake_case() {
        let json = serde_json::to_string(&TenantEventKind::ConfigChanged).unwrap();
        assert_eq!(json, "\"config_changed\"");
    }
}
