//! Change log listener that polls `tb_entity_change_log` for entity mutations.
//!
//! This module implements a durable event listener that:
//! 1. Polls `tb_entity_change_log` for new entries
//! 2. Parses Debezium envelope format (before/after/op/source)
//! 3. Converts entries to `EntityEvent` for observer processing
//! 4. Maintains checkpoint for recovery after restarts
//! 5. Handles backpressure and batch processing
//!
//! **Requires the `postgres` Cargo feature.**

#[cfg(not(feature = "postgres"))]
compile_error!(
    "`fraiseql-observers::listener::change_log` requires the `postgres` feature. \
     Enable it with: fraiseql-observers = { features = [\"postgres\"] }"
);

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
///
/// Decoded with the framework-owned contract's **Trinity types** —
/// `fk_customer_org`/`fk_contact` are `BIGINT` (internal join FKs) and
/// `object_id` is the public-facing `UUID` — reconciling the pre-existing
/// `String`/`String` mismatch so the poller decodes executor-written rows. The
/// type-typed values are projected back into [`ChangeLogEntry`]'s string fields
/// so downstream consumers are unchanged. `object_data` is nullable on the
/// contract (an effective change may carry no entity payload), so it is decoded
/// as `Option`.
///
/// The trailing three columns are the Change-Spine envelope/perf projection
/// surfaced top-level: `tenant_id` (public-facing UUID partition stamp, decoded
/// as `Uuid` — **distinct from `fk_customer_org`**, the internal BIGINT join FK),
/// `duration_ms` (`int4`), and `seq` (`int8`, monotonic ordering / dedup). All
/// three are contract-nullable.
type ChangeLogRow = (
    i64,                   // pk_entity_change_log
    Uuid,                  // id
    Option<i64>,           // fk_customer_org (BIGINT join FK)
    Option<i64>,           // fk_contact (BIGINT)
    String,                // object_type
    Uuid,                  // object_id (public-facing UUID)
    String,                // modification_type
    Option<String>,        // change_status
    Option<Value>,         // object_data (nullable)
    Option<Value>,         // extra_metadata
    Option<DateTime<Utc>>, // created_at
    Option<Uuid>,          // tenant_id (public-facing UUID partition stamp)
    Option<i32>,           // duration_ms (perf column, int4)
    Option<i64>,           // seq (Change-Spine ordering / dedup, int8)
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

    /// Public-facing tenant UUID — the RLS/JWT partition stamp. Distinct from
    /// [`fk_customer_org`](Self::fk_customer_org) (the internal BIGINT join FK)
    /// per the Trinity contract; `None` when the contract column is `NULL`.
    pub tenant_id: Option<String>,

    /// Wall-clock duration of the originating mutation in milliseconds, when the
    /// producer stamped it; `None` for cooperative producers without timing.
    pub duration_ms: Option<i32>,

    /// Monotonic Change-Spine sequence for durable ordering and dedup on
    /// `(object_type, seq)`; `None` when the source row carried no sequence.
    pub seq: Option<i64>,
}

impl ChangeLogEntry {
    /// Parse Debezium envelope to get operation code
    ///
    /// # Errors
    ///
    /// Returns [`ObserverError::TemplateRenderingFailed`] if the `op` field is
    /// missing or empty in the Debezium envelope.
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
    ///
    /// # Errors
    ///
    /// Returns [`ObserverError::TemplateRenderingFailed`] if the `after` field is
    /// absent in the Debezium envelope.
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
    ///
    /// # Errors
    ///
    /// Returns [`ObserverError::TemplateRenderingFailed`] if the Debezium operation
    /// code is unknown, the `object_id` is not a valid UUID, or the timestamp
    /// cannot be parsed.
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
            self.before_values().unwrap_or(Value::Object(serde_json::Map::default()))
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

        // Trinity: the multi-tenant partition stamp is the public-facing UUID
        // `tenant_id` — NOT `fk_customer_org` (the internal BIGINT join FK).
        // Surfacing the wrong one would key tenant isolation off an integer that
        // never matches the JWT/RLS tenant.
        let tenant_id = self.tenant_id.clone();

        Ok(EntityEvent {
            id: Uuid::parse_str(&self.pk_entity_change_log).unwrap_or_else(|_| Uuid::new_v4()),
            event_type: event_kind,
            entity_type: self.object_type.clone(),
            entity_id,
            data,
            changes,
            user_id,
            tenant_id,
            timestamp,
            duration_ms: self.duration_ms,
            seq: self.seq,
        })
    }

    /// Build field changes for UPDATE events by comparing before/after
    fn build_field_changes(&self) -> Result<Option<HashMap<String, FieldChanges>>> {
        if self.debezium_operation()? != 'u' {
            return Ok(None);
        }

        let Some(Value::Object(before)) = self.before_values() else {
            return Ok(None);
        };

        let Value::Object(after) = self.after_values()? else {
            return Ok(None);
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
    ///
    /// # Errors
    ///
    /// Returns [`ObserverError::DatabaseError`] if the database query fails.
    pub async fn next_batch(&mut self) -> Result<Vec<ChangeLogEntry>> {
        // Query: SELECT * FROM tb_entity_change_log
        // WHERE pk_entity_change_log > last_processed_id
        // ORDER BY pk_entity_change_log ASC
        // LIMIT batch_size

        #[allow(clippy::cast_possible_wrap)]
        // Reason: batch_size is bounded by config and won't exceed i64::MAX
        let batch_size_i64 = self.config.batch_size as i64;
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
                    created_at,
                    tenant_id,
                    duration_ms,
                    seq
                FROM core.tb_entity_change_log
                WHERE pk_entity_change_log > $1
                ORDER BY pk_entity_change_log ASC
                LIMIT $2
                ",
        )
        .bind(self.last_processed_id)
        .bind(batch_size_i64)
        .fetch_all(&self.config.pool)
        .await
        .map_err(|e| ObserverError::DatabaseError {
            reason: format!("Failed to query change log: {e}"),
        })?;

        let mut entries = Vec::new();

        for (
            pk,
            id,
            org,
            contact,
            obj_type,
            obj_id,
            mod_type,
            status,
            data,
            meta,
            created,
            tenant,
            duration_ms,
            seq,
        ) in rows
        {
            let created_at_str =
                created.map_or_else(|| Utc::now().to_rfc3339(), |dt| dt.to_rfc3339());

            entries.push(ChangeLogEntry {
                id: pk,
                pk_entity_change_log: id.to_string(),
                // BIGINT/UUID contract values projected into the string-typed
                // public fields (reconcile without breaking downstream readers).
                fk_customer_org: org.map(|n| n.to_string()).unwrap_or_default(),
                fk_contact: contact.map(|n| n.to_string()),
                object_type: obj_type,
                object_id: obj_id.to_string(),
                modification_type: mod_type,
                change_status: status.unwrap_or_default(),
                object_data: data.unwrap_or(Value::Null),
                extra_metadata: meta,
                created_at: created_at_str,
                // Trinity: tenant_id is the public-facing UUID partition stamp,
                // kept distinct from fk_customer_org above.
                tenant_id: tenant.map(|t| t.to_string()),
                duration_ms,
                seq,
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
    pub const fn set_checkpoint(&mut self, id: i64) {
        self.last_processed_id = id;
    }

    /// Poll indefinitely for events (for background task)
    ///
    /// # Errors
    ///
    /// Propagates errors from [`ChangeLogListener::next_batch`] if a database
    /// query fails unrecoverably.
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
