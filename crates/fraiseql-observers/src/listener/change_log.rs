//! Change log listener that polls `tb_entity_change_log` for entity mutations.
//!
//! This module implements a durable event listener that:
//! 1. Polls `tb_entity_change_log` for new entries
//! 2. Parses Debezium envelope format (before/after/op/source)
//! 3. Converts entries to `EntityEvent` for observer processing
//! 4. Maintains checkpoint for recovery after restarts
//! 5. Handles backpressure and batch processing

use std::{collections::HashMap, time::Duration};

use chrono::{DateTime, TimeZone, Utc};
use serde_json::Value;
use sqlx::postgres::PgPool;
use tracing::{debug, error, info};
use uuid::Uuid;

use crate::{
    error::{ObserverError, Result},
    event::{EntityEvent, EventKind, FieldChanges},
};

/// Row type returned from the `tb_entity_change_log` query.
type ChangeLogRow = (
    i64,
    Uuid,
    Option<String>,
    Option<String>,
    String,
    String,
    String,
    Option<String>,
    Value,
    Option<Value>,
    Option<DateTime<Utc>>,
);

/// Configuration for the change log listener
#[derive(Debug, Clone)]
pub struct ChangeLogListenerConfig {
    /// PostgreSQL connection pool
    pub pool: PgPool,

    /// How often to poll the change log (milliseconds)
    pub poll_interval_ms: u64,

    /// Maximum events to fetch per batch
    pub batch_size: usize,

    /// Resume from this change log ID (for recovery)
    pub resume_from_id: Option<i64>,
}

impl ChangeLogListenerConfig {
    /// Create config with defaults
    #[must_use]
    pub const fn new(pool: PgPool) -> Self {
        Self {
            pool,
            poll_interval_ms: 100,
            batch_size: 100,
            resume_from_id: None,
        }
    }

    /// Set poll interval
    #[must_use]
    pub const fn with_poll_interval(mut self, ms: u64) -> Self {
        self.poll_interval_ms = ms;
        self
    }

    /// Set batch size
    #[must_use]
    pub const fn with_batch_size(mut self, size: usize) -> Self {
        self.batch_size = size;
        self
    }

    /// Set resume checkpoint
    #[must_use]
    pub const fn with_resume_from(mut self, id: i64) -> Self {
        self.resume_from_id = Some(id);
        self
    }
}

/// Single entry from `tb_entity_change_log`
#[derive(Debug, Clone)]
pub struct ChangeLogEntry {
    /// Row ID (bigserial)
    pub id: i64,

    /// UUID for the entry
    pub pk_entity_change_log: String,

    /// Multi-tenant: organization that owns this change
    pub fk_customer_org: String,

    /// User ID who made the change (optional)
    pub fk_contact: Option<String>,

    /// Entity type (User, Order, Product, etc.)
    pub object_type: String,

    /// Entity ID (UUID)
    pub object_id: String,

    /// Operation type (INSERT, UPDATE, DELETE, NOOP)
    pub modification_type: String,

    /// Status (success, failed, etc.)
    pub change_status: String,

    /// Debezium envelope with before/after/op/source
    pub object_data: Value,

    /// Additional metadata (JSON)
    pub extra_metadata: Option<Value>,

    /// When the change was recorded
    pub created_at: String,
}

impl ChangeLogEntry {
    /// Parse Debezium envelope to get operation code
    pub fn debezium_operation(&self) -> Result<char> {
        self.object_data
            .get("op")
            .and_then(|v| v.as_str())
            .and_then(|s| s.chars().next())
            .ok_or_else(|| ObserverError::TemplateRenderingFailed {
                reason: "Missing 'op' field in Debezium envelope".to_string(),
            })
    }

    /// Get "after" values (entity state after change)
    pub fn after_values(&self) -> Result<Value> {
        self.object_data.get("after").cloned().ok_or_else(|| {
            ObserverError::TemplateRenderingFailed {
                reason: "Missing 'after' field in Debezium envelope".to_string(),
            }
        })
    }

    /// Get "before" values (entity state before change)
    #[must_use]
    pub fn before_values(&self) -> Option<Value> {
        self.object_data.get("before").cloned()
    }

