//! PostgreSQL to NATS bridge implementation.
//!
//! This module provides the outbox-style bridge that reliably publishes
//! PostgreSQL entity change events to NATS JetStream.
//!
//! # Architecture
//!
//! ```text
//! PostgreSQL (tb_entity_change_log)
//!     │
//!     ↓ cursor-based polling
//! PostgresNatsBridge
//!     │
//!     ↓ publish + ACK
//! NATS JetStream
//! ```
//!
//! # Guarantees
//!
//! - **Zero data loss**: `tb_entity_change_log` is the durable source of truth
//! - **At-least-once delivery**: Events may be published multiple times
//! - **Crash recovery**: Checkpoint-based resumption from last processed ID
//! - **Monotonic progression**: Cursor always moves forward
//!
//! # Non-Guarantees
//!
//! - Does NOT guarantee exactly-once delivery (consumers must be idempotent)
//! - Does NOT guarantee global ordering (only per-subject best-effort)

use std::{sync::Arc, time::Duration};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::{PgPool, postgres::PgListener};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

#[cfg(feature = "nats")]
use super::{EventTransport, NatsTransport};
use crate::{
    error::{ObserverError, Result},
    event::EntityEvent,
};

// ============================================================================
// CheckpointStore Trait
// ============================================================================

/// Checkpoint store for bridge cursor persistence.
///
/// Stores last processed `pk_entity_change_log` per transport, enabling
/// crash recovery and exactly-once processing semantics (from the bridge's
/// perspective).
#[async_trait]
pub trait CheckpointStore: Send + Sync {
    /// Get checkpoint for transport.
    ///
    /// Returns `None` if no checkpoint exists (first run).
    async fn get_checkpoint(&self, transport_name: &str) -> Result<Option<i64>>;

    /// Save checkpoint for transport (idempotent).
    ///
    /// Uses UPSERT semantics - safe to call multiple times with same value.
    async fn save_checkpoint(&self, transport_name: &str, cursor: i64) -> Result<()>;

    /// Delete checkpoint for transport (used for testing/reset).
    async fn delete_checkpoint(&self, transport_name: &str) -> Result<()>;
}

// ============================================================================
// PostgresCheckpointStore Implementation
// ============================================================================

/// PostgreSQL-backed checkpoint store.
///
/// Stores checkpoints in `core.tb_transport_checkpoint` table with
/// UPSERT semantics for crash-safe persistence.
#[derive(Clone)]
pub struct PostgresCheckpointStore {
    pool: PgPool,
}

impl PostgresCheckpointStore {
    /// Create a new PostgreSQL checkpoint store.
    #[must_use]
    pub const fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl CheckpointStore for PostgresCheckpointStore {
    async fn get_checkpoint(&self, transport_name: &str) -> Result<Option<i64>> {
        let row: Option<(i64,)> = sqlx::query_as(
            "SELECT last_pk FROM core.tb_transport_checkpoint WHERE transport_name = $1",
        )
        .bind(transport_name)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|(last_pk,)| last_pk))
    }

    async fn save_checkpoint(&self, transport_name: &str, cursor: i64) -> Result<()> {
        sqlx::query(
            r"
            INSERT INTO core.tb_transport_checkpoint (transport_name, last_pk, updated_at)
            VALUES ($1, $2, NOW())
            ON CONFLICT (transport_name)
            DO UPDATE SET last_pk = EXCLUDED.last_pk, updated_at = NOW()
            ",
        )
        .bind(transport_name)
        .bind(cursor)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn delete_checkpoint(&self, transport_name: &str) -> Result<()> {
        sqlx::query("DELETE FROM core.tb_transport_checkpoint WHERE transport_name = $1")
            .bind(transport_name)
            .execute(&self.pool)
            .await?;

        Ok(())
    }
}

// ============================================================================
// ChangeLogEntry (Row from tb_entity_change_log)
// ============================================================================

