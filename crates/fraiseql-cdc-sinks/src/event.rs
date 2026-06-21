//! The in-memory change event — one drained `core.tb_entity_change_log` row.
//!
//! The parent #382 plan refers to a `ChangeEvent` envelope living in a
//! `fraiseql-events` crate; that crate does not exist. The real drain source is
//! the framework-owned `core.tb_entity_change_log` outbox (migration
//! `08_create_entity_change_log_contract.sql`), so [`ChangeEvent`] is defined
//! here, in the only crate that consumes it.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

/// The change operation that produced an outbox row.
///
/// Parsed from the `modification_type` column. Anything that is not a plain
/// `INSERT`/`UPDATE`/`DELETE` (e.g. a custom mutation verb) maps to
/// [`ChangeOp::Custom`], mirroring the change-log reader's `r` (read/other)
/// Debezium fallback.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum ChangeOp {
    /// A row was created.
    Insert,
    /// A row was modified.
    Update,
    /// A row was removed.
    Delete,
    /// Any other producer verb (custom mutation).
    Custom,
}

impl ChangeOp {
    /// Parse from a `modification_type` value (case-insensitive).
    #[must_use]
    pub fn from_modification_type(modification_type: &str) -> Self {
        match modification_type.to_ascii_uppercase().as_str() {
            "INSERT" => Self::Insert,
            "UPDATE" => Self::Update,
            "DELETE" => Self::Delete,
            _ => Self::Custom,
        }
    }

    /// The lowercase op name, safe for use as a subject/topic segment.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Insert => "insert",
            Self::Update => "update",
            Self::Delete => "delete",
            Self::Custom => "custom",
        }
    }

    /// The single-letter Debezium-style op code (`c`/`u`/`d`/`r`).
    #[must_use]
    pub const fn debezium_code(self) -> char {
        match self {
            Self::Insert => 'c',
            Self::Update => 'u',
            Self::Delete => 'd',
            Self::Custom => 'r',
        }
    }
}

/// One drained `core.tb_entity_change_log` outbox row, ready to publish.
///
/// `after` is the uniform after-image (`object_data`); `before` is the opt-in
/// pre-image (`object_data_before`, `NULL` unless the producer opted in). `seq`
/// is the load-bearing global ordering / dedup key (the consumer dedup contract
/// is `(object_type, seq)`).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct ChangeEvent {
    /// Monotonic global sequence (the `seq` outbox column) — ordering + dedup key.
    pub seq:         i64,
    /// The physical table / GraphQL object type (the `object_type` column).
    pub object_type: String,
    /// The change operation (parsed from `modification_type`).
    pub op:          ChangeOp,
    /// The per-tenant partition stamp (the `tenant_id` column), if any.
    pub tenant_id:   Option<Uuid>,
    /// The changed entity's public-facing identifier (the `object_id` column).
    pub object_id:   Option<Uuid>,
    /// The after-image (`object_data`); `None`/`Null` for a delete or no-payload row.
    pub after:       Option<Value>,
    /// The opt-in pre-image (`object_data_before`); `None` unless opted in.
    pub before:      Option<Value>,
    /// The commit timestamp (the `commit_time` column), if recorded.
    pub commit_time: Option<DateTime<Utc>>,
}

impl ChangeEvent {
    /// Construct a minimal event (no tenant/payload). Use the `with_*` builders
    /// to add optional fields.
    #[must_use]
    pub fn new(seq: i64, object_type: impl Into<String>, op: ChangeOp) -> Self {
        Self {
            seq,
            object_type: object_type.into(),
            op,
            tenant_id: None,
            object_id: None,
            after: None,
            before: None,
            commit_time: None,
        }
    }

    /// Set the tenant partition stamp.
    #[must_use]
    pub const fn with_tenant(mut self, tenant_id: Uuid) -> Self {
        self.tenant_id = Some(tenant_id);
        self
    }

    /// Set the changed-entity identifier.
    #[must_use]
    pub const fn with_object_id(mut self, object_id: Uuid) -> Self {
        self.object_id = Some(object_id);
        self
    }

    /// Set the after-image payload.
    #[must_use]
    pub fn with_after(mut self, after: Value) -> Self {
        self.after = Some(after);
        self
    }

    /// Set the pre-image payload.
    #[must_use]
    pub fn with_before(mut self, before: Value) -> Self {
        self.before = Some(before);
        self
    }
}

#[cfg(test)]
mod tests;