    /// Convert to `EntityEvent` for observer processing
    pub fn to_entity_event(&self) -> Result<EntityEvent> {
        // Map operation code to EventKind
        let event_kind = match self.debezium_operation()? {
            'c' => EventKind::Created,
            'u' => EventKind::Updated,
            'd' => EventKind::Deleted,
            'r' => EventKind::Custom, // read/noop
            op => {
                return Err(ObserverError::TemplateRenderingFailed {
                    reason: format!("Unknown operation code: {op}"),
                });
            },
        };

        // Parse entity_id from object_id (should be UUID format)
        let entity_id = Uuid::parse_str(&self.object_id).map_err(|e| {
            ObserverError::TemplateRenderingFailed {
                reason: format!("Invalid entity ID (not UUID): {} - {}", self.object_id, e),
            }
        })?;

        // Parse timestamp from created_at (PostgreSQL TIMESTAMPTZ format)
        // PostgreSQL returns: "2026-01-23 12:34:56.123456" or "2026-01-23 12:34:56.123456+00"
        let timestamp = if let Ok(dt) = DateTime::parse_from_rfc3339(&self.created_at) {
            dt.with_timezone(&Utc)
        } else {
            // Try PostgreSQL format without timezone indicator
            let ndt =
                chrono::NaiveDateTime::parse_from_str(&self.created_at, "%Y-%m-%d %H:%M:%S%.f")
                    .map_err(|e| ObserverError::TemplateRenderingFailed {
                        reason: format!("Invalid timestamp format: {} - {}", self.created_at, e),
                    })?;
            Utc.from_utc_datetime(&ndt)
        };

        // Get entity data (use "after" values, or "before" for DELETE)
        let data = if event_kind == EventKind::Deleted {
            self.before_values().unwrap_or(Value::Object(Default::default()))
        } else {
            self.after_values()?
        };

        // Build field changes for UPDATE events
        let changes = if event_kind == EventKind::Updated {
            self.build_field_changes()?
        } else {
            None
        };

        // Use fk_contact as user_id if available
        let user_id = self.fk_contact.clone();

        Ok(EntityEvent {
            id: Uuid::parse_str(&self.pk_entity_change_log).unwrap_or_else(|_| Uuid::new_v4()),
            event_type: event_kind,
            entity_type: self.object_type.clone(),
            entity_id,
            data,
            changes,
            user_id,
            tenant_id: None,
            timestamp,
        })
    }

    /// Build field changes for UPDATE events by comparing before/after
    fn build_field_changes(&self) -> Result<Option<HashMap<String, FieldChanges>>> {
        if self.debezium_operation()? != 'u' {
            return Ok(None);
        }

        let before = match self.before_values() {
            Some(Value::Object(b)) => b,
            _ => return Ok(None),
        };

        let after = match self.after_values()? {
            Value::Object(a) => a,
            _ => return Ok(None),
        };

        let mut changes = HashMap::new();

        // Compare before and after to find changed fields
        for (key, after_val) in &after {
            if let Some(before_val) = before.get(key) {
                if before_val != after_val {
                    changes.insert(
                        key.clone(),
                        FieldChanges {
                            old: before_val.clone(),
                            new: after_val.clone(),
                        },
                    );
                }
            } else {
                // Field added in after (new field)
                changes.insert(
                    key.clone(),
                    FieldChanges {
                        old: Value::Null,
                        new: after_val.clone(),
                    },
                );
            }
        }

        // Check for deleted fields (in before but not in after)
        for (key, before_val) in &before {
            if !after.contains_key(key) {
                changes.insert(
                    key.clone(),
                    FieldChanges {
                        old: before_val.clone(),
                        new: Value::Null,
                    },
                );
            }
        }

        if changes.is_empty() {
            Ok(None)
        } else {
            Ok(Some(changes))
        }
    }
}

/// Change log listener that polls database for mutations
pub struct ChangeLogListener {
    config:            ChangeLogListenerConfig,
    last_processed_id: i64,
}

impl ChangeLogListener {
    /// Create a new change log listener
    #[must_use]
    pub fn new(config: ChangeLogListenerConfig) -> Self {
        let last_processed_id = config.resume_from_id.unwrap_or(0);

        Self {
            config,
            last_processed_id,
        }
    }