/// Row from `core.tb_entity_change_log` table.
///
/// Represents a single entity change event stored in PostgreSQL.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ChangeLogEntry {
    /// Primary key (used as cursor)
    pub pk_entity_change_log: i64,

    /// UUID identifier for the change
    pub id: Uuid,

    /// Customer organization ID (tenant)
    pub fk_customer_org: Option<i64>,

    /// Contact ID (user who made the change)
    pub fk_contact: Option<i64>,

    /// Entity type (e.g., "Order", "User")
    pub object_type: String,

    /// Entity ID (UUID of the changed entity)
    pub object_id: Uuid,

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

    /// NATS event ID (for deduplication)
    pub nats_event_id: Option<Uuid>,
}

impl ChangeLogEntry {
    /// Convert to `EntityEvent` for publishing.
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

        // Use the existing UUID from the change log, or generate new one
        let event_id = self.nats_event_id.unwrap_or_else(Uuid::new_v4);

        let mut event = EntityEvent::new(
            event_type,
            self.object_type.clone(),
            self.object_id,
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

// ============================================================================
// PostgresNatsBridge
// ============================================================================

/// Bridge configuration.
#[derive(Debug, Clone)]
pub struct BridgeConfig {
    /// Transport name for checkpoint storage (e.g., "pg_to_nats")
    pub transport_name: String,

    /// Batch size for fetching entries
    pub batch_size: usize,

    /// Poll interval when no NOTIFY received (seconds)
    pub poll_interval_secs: u64,

    /// PostgreSQL NOTIFY channel name
    pub notify_channel: String,
}

impl Default for BridgeConfig {
    fn default() -> Self {
        Self {
            transport_name:     "pg_to_nats".to_string(),
            batch_size:         100,
            poll_interval_secs: 1,
            notify_channel:     "fraiseql_events".to_string(),
        }
    }
}

/// PostgreSQL to NATS bridge.
///
/// Reliably publishes entity change events from PostgreSQL's
/// `tb_entity_change_log` to NATS JetStream using cursor-based polling.
///
/// # Design Properties
///
/// 1. **LISTEN/NOTIFY is NEVER relied upon for correctness**:
///    - NOTIFY only triggers faster polling
///    - If NOTIFY is missed, poll interval catches events
///    - Bridge works correctly even if all NOTIFY messages are lost
///
/// 2. **Cursor-based durability with monotonic progression**:
///    - `cursor` = last processed `pk_entity_change_log`
///    - Stored in separate `tb_transport_checkpoint` table
///    - On crash/restart, resumes from last checkpoint
///
/// 3. **Publish-then-mark pattern with conditional update**:
///    - Publish to NATS JetStream
///    - Wait for ACK
///    - Mark as published (conditional: only if not already published)
///    - Safe against accidental multi-bridge races
#[cfg(feature = "nats")]
pub struct PostgresNatsBridge {
    pool:             PgPool,
    nats_transport:   Arc<NatsTransport>,
    checkpoint_store: Arc<dyn CheckpointStore>,
    config:           BridgeConfig,
}

#[cfg(feature = "nats")]
impl PostgresNatsBridge {
    /// Create a new bridge instance.
    pub fn new(
        pool: PgPool,
        nats_transport: Arc<NatsTransport>,
        checkpoint_store: Arc<dyn CheckpointStore>,
        config: BridgeConfig,
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
        pool: PgPool,
        nats_transport: Arc<NatsTransport>,
        checkpoint_store: Arc<dyn CheckpointStore>,
    ) -> Self {
        Self::new(pool, nats_transport, checkpoint_store, BridgeConfig::default())
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

    /// Fetch batch from cursor (NOT filtered by `nats_published_at`).
    ///
    /// Fetches by cursor only to ensure monotonic progression even if:
    /// - Some rows were already published
    /// - Bridge restarted mid-run
    /// - Multiple publishers existed in the past
    async fn fetch_batch_from_cursor(&self, cursor: i64) -> Result<Vec<ChangeLogEntry>> {
        #[allow(clippy::cast_possible_wrap)]
        let entries: Vec<ChangeLogEntry> = sqlx::query_as(
            r"
            SELECT pk_entity_change_log, id, fk_customer_org, fk_contact,
                   object_type, object_id, modification_type, change_status,
                   object_data, extra_metadata, created_at,
                   nats_published_at, nats_event_id
            FROM core.tb_entity_change_log
            WHERE pk_entity_change_log > $1
            ORDER BY pk_entity_change_log ASC
            LIMIT $2
            ",
        )
        .bind(cursor)
        .bind(self.config.batch_size as i64)
        .fetch_all(&self.pool)
        .await?;

        Ok(entries)
    }

    /// Mark event as published (idempotent, safe against races).
    ///
    /// Uses conditional update to prevent races if multiple bridges run.
    /// Only updates if not already published.
    ///
    /// Returns `true` if this bridge was first to publish, `false` if already published.
    async fn mark_published(&self, pk_change_log: i64, event_id: Uuid) -> Result<bool> {
        let result = sqlx::query(
            r"
            UPDATE core.tb_entity_change_log
            SET nats_published_at = NOW(),
                nats_event_id = $2
            WHERE pk_entity_change_log = $1
              AND nats_published_at IS NULL
            ",
        )
        .bind(pk_change_log)
        .bind(event_id)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() == 1)
    }

    /// Main bridge loop.
    ///
    /// # Algorithm
    ///
    /// 1. Fetch batch from cursor (not filtered by published status)
    /// 2. For each entry:
    ///    - If already published → skip, advance cursor
    ///    - If unpublished → publish, mark published (conditional), advance cursor
    /// 3. Save cursor checkpoint
    /// 4. Wait for NOTIFY or timeout
    /// 5. Repeat
    pub async fn run(&self) -> Result<()> {
        info!("Starting PostgreSQL → NATS bridge: {}", self.config.transport_name);

        // Load last cursor on startup (crash recovery)
        let mut cursor = self.load_cursor().await?;
        info!("Bridge starting from cursor: {cursor}");

        // Start LISTEN connection (wake-up signal only)
        let mut notify_listener = PgListener::connect_with(&self.pool).await.map_err(|e| {
            ObserverError::ListenerConnectionFailed {
                reason: format!("Failed to create PgListener: {e}"),
            }
        })?;

        notify_listener.listen(&self.config.notify_channel).await.map_err(|e| {
            ObserverError::ListenerConnectionFailed {
                reason: format!("Failed to LISTEN on {}: {e}", self.config.notify_channel),
            }
        })?;

        loop {
            // Fetch batch from cursor
            let entries = self.fetch_batch_from_cursor(cursor).await?;

            if entries.is_empty() {
                // No more entries, wait for NOTIFY or timeout
                let poll_duration = Duration::from_secs(self.config.poll_interval_secs);

                tokio::select! {
                    notification = notify_listener.recv() => {
                        match notification {
                            Ok(_) => {
                                debug!("Received NOTIFY wake-up signal");
                            }
                            Err(e) => {
                                warn!("NOTIFY listener error: {}. Continuing with poll.", e);
                            }
                        }
                    }
                    () = tokio::time::sleep(poll_duration) => {
                        debug!("Poll interval timeout, re-checking for new entries");
                    }
                }
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
                            // Mark as published (conditional, safe against races)
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

                            // Advance cursor (regardless of who published)
                            cursor = entry.pk_entity_change_log;
                        },
                        Err(e) => {
                            error!(
                                "Failed to publish event {} to NATS: {}. Will retry on next iteration.",
                                event.id, e
                            );
                            // Do NOT advance cursor - will retry this event next iteration
                            break;
                        },
                    }
                }

                // Save cursor checkpoint after batch
                self.save_cursor(cursor).await?;
            }
        }
    }

    /// Run the bridge with graceful shutdown support.
    ///
    /// Stops when the shutdown signal is received.
    pub async fn run_with_shutdown(
        &self,
        mut shutdown: tokio::sync::broadcast::Receiver<()>,
    ) -> Result<()> {
        info!(
            "Starting PostgreSQL → NATS bridge with shutdown support: {}",
            self.config.transport_name
        );

        let mut cursor = self.load_cursor().await?;
        info!("Bridge starting from cursor: {cursor}");

        let mut notify_listener = PgListener::connect_with(&self.pool).await.map_err(|e| {
            ObserverError::ListenerConnectionFailed {
                reason: format!("Failed to create PgListener: {e}"),
            }
        })?;

        notify_listener.listen(&self.config.notify_channel).await.map_err(|e| {
            ObserverError::ListenerConnectionFailed {
                reason: format!("Failed to LISTEN on {}: {e}", self.config.notify_channel),
            }
        })?;

        loop {
            // Check for shutdown signal
            if shutdown.try_recv().is_ok() {
                info!("Shutdown signal received, stopping bridge");
                self.save_cursor(cursor).await?;
                return Ok(());
            }

            let entries = self.fetch_batch_from_cursor(cursor).await?;

            if entries.is_empty() {
                let poll_duration = Duration::from_secs(self.config.poll_interval_secs);

                tokio::select! {
                    _ = shutdown.recv() => {
                        info!("Shutdown signal received during wait, stopping bridge");
                        self.save_cursor(cursor).await?;
                        return Ok(());
                    }
                    _ = notify_listener.recv() => {
                        debug!("Received NOTIFY wake-up signal");
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

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bridge_config_default() {
        let config = BridgeConfig::default();
        assert_eq!(config.transport_name, "pg_to_nats");
        assert_eq!(config.batch_size, 100);
        assert_eq!(config.poll_interval_secs, 1);
        assert_eq!(config.notify_channel, "fraiseql_events");
    }

    #[test]
    fn test_change_log_entry_to_event_insert() {
        use crate::event::EventKind;

        let entry = ChangeLogEntry {
            pk_entity_change_log: 1,
            id:                   Uuid::new_v4(),
            fk_customer_org:      Some(123),
            fk_contact:           Some(456),
            object_type:          "Order".to_string(),
            object_id:            Uuid::new_v4(),
            modification_type:    "INSERT".to_string(),
            change_status:        None,
            object_data:          Some(serde_json::json!({"total": 100})),
            extra_metadata:       None,
            created_at:           Utc::now(),
            nats_published_at:    None,
            nats_event_id:        None,
        };

        let event = entry.to_entity_event().unwrap();
        assert_eq!(event.entity_type, "Order");
        assert_eq!(event.event_type, EventKind::Created);
        assert_eq!(event.user_id, Some("456".to_string()));
    }

    #[test]
    fn test_change_log_entry_to_event_update() {
        use crate::event::EventKind;

        let entry = ChangeLogEntry {
            pk_entity_change_log: 2,
            id:                   Uuid::new_v4(),
            fk_customer_org:      None,
            fk_contact:           None,
            object_type:          "User".to_string(),
            object_id:            Uuid::new_v4(),
            modification_type:    "UPDATE".to_string(),
            change_status:        None,
            object_data:          None,
            extra_metadata:       None,
            created_at:           Utc::now(),
            nats_published_at:    None,
            nats_event_id:        None,
        };

        let event = entry.to_entity_event().unwrap();
        assert_eq!(event.event_type, EventKind::Updated);
    }

    #[test]
    fn test_change_log_entry_to_event_delete() {
        use crate::event::EventKind;

        let entry = ChangeLogEntry {
            pk_entity_change_log: 3,
            id:                   Uuid::new_v4(),
            fk_customer_org:      None,
            fk_contact:           None,
            object_type:          "Product".to_string(),
            object_id:            Uuid::new_v4(),
            modification_type:    "DELETE".to_string(),
            change_status:        None,
            object_data:          None,
            extra_metadata:       None,
            created_at:           Utc::now(),
            nats_published_at:    None,
            nats_event_id:        None,
        };

        let event = entry.to_entity_event().unwrap();
        assert_eq!(event.event_type, EventKind::Deleted);
    }

    #[test]
    fn test_change_log_entry_invalid_modification_type() {
        let entry = ChangeLogEntry {
            pk_entity_change_log: 4,
            id:                   Uuid::new_v4(),
            fk_customer_org:      None,
            fk_contact:           None,
            object_type:          "Test".to_string(),
            object_id:            Uuid::new_v4(),
            modification_type:    "INVALID".to_string(),
            change_status:        None,
            object_data:          None,
            extra_metadata:       None,
            created_at:           Utc::now(),
            nats_published_at:    None,
            nats_event_id:        None,
        };

        let result = entry.to_entity_event();
        assert!(result.is_err());
    }

    #[test]
    fn test_postgres_checkpoint_store_clone() {
        fn assert_clone<T: Clone>() {}
        assert_clone::<PostgresCheckpointStore>();
    }
}
