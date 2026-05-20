//! MySQL to NATS bridge implementation.
//!
//! This module provides the outbox-style bridge that reliably publishes
//! MySQL entity change events to NATS `JetStream`.
//!
//! # Architecture
//!
//! ```text
//! MySQL (tb_entity_change_log)
//!     │
//!     ↓ cursor-based polling
//! MySQLNatsBridge
//!     │
//!     ↓ publish + ACK
//! NATS JetStream
//! ```
//!
//! # Differences from PostgreSQL Bridge
//!
//! - **No LISTEN/NOTIFY**: MySQL uses pure polling (no wake-up signals)
//! - **UUID format**: MySQL uses CHAR(36) for UUIDs
//! - **Timestamps**: MySQL uses TIMESTAMP instead of TIMESTAMPTZ
//!
//! # Guarantees
//!
//! Same as PostgreSQL bridge:
//! - **Zero data loss**: `tb_entity_change_log` is the durable source of truth
//! - **At-least-once delivery**: Events may be published multiple times
//! - **Crash recovery**: Checkpoint-based resumption from last processed ID

use std::{sync::Arc, time::Duration};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::mysql::MySqlPool;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use super::CheckpointStore;
#[cfg(all(feature = "mysql", feature = "nats"))]
use super::{EventTransport, NatsTransport};
use crate::{
    error::{ObserverError, Result},
    event::EntityEvent,
};

// ============================================================================
// MySQLCheckpointStore Implementation
// ============================================================================

/// MySQL-backed checkpoint store.
///
/// Stores checkpoints in `tb_transport_checkpoint` table with
/// UPSERT semantics for crash-safe persistence.
#[derive(Clone)]
#[cfg(feature = "mysql")]
pub struct MySQLCheckpointStore {
    pool: MySqlPool,
}

#[cfg(feature = "mysql")]
impl MySQLCheckpointStore {
    /// Create a new MySQL checkpoint store.
    #[must_use]
    pub const fn new(pool: MySqlPool) -> Self {
        Self { pool }
    }
}

#[cfg(feature = "mysql")]
// Reason: CheckpointStore is defined with #[async_trait]; all implementations must match
// its transformed method signatures to satisfy the trait contract
// async_trait: dyn-dispatch required; remove when RTN + Send is stable (RFC 3425)
#[async_trait]
impl CheckpointStore for MySQLCheckpointStore {
    async fn get_checkpoint(&self, transport_name: &str) -> Result<Option<i64>> {
        let row: Option<(i64,)> =
            sqlx::query_as("SELECT last_pk FROM tb_transport_checkpoint WHERE transport_name = ?")
                .bind(transport_name)
                .fetch_optional(&self.pool)
                .await
                .map_err(|e| ObserverError::DatabaseError {
                    reason: format!("MySQL checkpoint query failed: {e}"),
                })?;

        Ok(row.map(|(last_pk,)| last_pk))
    }

    async fn save_checkpoint(&self, transport_name: &str, cursor: i64) -> Result<()> {
        // MySQL UPSERT syntax: INSERT ... ON DUPLICATE KEY UPDATE
        sqlx::query(
            r"
            INSERT INTO tb_transport_checkpoint (transport_name, last_pk, updated_at)
            VALUES (?, ?, NOW())
            ON DUPLICATE KEY UPDATE last_pk = VALUES(last_pk), updated_at = NOW()
            ",
        )
        .bind(transport_name)
        .bind(cursor)
        .execute(&self.pool)
        .await
        .map_err(|e| ObserverError::DatabaseError {
            reason: format!("MySQL checkpoint save failed: {e}"),
        })?;

        Ok(())
    }

    async fn delete_checkpoint(&self, transport_name: &str) -> Result<()> {
        sqlx::query("DELETE FROM tb_transport_checkpoint WHERE transport_name = ?")
            .bind(transport_name)
            .execute(&self.pool)
            .await
            .map_err(|e| ObserverError::DatabaseError {
                reason: format!("MySQL checkpoint delete failed: {e}"),
            })?;

        Ok(())
    }
}

// ============================================================================
// MySQLChangeLogEntry (Row from tb_entity_change_log)
// ============================================================================

/// Row from MySQL `tb_entity_change_log` table.
///
/// Represents a single entity change event stored in MySQL.
#[derive(Debug, Clone)]
#[cfg(feature = "mysql")]
pub struct MySQLChangeLogEntry {
    /// Primary key (used as cursor)
    pub pk_entity_change_log: i64,