    /// Fetch next batch of entries from change log
    pub async fn next_batch(&mut self) -> Result<Vec<ChangeLogEntry>> {
        // Query: SELECT * FROM tb_entity_change_log
        // WHERE pk_entity_change_log > last_processed_id
        // ORDER BY pk_entity_change_log ASC
        // LIMIT batch_size

        let rows: Vec<ChangeLogRow> = sqlx::query_as(
            r"
                SELECT
                    pk_entity_change_log,
                    id,
                    fk_customer_org,
                    fk_contact,
                    object_type,
                    object_id,
                    modification_type,
                    change_status,
                    object_data,
                    extra_metadata,
                    created_at
                FROM core.tb_entity_change_log
                WHERE pk_entity_change_log > $1
                ORDER BY pk_entity_change_log ASC
                LIMIT $2
                ",
        )
        .bind(self.last_processed_id)
        .bind(self.config.batch_size as i64)
        .fetch_all(&self.config.pool)
        .await
        .map_err(|e| ObserverError::DatabaseError {
            reason: format!("Failed to query change log: {e}"),
        })?;

        let mut entries = Vec::new();

        for (pk, id, org, contact, obj_type, obj_id, mod_type, status, data, meta, created) in rows
        {
            let created_at_str =
                created.map_or_else(|| Utc::now().to_rfc3339(), |dt| dt.to_rfc3339());

            entries.push(ChangeLogEntry {
                id:                   pk,
                pk_entity_change_log: id.to_string(),
                fk_customer_org:      org.unwrap_or_default(),
                fk_contact:           contact,
                object_type:          obj_type,
                object_id:            obj_id,
                modification_type:    mod_type,
                change_status:        status.unwrap_or_default(),
                object_data:          data,
                extra_metadata:       meta,
                created_at:           created_at_str,
            });

            // Update checkpoint for recovery
            self.last_processed_id = pk;
        }

        debug!("Fetched {} entries from change log", entries.len());

        Ok(entries)
    }

    /// Get the current checkpoint (last processed ID)
    #[must_use]
    pub const fn checkpoint(&self) -> i64 {
        self.last_processed_id
    }

    /// Set checkpoint (for recovery)
    pub fn set_checkpoint(&mut self, id: i64) {
        self.last_processed_id = id;
    }

