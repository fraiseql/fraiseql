//! Event types and data structures for the observer system.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// The type of database event that triggered the observer.
///
/// Deliberately a **closed** enum (no `#[non_exhaustive]`): consumers that route
/// on the event kind — in particular the GraphQL subscription bridge, which must
/// never fabricate a `Create` from a snapshot/no-op row (#773) — are required to
/// match exhaustively, so adding a variant is a compile error at every mapping
/// site instead of a silent fall-through.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum EventKind {
    /// Entity was created
    #[serde(rename = "INSERT")]
    Created,
    /// Entity was updated
    #[serde(rename = "UPDATE")]
    Updated,
    /// Entity was deleted
    #[serde(rename = "DELETE")]
    Deleted,
    /// Custom event type
    #[serde(rename = "CUSTOM")]
    Custom,
}

impl EventKind {
    /// Convert to string representation
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            EventKind::Created => "INSERT",
            EventKind::Updated => "UPDATE",
            EventKind::Deleted => "DELETE",
            EventKind::Custom => "CUSTOM",
        }
    }
}

/// Changes to a field (old vs new value)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldChanges {
    /// Old value before update
    pub old: serde_json::Value,
    /// New value after update
    pub new: serde_json::Value,
}

/// Entity event from database mutation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityEvent {
    /// Unique event identifier
    pub id:             Uuid,
    /// Type of event (INSERT, UPDATE, DELETE, CUSTOM)
    pub event_type:     EventKind,
    /// Entity type name (e.g., "Order", "User", "Product")
    pub entity_type:    String,
    /// Entity instance ID
    pub entity_id:      Uuid,
    /// Current entity data
    pub data:           serde_json::Value,
    /// Field changes (for UPDATE events)
    pub changes:        Option<std::collections::HashMap<String, FieldChanges>>,
    /// User ID from auth context (if available)
    pub user_id:        Option<String>,
    /// Tenant ID for multi-tenant isolation (if applicable). Public-facing UUID
    /// partition stamp (Change-Spine envelope column `tenant_id`).
    #[serde(default)]
    pub tenant_id:      Option<String>,
    /// When the event occurred
    pub timestamp:      DateTime<Utc>,
    /// Wall-clock duration of the originating mutation in milliseconds, when the
    /// producer stamped it (Change-Spine perf column `duration_ms`). `None` for
    /// cooperative external producers that do not record timing.
    #[serde(default)]
    pub duration_ms:    Option<i32>,
    /// Monotonic Change-Spine sequence (`seq`) for durable ordering and dedup on
    /// `(object_type, seq)`. `None` when the source row carried no sequence.
    #[serde(default)]
    pub seq:            Option<i64>,
    /// Actor classification of the request that produced this change
    /// (Change-Spine envelope column `actor_type`): `"human_user"`,
    /// `"service_account"`, `"ai_agent"`, or `"system_job"` (#390). Recorded for
    /// forensics / downstream fan-out, never an authorization input. `None` for
    /// cooperative external producers that did not stamp it.
    #[serde(default)]
    pub actor_type:     Option<String>,
    /// For a delegated-agent request (RFC 8693 `act` claim), the public-facing
    /// UUID of the underlying human the agent acts for (Change-Spine envelope
    /// column `acting_for`, #390). Mirrors [`tenant_id`](Self::tenant_id)'s
    /// stringified-UUID shape. `None` for non-delegated requests.
    #[serde(default)]
    pub acting_for:     Option<String>,
    /// Schema version of the producer that wrote this change (Change-Spine
    /// envelope column `schema_version`, #377) — the application schema the
    /// originating mutation ran against. Recorded for cross-version forensics and
    /// deploy audit (e.g. "which schema produced this dead-lettered action"); see
    /// `docs/operations/zero-downtime-deploys.md`. `None` when the producer did
    /// not stamp it.
    #[serde(default)]
    pub schema_version: Option<String>,
    /// The change-data-capture *origin* of this row (`extra_metadata.cdc_source`,
    /// #366). `Some("fallback_trigger")` marks an **externally-captured** write (a
    /// third-party daemon / `psql` INSERT caught by the shipped capture trigger);
    /// FraiseQL's own executor-written rows carry no such marker. `after:capture`
    /// function dispatch keys on this so a mediated/executor write never re-enters
    /// the capture path (loop safety). `None` when the row is not a captured write.
    #[serde(default)]
    pub cdc_source:     Option<String>,
}