    /// UUID identifier for the change (stored as CHAR(36))
    pub id: String,

    /// Customer organization ID (tenant)
    pub fk_customer_org: Option<i64>,

    /// Contact ID (user who made the change)
    pub fk_contact: Option<i64>,

    /// Entity type (e.g., "Order", "User")
    pub object_type: String,

    /// Entity ID (UUID of the changed entity, stored as CHAR(36))
    pub object_id: String,

    /// Modification type: INSERT, UPDATE, DELETE
    pub modification_type: String,

    /// Change status (e.g., "pending", "processed")
    pub change_status: Option<String>,

    /// Entity data as JSON
    pub object_data: Option<Value>,

    /// Extra metadata as JSON
    pub extra_metadata: Option<Value>,

    /// When the change was created
    pub created_at: DateTime<Utc>,

    /// When the change was published to NATS (None = not published)
    pub nats_published_at: Option<DateTime<Utc>>,

    /// NATS event ID (for deduplication, stored as CHAR(36))
    pub nats_event_id: Option<String>,
}

#[cfg(feature = "mysql")]
impl MySQLChangeLogEntry {
    /// Convert to `EntityEvent` for publishing.
    ///
    /// # Errors
    ///
    /// Returns [`ObserverError::InvalidConfig`] if `modification_type` is not one
    /// of `INSERT`, `UPDATE`, or `DELETE`.
    pub fn to_entity_event(&self) -> Result<EntityEvent> {
        use crate::event::EventKind;

        let event_type = match self.modification_type.to_uppercase().as_str() {
            "INSERT" => EventKind::Created,
            "UPDATE" => EventKind::Updated,
            "DELETE" => EventKind::Deleted,
            other => {
                return Err(ObserverError::InvalidConfig {
                    message: format!("Unknown modification type: {other}"),
                });
            },
        };

        // Parse UUIDs from string format
        let event_id = self
            .nats_event_id
            .as_ref()
            .and_then(|s| Uuid::parse_str(s).ok())
            .unwrap_or_else(Uuid::new_v4);

        let entity_id =
            Uuid::parse_str(&self.object_id).map_err(|e| ObserverError::InvalidConfig {
                message: format!("Invalid object_id UUID: {e}"),
            })?;

        let mut event = EntityEvent::new(
            event_type,
            self.object_type.clone(),
            entity_id,
            self.object_data.clone().unwrap_or(Value::Null),
        );

        // Override the auto-generated ID with the change log's ID
        event.id = event_id;

        // Set user ID from contact if available
        if let Some(contact_id) = self.fk_contact {
            event.user_id = Some(contact_id.to_string());
        }

        Ok(event)
    }
}

// Implement FromRow manually since MySQL JSON handling differs
#[cfg(feature = "mysql")]
impl<'r> sqlx::FromRow<'r, sqlx::mysql::MySqlRow> for MySQLChangeLogEntry {
    fn from_row(row: &'r sqlx::mysql::MySqlRow) -> std::result::Result<Self, sqlx::Error> {
        use sqlx::Row;

        Ok(Self {
            pk_entity_change_log: row.try_get("pk_entity_change_log")?,
            id: row.try_get("id")?,
            fk_customer_org: row.try_get("fk_customer_org")?,
            fk_contact: row.try_get("fk_contact")?,
            object_type: row.try_get("object_type")?,
            object_id: row.try_get("object_id")?,
            modification_type: row.try_get("modification_type")?,
            change_status: row.try_get("change_status")?,
            object_data: row.try_get("object_data")?,
            extra_metadata: row.try_get("extra_metadata")?,
            created_at: row.try_get("created_at")?,
            nats_published_at: row.try_get("nats_published_at")?,
            nats_event_id: row.try_get("nats_event_id")?,
        })
    }
}

// ============================================================================
// MySQLBridgeConfig
// ============================================================================

/// MySQL bridge configuration.
#[derive(Debug, Clone)]
#[cfg(feature = "mysql")]
pub struct MySQLBridgeConfig {
    /// Transport name for checkpoint storage (e.g., `mysql_to_nats`)
    pub transport_name: String,

    /// Batch size for fetching entries
    pub batch_size: usize,

    /// Poll interval (seconds) - MySQL has no LISTEN/NOTIFY
    pub poll_interval_secs: u64,
}

#[cfg(feature = "mysql")]
impl Default for MySQLBridgeConfig {
    fn default() -> Self {
        Self {
            transport_name: "mysql_to_nats".to_string(),
            batch_size: 100,
            poll_interval_secs: 1,
        }
    }
}

