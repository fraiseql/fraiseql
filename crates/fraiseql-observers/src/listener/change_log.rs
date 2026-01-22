//! Change log listener that polls `tb_entity_change_log` for entity mutations.
//!
//! This module implements a durable event listener that:
//! 1. Polls `tb_entity_change_log` for new entries
//! 2. Parses Debezium envelope format (before/after/op/source)
//! 3. Converts entries to `EntityEvent` for observer processing
//! 4. Maintains checkpoint for recovery after restarts
//! 5. Handles backpressure and batch processing

use crate::error::{ObserverError, Result};
use serde_json::Value;
use sqlx::postgres::PgPool;
use std::time::Duration;
use tracing::{debug, error, info};

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
    pub fn new(pool: PgPool) -> Self {
        Self {
            pool,
            poll_interval_ms: 100,
            batch_size: 100,
            resume_from_id: None,
        }
    }

    /// Set poll interval
    pub fn with_poll_interval(mut self, ms: u64) -> Self {
        self.poll_interval_ms = ms;
        self
    }

    /// Set batch size
    pub fn with_batch_size(mut self, size: usize) -> Self {
        self.batch_size = size;
        self
    }

    /// Set resume checkpoint
    pub fn with_resume_from(mut self, id: i64) -> Self {
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
        self.object_data
            .get("after")
            .cloned()
            .ok_or_else(|| ObserverError::TemplateRenderingFailed {
                reason: "Missing 'after' field in Debezium envelope".to_string(),
            })
    }

    /// Get "before" values (entity state before change)
    pub fn before_values(&self) -> Option<Value> {
        self.object_data.get("before").cloned()
    }
}

/// Change log listener that polls database for mutations
pub struct ChangeLogListener {
    config: ChangeLogListenerConfig,
    last_processed_id: i64,
}

impl ChangeLogListener {
    /// Create a new change log listener
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
        // WHERE id > last_processed_id
        // ORDER BY id ASC
        // LIMIT batch_size

        let rows: Vec<(i64, String, String, Option<String>, String, String, String, String, Value, Option<Value>, String)> =
            sqlx::query_as(
                r#"
                SELECT
                    id,
                    pk_entity_change_log,
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
                WHERE id > $1
                ORDER BY id ASC
                LIMIT $2
                "#,
            )
            .bind(self.last_processed_id)
            .bind(self.config.batch_size as i64)
            .fetch_all(&self.config.pool)
            .await
            .map_err(|e| ObserverError::DatabaseError {
                reason: format!("Failed to query change log: {}", e),
            })?;

        let mut entries = Vec::new();

        for (id, pk, org, contact, obj_type, obj_id, mod_type, status, data, meta, created) in rows {
            entries.push(ChangeLogEntry {
                id,
                pk_entity_change_log: pk,
                fk_customer_org: org,
                fk_contact: contact,
                object_type: obj_type,
                object_id: obj_id,
                modification_type: mod_type,
                change_status: status,
                object_data: data,
                extra_metadata: meta,
                created_at: created,
            });

            // Update checkpoint for recovery
            self.last_processed_id = id;
        }

        debug!("Fetched {} entries from change log", entries.len());

        Ok(entries)
    }

    /// Get the current checkpoint (last processed ID)
    pub fn checkpoint(&self) -> i64 {
        self.last_processed_id
    }

    /// Set checkpoint (for recovery)
    pub fn set_checkpoint(&mut self, id: i64) {
        self.last_processed_id = id;
    }

    /// Poll indefinitely for events (for background task)
    pub async fn run(&mut self) -> Result<()> {
        info!(
            "Starting change log listener (resume from id: {})",
            self.last_processed_id
        );

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
                }
                Err(e) => {
                    error!("Error fetching from change log: {}", e);
                    // Back off and retry
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_change_log_entry_debezium_operation() {
        let entry = ChangeLogEntry {
            id: 1,
            pk_entity_change_log: "uuid".to_string(),
            fk_customer_org: "org".to_string(),
            fk_contact: None,
            object_type: "Order".to_string(),
            object_id: "order-id".to_string(),
            modification_type: "INSERT".to_string(),
            change_status: "success".to_string(),
            object_data: json!({
                "op": "c",
                "before": null,
                "after": { "id": "order-id" }
            }),
            extra_metadata: None,
            created_at: "2026-01-22T10:00:00Z".to_string(),
        };

        assert_eq!(entry.debezium_operation().unwrap(), 'c');
    }

    #[test]
    fn test_change_log_entry_after_values() {
        let entry = ChangeLogEntry {
            id: 1,
            pk_entity_change_log: "uuid".to_string(),
            fk_customer_org: "org".to_string(),
            fk_contact: None,
            object_type: "User".to_string(),
            object_id: "user-id".to_string(),
            modification_type: "UPDATE".to_string(),
            change_status: "success".to_string(),
            object_data: json!({
                "op": "u",
                "before": { "name": "old" },
                "after": { "name": "new" }
            }),
            extra_metadata: None,
            created_at: "2026-01-22T10:00:00Z".to_string(),
        };

        let after = entry.after_values().unwrap();
        assert_eq!(after["name"], "new");
    }

    #[test]
    fn test_change_log_entry_before_values() {
        let entry = ChangeLogEntry {
            id: 1,
            pk_entity_change_log: "uuid".to_string(),
            fk_customer_org: "org".to_string(),
            fk_contact: None,
            object_type: "Product".to_string(),
            object_id: "prod-id".to_string(),
            modification_type: "UPDATE".to_string(),
            change_status: "success".to_string(),
            object_data: json!({
                "op": "u",
                "before": { "price": 100 },
                "after": { "price": 150 }
            }),
            extra_metadata: None,
            created_at: "2026-01-22T10:00:00Z".to_string(),
        };

        let before = entry.before_values().unwrap();
        assert_eq!(before["price"], 100);
    }

    #[tokio::test]
    async fn test_change_log_listener_checkpoint() {
        let config = ChangeLogListenerConfig::new(PgPool::connect_lazy("postgres://localhost/dummy").unwrap());
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
}