impl EntityEvent {
    /// Create a new entity event
    #[must_use]
    pub fn new(
        event_type: EventKind,
        entity_type: String,
        entity_id: Uuid,
        data: serde_json::Value,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            event_type,
            entity_type,
            entity_id,
            data,
            changes: None,
            user_id: None,
            tenant_id: None,
            timestamp: Utc::now(),
            duration_ms: None,
            seq: None,
            actor_type: None,
            acting_for: None,
            schema_version: None,
            cdc_source: None,
        }
    }

    /// Set the `user_id` for this event
    #[must_use]
    pub fn with_user_id(mut self, user_id: String) -> Self {
        self.user_id = Some(user_id);
        self
    }

    /// Set the `tenant_id` for this event (for multi-tenant isolation)
    #[must_use]
    pub fn with_tenant_id(mut self, tenant_id: impl Into<String>) -> Self {
        self.tenant_id = Some(tenant_id.into());
        self
    }

    /// Set the mutation `duration_ms` for this event (Change-Spine perf column)
    #[must_use]
    pub const fn with_duration_ms(mut self, duration_ms: i32) -> Self {
        self.duration_ms = Some(duration_ms);
        self
    }

    /// Set the Change-Spine `seq` for this event (durable ordering / dedup)
    #[must_use]
    pub const fn with_seq(mut self, seq: i64) -> Self {
        self.seq = Some(seq);
        self
    }

    /// Set the Change-Spine `actor_type` classification for this event (#390)
    #[must_use]
    pub fn with_actor_type(mut self, actor_type: impl Into<String>) -> Self {
        self.actor_type = Some(actor_type.into());
        self
    }

    /// Set the Change-Spine `acting_for` delegated-human UUID for this event (#390)
    #[must_use]
    pub fn with_acting_for(mut self, acting_for: impl Into<String>) -> Self {
        self.acting_for = Some(acting_for.into());
        self
    }

    /// Set the Change-Spine `schema_version` for this event (#377)
    #[must_use]
    pub fn with_schema_version(mut self, schema_version: impl Into<String>) -> Self {
        self.schema_version = Some(schema_version.into());
        self
    }

    /// Set field changes for UPDATE events
    #[must_use]
    pub fn with_changes(
        mut self,
        changes: std::collections::HashMap<String, FieldChanges>,
    ) -> Self {
        self.changes = Some(changes);
        self
    }

    /// Check if a field changed value
    #[must_use]
    pub fn field_changed(&self, field_name: &str) -> bool {
        self.changes.as_ref().is_some_and(|changes| changes.contains_key(field_name))
    }

    /// Check if a field changed to a specific value.
    ///
    /// Numbers compare **numerically** (`100.00 == 100`), not by JSON
    /// representation — a PostgreSQL `numeric` column arrives as `100.00`
    /// while a condition literal parses as the integer `100` (#843).
    #[must_use]
    pub fn field_changed_to(&self, field_name: &str, expected_value: &serde_json::Value) -> bool {
        self.changes
            .as_ref()
            .and_then(|changes| changes.get(field_name))
            .is_some_and(|change| json_semantic_eq(&change.new, expected_value))
    }

    /// Check if a field changed from a specific value.
    ///
    /// Numbers compare numerically, as in [`Self::field_changed_to`] (#843).
    #[must_use]
    pub fn field_changed_from(&self, field_name: &str, expected_value: &serde_json::Value) -> bool {
        self.changes
            .as_ref()
            .and_then(|changes| changes.get(field_name))
            .is_some_and(|change| json_semantic_eq(&change.old, expected_value))
    }

    /// Check if this is a new entity (no old value for any field)
    #[must_use]
    pub fn is_new(&self) -> bool {
        self.event_type == EventKind::Created
    }

    /// Check if this is a delete event
    #[must_use]
    pub fn is_deleted(&self) -> bool {
        self.event_type == EventKind::Deleted
    }
}

/// Semantic JSON equality: numbers compare **numerically**, everything else by
/// strict `serde_json::Value` equality (#843).
///
/// `serde_json`'s `PartialEq` is representation-strict across `Number` variants,
/// so `Number(PosInt(100)) != Number(Float(100.0))` — which made `total == 100`
/// false for a PostgreSQL `numeric(10,2)` value of `100.00` while `>=`/`<=`
/// (which coerce) reported the same operands equal. Integers compare exactly
/// (`i64`/`u64` first, so values above 2^53 do not round through `f64`); the
/// `f64` fallback covers genuine floats and mixed representations. Comparison
/// is shallow: numbers nested inside arrays/objects still use strict equality.
#[allow(clippy::float_cmp)]
// Reason: exact f64 equality is the intended semantic — 100.00 == 100 after
// coercion, with no epsilon tolerance to surprise operators.
pub(crate) fn json_semantic_eq(left: &serde_json::Value, right: &serde_json::Value) -> bool {
    match (left, right) {
        (serde_json::Value::Number(l), serde_json::Value::Number(r)) => {
            if let (Some(li), Some(ri)) = (l.as_i64(), r.as_i64()) {
                return li == ri;
            }
            if let (Some(lu), Some(ru)) = (l.as_u64(), r.as_u64()) {
                return lu == ru;
            }
            match (l.as_f64(), r.as_f64()) {
                (Some(lf), Some(rf)) => lf == rf,
                _ => false,
            }
        },
        _ => left == right,
    }
}

#[cfg(test)]
mod tests;