// ============================================================================
// MySQLNatsBridge
// ============================================================================

/// MySQL to NATS bridge.
///
/// Reliably publishes entity change events from MySQL's
/// `tb_entity_change_log` to NATS `JetStream` using cursor-based polling.
///
/// # Key Differences from PostgreSQL Bridge
///
/// - **No LISTEN/NOTIFY**: Uses pure polling with configurable interval
/// - **UUID format**: MySQL stores UUIDs as CHAR(36), requires parsing
/// - **Timestamp type**: MySQL uses TIMESTAMP, not TIMESTAMPTZ
///
/// # Design Properties
///
/// Same as PostgreSQL bridge:
/// 1. CURSOR-based polling ensures no missed events
/// 2. Checkpoint persistence enables crash recovery
/// 3. Conditional `mark_published` prevents races
/// 4. At-least-once delivery (consumers must be idempotent)
#[cfg(all(feature = "mysql", feature = "nats"))]
pub struct MySQLNatsBridge {
    pool: MySqlPool,
    nats_transport: Arc<NatsTransport>,
    checkpoint_store: Arc<dyn CheckpointStore>,
    config: MySQLBridgeConfig,
}

#[cfg(all(feature = "mysql", feature = "nats"))]
impl MySQLNatsBridge {
    /// Create a new MySQL-to-NATS bridge.
    pub fn new(
        pool: MySqlPool,
        nats_transport: Arc<NatsTransport>,
        checkpoint_store: Arc<dyn CheckpointStore>,
        config: MySQLBridgeConfig,
    ) -> Self {
        Self {
            pool,
            nats_transport,
            checkpoint_store,
            config,
        }
    }

    /// Create with default configuration.
    pub fn with_defaults(
        pool: MySqlPool,
        nats_transport: Arc<NatsTransport>,
        checkpoint_store: Arc<dyn CheckpointStore>,
    ) -> Self {
        Self::new(pool, nats_transport, checkpoint_store, MySQLBridgeConfig::default())
    }

    /// Load last processed cursor from checkpoint store.
    async fn load_cursor(&self) -> Result<i64> {
        let cursor = self
            .checkpoint_store
            .get_checkpoint(&self.config.transport_name)
            .await?
            .unwrap_or(0);

        Ok(cursor)
    }

    /// Save cursor checkpoint.
    async fn save_cursor(&self, cursor: i64) -> Result<()> {
        self.checkpoint_store.save_checkpoint(&self.config.transport_name, cursor).await
    }

    /// Fetch batch from cursor.
    async fn fetch_batch_from_cursor(&self, cursor: i64) -> Result<Vec<MySQLChangeLogEntry>> {
        #[allow(clippy::cast_possible_wrap)]
        // Reason: value is non-negative; wrap cannot occur in practice
        let entries: Vec<MySQLChangeLogEntry> = sqlx::query_as(
            r"
            SELECT pk_entity_change_log, id, fk_customer_org, fk_contact,
                   object_type, object_id, modification_type, change_status,
                   object_data, extra_metadata, created_at,
                   nats_published_at, nats_event_id
            FROM tb_entity_change_log
            WHERE pk_entity_change_log > ?
            ORDER BY pk_entity_change_log ASC
            LIMIT ?
            ",
        )
        .bind(cursor)
        .bind(self.config.batch_size as i64)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| ObserverError::DatabaseError {
            reason: format!("MySQL fetch batch failed: {e}"),
        })?;