    /// Poll indefinitely for events (for background task)
    pub async fn run(&mut self) -> Result<()> {
        info!("Starting change log listener (resume from id: {})", self.last_processed_id);

        loop {
            match self.next_batch().await {
                Ok(entries) => {
                    if !entries.is_empty() {
                        debug!("Fetched {} entries", entries.len());
                    }

                    // Yield control to allow other tasks to run
                    if entries.is_empty() {
                        tokio::time::sleep(Duration::from_millis(self.config.poll_interval_ms))
                            .await;
                    }
                },
                Err(e) => {
                    error!("Error fetching from change log: {}", e);
                    // Back off and retry
                    tokio::time::sleep(Duration::from_secs(1)).await;
                },
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn test_change_log_entry_debezium_operation() {
        let entry = ChangeLogEntry {
            id:                   1,
            pk_entity_change_log: "uuid".to_string(),
            fk_customer_org:      "org".to_string(),
            fk_contact:           None,
            object_type:          "Order".to_string(),
            object_id:            "order-id".to_string(),
            modification_type:    "INSERT".to_string(),
            change_status:        "success".to_string(),
            object_data:          json!({
                "op": "c",
                "before": null,
                "after": { "id": "order-id" }
            }),
            extra_metadata:       None,
            created_at:           "2026-01-22T10:00:00Z".to_string(),
        };

        assert_eq!(entry.debezium_operation().unwrap(), 'c');
    }

    #[test]
    fn test_change_log_entry_after_values() {
        let entry = ChangeLogEntry {
            id:                   1,
            pk_entity_change_log: "uuid".to_string(),
            fk_customer_org:      "org".to_string(),
            fk_contact:           None,
            object_type:          "User".to_string(),
            object_id:            "user-id".to_string(),
            modification_type:    "UPDATE".to_string(),
            change_status:        "success".to_string(),
            object_data:          json!({
                "op": "u",
                "before": { "name": "old" },
                "after": { "name": "new" }
            }),
            extra_metadata:       None,
            created_at:           "2026-01-22T10:00:00Z".to_string(),
        };

        let after = entry.after_values().unwrap();
        assert_eq!(after["name"], "new");
    }

    #[test]
    fn test_change_log_entry_before_values() {
        let entry = ChangeLogEntry {
            id:                   1,
            pk_entity_change_log: "uuid".to_string(),
            fk_customer_org:      "org".to_string(),
            fk_contact:           None,
            object_type:          "Product".to_string(),
            object_id:            "prod-id".to_string(),
            modification_type:    "UPDATE".to_string(),
            change_status:        "success".to_string(),
            object_data:          json!({
                "op": "u",
                "before": { "price": 100 },
                "after": { "price": 150 }
            }),
            extra_metadata:       None,
            created_at:           "2026-01-22T10:00:00Z".to_string(),
        };

        let before = entry.before_values().unwrap();
        assert_eq!(before["price"], 100);
    }

    #[tokio::test]
    async fn test_change_log_listener_checkpoint() {
        let config = ChangeLogListenerConfig::new(
            PgPool::connect_lazy("postgres://localhost/dummy").unwrap(),
        );
        let mut listener = ChangeLogListener::new(config);

        assert_eq!(listener.checkpoint(), 0);

        listener.set_checkpoint(42);
        assert_eq!(listener.checkpoint(), 42);
    }

    #[tokio::test]
    async fn test_config_builder() {
        let pool = PgPool::connect_lazy("postgres://localhost/dummy").unwrap();
        let config = ChangeLogListenerConfig::new(pool)
            .with_poll_interval(500)
            .with_batch_size(50)
            .with_resume_from(100);

        assert_eq!(config.poll_interval_ms, 500);
        assert_eq!(config.batch_size, 50);
        assert_eq!(config.resume_from_id, Some(100));
    }

    // Event conversion tests

    #[test]
    fn test_insert_to_entity_event() {
        let entity_id = Uuid::new_v4();
        let entry = ChangeLogEntry {
            id:                   1,
            pk_entity_change_log: Uuid::new_v4().to_string(),
            fk_customer_org:      "org".to_string(),
            fk_contact:           Some("user-123".to_string()),
            object_type:          "Order".to_string(),
            object_id:            entity_id.to_string(),
            modification_type:    "INSERT".to_string(),
            change_status:        "success".to_string(),
            object_data:          json!({
                "op": "c",
                "before": null,
                "after": { "id": entity_id.to_string(), "total": 150.00, "status": "pending" }
            }),
            extra_metadata:       None,
            created_at:           "2026-01-22T10:30:00+00:00".to_string(),
        };

        let event = entry.to_entity_event().unwrap();

        assert_eq!(event.event_type, EventKind::Created);
        assert_eq!(event.entity_type, "Order");
        assert_eq!(event.entity_id, entity_id);
        assert_eq!(event.data["total"], 150.00);
        assert_eq!(event.user_id, Some("user-123".to_string()));
        assert!(event.changes.is_none()); // No changes for CREATE
    }

    #[test]
    fn test_update_to_entity_event() {
        let entity_id = Uuid::new_v4();
        let entry = ChangeLogEntry {
            id:                   2,
            pk_entity_change_log: Uuid::new_v4().to_string(),
            fk_customer_org:      "org".to_string(),
            fk_contact:           Some("user-456".to_string()),
            object_type:          "Order".to_string(),
            object_id:            entity_id.to_string(),
            modification_type:    "UPDATE".to_string(),
            change_status:        "success".to_string(),
            object_data:          json!({
                "op": "u",
                "before": { "status": "pending", "total": 100.00 },
                "after": { "status": "shipped", "total": 100.00 }
            }),
            extra_metadata:       None,
            created_at:           "2026-01-22T10:35:00+00:00".to_string(),
        };

        let event = entry.to_entity_event().unwrap();

        assert_eq!(event.event_type, EventKind::Updated);
        assert_eq!(event.data["status"], "shipped");

        // Verify field changes captured
        let changes = event.changes.unwrap();
        assert!(changes.contains_key("status"));
        assert_eq!(changes["status"].old, "pending");
        assert_eq!(changes["status"].new, "shipped");
        // Total unchanged, should not be in changes
        assert!(!changes.contains_key("total"));
    }

    #[test]
    fn test_delete_to_entity_event() {
        let entity_id = Uuid::new_v4();
        let entry = ChangeLogEntry {
            id:                   3,
            pk_entity_change_log: Uuid::new_v4().to_string(),
            fk_customer_org:      "org".to_string(),
            fk_contact:           None,
            object_type:          "User".to_string(),
            object_id:            entity_id.to_string(),
            modification_type:    "DELETE".to_string(),
            change_status:        "success".to_string(),
            object_data:          json!({
                "op": "d",
                "before": { "id": entity_id.to_string(), "email": "user@example.com" },
                "after": null
            }),
            extra_metadata:       None,
            created_at:           "2026-01-22T10:40:00+00:00".to_string(),
        };

        let event = entry.to_entity_event().unwrap();

        assert_eq!(event.event_type, EventKind::Deleted);
        // For DELETE, data should use before values
        assert_eq!(event.data["email"], "user@example.com");
        assert_eq!(event.user_id, None);
    }

    #[test]
    fn test_field_changes_new_field() {
        let entity_id = Uuid::new_v4();
        let entry = ChangeLogEntry {
            id:                   4,
            pk_entity_change_log: Uuid::new_v4().to_string(),
            fk_customer_org:      "org".to_string(),
            fk_contact:           None,
            object_type:          "Product".to_string(),
            object_id:            entity_id.to_string(),
            modification_type:    "UPDATE".to_string(),
            change_status:        "success".to_string(),
            object_data:          json!({
                "op": "u",
                "before": { "name": "Widget" },
                "after": { "name": "Widget", "description": "A useful widget" }
            }),
            extra_metadata:       None,
            created_at:           "2026-01-22T10:45:00+00:00".to_string(),
        };

        let event = entry.to_entity_event().unwrap();
        let changes = event.changes.unwrap();

        // Should have changes for the new field
        assert!(changes.contains_key("description"));
        assert_eq!(changes["description"].old, Value::Null);
        assert_eq!(changes["description"].new, "A useful widget");
    }

    #[test]
    fn test_field_changes_deleted_field() {
        let entity_id = Uuid::new_v4();
        let entry = ChangeLogEntry {
            id:                   5,
            pk_entity_change_log: Uuid::new_v4().to_string(),
            fk_customer_org:      "org".to_string(),
            fk_contact:           None,
            object_type:          "User".to_string(),
            object_id:            entity_id.to_string(),
            modification_type:    "UPDATE".to_string(),
            change_status:        "success".to_string(),
            object_data:          json!({
                "op": "u",
                "before": { "name": "John", "temp_field": "value" },
                "after": { "name": "John" }
            }),
            extra_metadata:       None,
            created_at:           "2026-01-22T10:50:00+00:00".to_string(),
        };

        let event = entry.to_entity_event().unwrap();
        let changes = event.changes.unwrap();

        // Should have changes for the deleted field
        assert!(changes.contains_key("temp_field"));
        assert_eq!(changes["temp_field"].old, "value");
        assert_eq!(changes["temp_field"].new, Value::Null);
    }

    #[test]
    fn test_timestamp_parsing() {
        let entity_id = Uuid::new_v4();
        let entry = ChangeLogEntry {
            id:                   6,
            pk_entity_change_log: Uuid::new_v4().to_string(),
            fk_customer_org:      "org".to_string(),
            fk_contact:           None,
            object_type:          "Order".to_string(),
            object_id:            entity_id.to_string(),
            modification_type:    "INSERT".to_string(),
            change_status:        "success".to_string(),
            object_data:          json!({
                "op": "c",
                "before": null,
                "after": { "id": entity_id.to_string() }
            }),
            extra_metadata:       None,
            created_at:           "2026-01-22T15:30:45.123456+00:00".to_string(),
        };

        let event = entry.to_entity_event().unwrap();

        // Verify timestamp was parsed correctly
        assert!(event.timestamp.to_rfc3339().contains("2026-01-22T15:30:45"));
    }
}