        Ok(entries)
    }

    /// Mark event as published (idempotent, safe against races).
    async fn mark_published(&self, pk_change_log: i64, event_id: Uuid) -> Result<bool> {
        let result = sqlx::query(
            r"
            UPDATE tb_entity_change_log
            SET nats_published_at = NOW(),
                nats_event_id = ?
            WHERE pk_entity_change_log = ?
              AND nats_published_at IS NULL
            ",
        )
        .bind(event_id.to_string())
        .bind(pk_change_log)
        .execute(&self.pool)
        .await
        .map_err(|e| ObserverError::DatabaseError {
            reason: format!("MySQL mark_published failed: {e}"),
        })?;

        Ok(result.rows_affected() == 1)
    }

    /// Main bridge loop.
    ///
    /// Unlike PostgreSQL, MySQL has no LISTEN/NOTIFY, so this uses
    /// pure polling with configurable interval.
    ///
    /// # Errors
    ///
    /// Propagates database query errors or NATS publish errors encountered
    /// while fetching or forwarding change log entries.
    pub async fn run(&self) -> Result<()> {
        info!("Starting MySQL → NATS bridge: {}", self.config.transport_name);

        let mut cursor = self.load_cursor().await?;
        info!("Bridge starting from cursor: {cursor}");

        loop {
            let entries = self.fetch_batch_from_cursor(cursor).await?;

            if entries.is_empty() {
                // No entries, wait for poll interval
                debug!("No new entries, sleeping for {}s", self.config.poll_interval_secs);
                tokio::time::sleep(Duration::from_secs(self.config.poll_interval_secs)).await;
            } else {
                debug!("Processing {} entries from cursor {}", entries.len(), cursor);

                for entry in entries {
                    // Check if already published (skip if so)
                    if entry.nats_published_at.is_some() {
                        debug!(
                            "Entry pk={} already published, skipping (advancing cursor)",
                            entry.pk_entity_change_log
                        );
                        cursor = entry.pk_entity_change_log;
                        continue;
                    }

                    // Convert to EntityEvent
                    let event = match entry.to_entity_event() {
                        Ok(e) => e,
                        Err(e) => {
                            warn!(
                                "Failed to convert entry pk={} to event: {}. Skipping.",
                                entry.pk_entity_change_log, e
                            );
                            cursor = entry.pk_entity_change_log;
                            continue;
                        },
                    };

                    // Publish to NATS JetStream
                    match self.nats_transport.publish(event.clone()).await {
                        Ok(()) => {
                            let was_first =
                                self.mark_published(entry.pk_entity_change_log, event.id).await?;

                            if was_first {
                                debug!(
                                    "Published event {} (cursor: {})",
                                    event.id, entry.pk_entity_change_log
                                );
                            } else {
                                debug!(
                                    "Event {} already published by another bridge, safe skip",
                                    event.id
                                );
                            }

                            cursor = entry.pk_entity_change_log;
                        },
                        Err(e) => {
                            error!(
                                "Failed to publish event {} to NATS: {}. Will retry.",
                                event.id, e
                            );
                            break;
                        },
                    }
                }

                self.save_cursor(cursor).await?;
            }
        }
    }

    /// Run with graceful shutdown support.
    ///
    /// # Errors
    ///
    /// Returns an error if the MySQL polling connection fails, cursor
    /// loading/saving fails, or NATS publishing encounters an unrecoverable
    /// error.
    pub async fn run_with_shutdown(
        &self,
        mut shutdown: tokio::sync::broadcast::Receiver<()>,
    ) -> Result<()> {
        info!(
            "Starting MySQL → NATS bridge with shutdown support: {}",
            self.config.transport_name
        );

        let mut cursor = self.load_cursor().await?;
        info!("Bridge starting from cursor: {cursor}");

        loop {
            // Check for shutdown signal
            if shutdown.try_recv().is_ok() {
                info!("Shutdown signal received, stopping bridge");
                self.save_cursor(cursor).await?;
                return Ok(());
            }

            let entries = self.fetch_batch_from_cursor(cursor).await?;

            if entries.is_empty() {
                // Poll interval with shutdown check
                let poll_duration = Duration::from_secs(self.config.poll_interval_secs);

                tokio::select! {
                    _ = shutdown.recv() => {
                        info!("Shutdown signal received during wait, stopping bridge");
                        self.save_cursor(cursor).await?;
                        return Ok(());
                    }
                    () = tokio::time::sleep(poll_duration) => {
                        debug!("Poll interval timeout");
                    }
                }
            } else {
                for entry in entries {
                    if entry.nats_published_at.is_some() {
                        cursor = entry.pk_entity_change_log;
                        continue;
                    }

                    let event = match entry.to_entity_event() {
                        Ok(e) => e,
                        Err(e) => {
                            warn!(
                                "Failed to convert entry pk={} to event: {}. Skipping.",
                                entry.pk_entity_change_log, e
                            );
                            cursor = entry.pk_entity_change_log;
                            continue;
                        },
                    };

                    match self.nats_transport.publish(event.clone()).await {
                        Ok(()) => {
                            let _ =
                                self.mark_published(entry.pk_entity_change_log, event.id).await?;
                            cursor = entry.pk_entity_change_log;
                        },
                        Err(e) => {
                            error!("Failed to publish event {}: {}. Retrying.", event.id, e);
                            break;
                        },
                    }
                }

                self.save_cursor(cursor).await?;
            }
        }
    }
}
