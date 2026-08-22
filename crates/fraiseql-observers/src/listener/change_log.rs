//! Change log listener that polls `tb_entity_change_log` for entity mutations.
//!
//! This module implements a polling event listener that:
//! 1. Polls `tb_entity_change_log` for undispatched entries
//! 2. Parses Debezium envelope format (before/after/op/source)
//! 3. Converts entries to `EntityEvent` for observer processing
//! 4. Records each dispatched batch in a durable per-listener ledger
//! 5. Handles backpressure and batch processing
//!
//! # What "already handled" means (#935)
//!
//! Not "its pk is below a watermark". `pk_entity_change_log` is allocated at
//! INSERT time but only becomes visible at COMMIT, so under concurrent writes pk
//! order and commit order diverge and a strict `pk > cursor` cursor silently and
//! permanently skips rows that commit late. Instead the poll is an **anti-join**
//! against `core.tb_observer_dispatch` (migration 14): a row is undispatched
//! until this listener has recorded dispatching it. Each poll considers rows
//! above the scan bound plus anything inside
//! [`ChangeLogListenerConfig::commit_lag_window`], and a periodic full sweep
//! recovers rows whose transaction outlived even that, at `WARN`.
//!
//! The driver's two jobs, both still required:
//!
//! - Call [`ChangeLogListener::record_dispatched`] once a batch's actions have run. Recording
//!   *after* dispatch is what keeps delivery at-least-once; recording at all is what keeps the
//!   commit-lag rescan from re-delivering across a restart.
//! - Restore the persisted cursor at startup (via [`crate::checkpoint::CheckpointStore::load`] +
//!   [`ChangeLogListenerConfig::with_resume_from`]) and persist it after each dispatched batch
//!   (#805). The cursor is now a scan bound and the floor a sweep drops back to, so without it a
//!   fresh process still walks the whole change log.
//!
//! **Requires the `postgres` Cargo feature.**

#[cfg(not(feature = "postgres"))]
compile_error!(
    "`fraiseql-observers::listener::change_log` requires the `postgres` feature. \
     Enable it with: fraiseql-observers = { features = [\"postgres\"] }"
);

use std::{
    collections::{HashMap, VecDeque},
    time::{Duration, Instant},
};

use chrono::{DateTime, TimeZone, Utc};
use serde_json::Value;
use sqlx::postgres::PgPool;
use tracing::{debug, error, info, warn};
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
/// The trailing columns are the Change-Spine envelope/perf projection surfaced
/// top-level: `tenant_id` (public-facing UUID partition stamp, decoded as `Uuid`
/// — **distinct from `fk_customer_org`**, the internal BIGINT join FK),
/// `duration_ms` (`int4`), `seq` (`int8`, monotonic ordering / dedup), the
/// #390 actor columns `actor_type` (`TEXT`, the request's actor classification)
/// and `acting_for` (`UUID`, the delegated human a delegated agent acts for —
/// decoded as `Uuid` like `tenant_id`, since the contract types it UUID), and
/// `schema_version` (`TEXT`, #377, the producer's application schema version).
/// All are contract-nullable.
///
/// Decoded **by column name** via `sqlx::FromRow`, so `SELECT` column order is
/// immaterial and the projection can grow past sqlx's 16-element tuple `FromRow`
/// ceiling (it reached exactly 16 at #390).
#[derive(sqlx::FromRow)]
struct ChangeLogRow {
    pk_entity_change_log: i64,
    id:                   Uuid,
    fk_customer_org:      Option<i64>, // BIGINT join FK
    fk_contact:           Option<i64>, // BIGINT
    object_type:          String,
    object_id:            Uuid, // public-facing UUID
    modification_type:    String,
    change_status:        Option<String>,
    object_data:          Option<Value>, // nullable on the contract (the after-image)
    object_data_before:   Option<Value>, /* the pre-image (changelog_pre_image); NULL unless
                                          * opted in */
    extra_metadata:       Option<Value>,
    created_at:           Option<DateTime<Utc>>,
    tenant_id:            Option<Uuid>, // public-facing UUID partition stamp
    duration_ms:          Option<i32>,  // perf column, int4
    seq:                  Option<i64>,  // Change-Spine ordering / dedup, int8
    actor_type:           Option<String>, // #390 actor classification, TEXT
    acting_for:           Option<Uuid>, // #390 delegated-human public UUID
    schema_version:       Option<String>, // #377 producer schema version, TEXT
}

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

    /// Stable listener identity — the key of this listener's dispatch ledger
    /// (`core.tb_observer_dispatch`) and, for the drivers that persist one, of
    /// its `observer_checkpoints` row. Two listeners sharing an id share a
    /// ledger and therefore split the change log between them; two listeners
    /// with different ids each dispatch every row.
    pub listener_id: String,

    /// How long after `created_at` a writing transaction may commit and still be
    /// caught by the cheap per-poll scan (#935). Rows whose transaction outlives
    /// this are only recovered by the periodic full sweep, loudly.
    ///
    /// Must exceed the longest transaction that writes to the change log.
    pub commit_lag_window: Duration,

    /// Run the full recovery sweep every N polls (the first poll always sweeps).
    /// The sweep is the safety net for a transaction that outlived
    /// `commit_lag_window`; each row it finds is reported at `WARN`.
    pub sweep_every: u64,
}

/// Default bound on how long after `created_at` a writing transaction may commit
/// and still be caught by the cheap per-poll scan. Matches the cdc drain's
/// window (#797), which solves the identical ordering problem.
const DEFAULT_COMMIT_LAG_WINDOW: Duration = Duration::from_mins(15);

/// Default poll period of the full recovery sweep (the first poll always sweeps).
const DEFAULT_SWEEP_EVERY: u64 = 256;

/// The listener identity used when a driver does not set one.
const DEFAULT_LISTENER_ID: &str = "default";

impl ChangeLogListenerConfig {
    /// Create config with defaults
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self {
            pool,
            poll_interval_ms: 100,
            batch_size: 100,
            resume_from_id: None,
            listener_id: DEFAULT_LISTENER_ID.to_string(),
            commit_lag_window: DEFAULT_COMMIT_LAG_WINDOW,
            sweep_every: DEFAULT_SWEEP_EVERY,
        }
    }

    /// Set the stable listener identity keying the dispatch ledger (#935).
    ///
    /// Drivers that persist a checkpoint should pass the **same** id they use
    /// for [`crate::checkpoint::CheckpointStore`], so cursor and ledger describe
    /// one listener.
    #[must_use]
    pub fn with_listener_id(mut self, id: impl Into<String>) -> Self {
        self.listener_id = id.into();
        self
    }

    /// Set the commit-lag window the per-poll scan reaches back over (#935).
    #[must_use]
    pub const fn with_commit_lag_window(mut self, window: Duration) -> Self {
        self.commit_lag_window = window;
        self
    }

    /// Run the full recovery sweep every `polls` polls (the first always sweeps).
    #[must_use]
    pub const fn with_sweep_every(mut self, polls: u64) -> Self {
        self.sweep_every = polls;
        self
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

    /// The changed entity's **after-image** (the `object_data` contract column),
    /// uniform across every producer (executor outbox AND the #366 capture
    /// trigger). `Value::Null` for a DELETE (no after-state) or when the producer
    /// wrote no payload. The Debezium operation is [`modification_type`](Self::modification_type),
    /// not a key inside this value — the base table never stores a
    /// `{op, before, after}` envelope.
    pub object_data: Value,

    /// The changed entity's **pre-image** (the `object_data_before` contract
    /// column), recorded only by producers that opt into `changelog_pre_image`;
    /// `None` otherwise. Surfaced by [`before_values`](Self::before_values).
    pub object_data_before: Option<Value>,

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

    /// The request's actor classification (#390 envelope column `actor_type`):
    /// `"human_user"`, `"service_account"`, `"ai_agent"`, or `"system_job"`.
    /// Recorded for forensics / downstream fan-out, never an authorization input;
    /// `None` when the producer did not stamp it.
    pub actor_type: Option<String>,

    /// For a delegated-agent request (RFC 8693 `act` claim), the delegated
    /// human's public-facing UUID (#390 envelope column `acting_for`), projected
    /// as a string like [`tenant_id`](Self::tenant_id); `None` for non-delegated
    /// requests.
    pub acting_for: Option<String>,

    /// The producer's application schema version (#377 envelope column
    /// `schema_version`) — the schema the originating mutation ran against, for
    /// cross-version forensics / deploy audit; `None` when the producer did not
    /// stamp it.
    pub schema_version: Option<String>,
}

impl ChangeLogEntry {
    /// The Debezium operation code (`'c'`/`'u'`/`'d'`/`'r'`) derived from
    /// [`modification_type`](Self::modification_type).
    ///
    /// The change-log contract stores the verb in `modification_type` and the
    /// after-image in `object_data` (NOT a `{op, before, after}` envelope), so the
    /// op is mapped from the verb: `INSERT`→`'c'`, `UPDATE`→`'u'`, `DELETE`→`'d'`,
    /// and the explicit no-op verbs `CUSTOM`/`NOOP`/`READ` → `'r'`.
    ///
    /// # Errors
    ///
    /// Returns [`ObserverError::TemplateRenderingFailed`] for any other verb: an
    /// unrecognised `modification_type` is producer contract drift and must be
    /// rejected loudly, not silently defaulted to a no-op (#773).
    pub fn debezium_operation(&self) -> Result<char> {
        match self.modification_type.to_uppercase().as_str() {
            "INSERT" => Ok('c'),
            "UPDATE" => Ok('u'),
            "DELETE" => Ok('d'),
            "CUSTOM" | "NOOP" | "READ" => Ok('r'),
            other => Err(ObserverError::TemplateRenderingFailed {
                reason: format!("Unknown modification_type: {other}"),
            }),
        }
    }

    /// The entity's **after** state — the `object_data` column directly (the
    /// uniform after-image), not a key inside an envelope.
    ///
    /// # Errors
    ///
    /// Infallible today; kept returning [`Result`] for call-site stability.
    pub fn after_values(&self) -> Result<Value> {
        Ok(self.object_data.clone())
    }

    /// The entity's **before** state — the `object_data_before` column. `None`
    /// unless the producing mutation/table opted into `changelog_pre_image`.
    #[must_use]
    pub fn before_values(&self) -> Option<Value> {
        self.object_data_before.clone()
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

        // #366: the capture-origin discriminator. A row written by the shipped
        // external-write capture trigger carries `extra_metadata.cdc_source =
        // "fallback_trigger"`; FraiseQL executor-written rows do not. `after:capture`
        // dispatch keys on this so only genuinely-external writes drive functions.
        let cdc_source = self
            .extra_metadata
            .as_ref()
            .and_then(|meta| meta.get("cdc_source"))
            .and_then(Value::as_str)
            .map(String::from);

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
            // #390 actor envelope: recorded-only classification + the delegated
            // human a delegated agent acts for, surfaced for out-of-session
            // consumers (NATS bridge / CDC fan-out).
            actor_type: self.actor_type.clone(),
            acting_for: self.acting_for.clone(),
            // #377 producer schema version, surfaced for deploy / cross-version audit.
            schema_version: self.schema_version.clone(),
            cdc_source,
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

        // A recorded pre-image with an empty diff is `Some(empty)` — "tracked,
        // nothing changed" — NOT `None`, which means "change tracking
        // unavailable" and makes `field_changed*` conditions error rather than
        // evaluate (#845). Collapsing the two was how a no-op UPDATE became
        // indistinguishable from a missing pre-image.
        Ok(Some(changes))
    }
}

/// Change log listener that polls database for mutations.
///
/// # Why there is no pk watermark (#935)
///
/// `pk_entity_change_log` is allocated at INSERT time but only becomes visible
/// at COMMIT, so pk order and commit order diverge under concurrent writes. A
/// strict `pk > last_processed_id` cursor therefore skipped — permanently and
/// silently — any row whose transaction committed after a higher-pk row had
/// already been polled. The cursor is now an *anti-join* against durable
/// per-listener dispatched state (`core.tb_observer_dispatch`, migration 14),
/// the same shape #797 shipped for the cdc drain: what has been handled is a
/// recorded fact, not an inference from ordering.
///
/// `last_processed_id` survives as a cheap scan bound, not as a correctness
/// boundary: each poll considers rows above it **plus** anything still inside
/// [`ChangeLogListenerConfig::commit_lag_window`], and a periodic full sweep
/// recovers rows whose transaction outlived even that, loudly.
///
/// # Drivers must record dispatch
///
/// [`next_batch`](Self::next_batch) hands out rows; the driver calls
/// [`record_dispatched`](Self::record_dispatched) once their actions have run.
/// Recording afterwards is what keeps delivery **at-least-once** — a crash in
/// between replays that batch. A driver that never records still cannot lose a
/// row (in-process state suppresses re-delivery for the window); it will
/// re-deliver duplicates after a restart or a sweep.
pub struct ChangeLogListener {
    config:            ChangeLogListenerConfig,
    last_processed_id: i64,
    /// The checkpoint this listener resumed from: rows at or below it were
    /// dispatched by a previous incarnation. Fixed for the listener's life — it
    /// is the floor a full sweep drops back to, never further.
    resume_floor:      i64,
    /// Rows handed out by `next_batch` but not yet recorded dispatched, with the
    /// instant they were handed out. Suppresses in-process re-delivery across
    /// the window the scan reaches back over; pruned at the window bound so it
    /// stays bounded by (window x throughput).
    in_flight:         VecDeque<(Instant, i64)>,
    /// Polls completed, driving the periodic full sweep (poll 0 always sweeps).
    polls:             u64,
    /// Whether the dispatch ledger DDL has been applied in this process.
    ledger_ready:      bool,
}

impl ChangeLogListener {
    /// Create a new change log listener
    #[must_use]
    pub fn new(config: ChangeLogListenerConfig) -> Self {
        let last_processed_id = config.resume_from_id.unwrap_or(0);

        Self {
            config,
            last_processed_id,
            resume_floor: last_processed_id,
            in_flight: VecDeque::new(),
            polls: 0,
            ledger_ready: false,
        }
    }

    /// Apply the dispatch-ledger DDL (`core.tb_observer_dispatch`, idempotent).
    ///
    /// Called automatically on the first poll; exposed so a driver can surface a
    /// DDL permission failure at startup rather than on the first event.
    ///
    /// # Errors
    ///
    /// Returns [`ObserverError::DatabaseError`] if the DDL fails.
    pub async fn init_dispatch_ledger(&mut self) -> Result<()> {
        let ddl = sqlx::raw_sql(crate::migrations::observer_dispatch_sql())
            .execute(&self.config.pool)
            .await;

        if let Err(ddl_error) = ddl {
            // A least-privilege deployment runs the poller as a role with no
            // CREATE on `core` — the same shape migration 12's RLS notes assume.
            // PostgreSQL refuses `CREATE TABLE IF NOT EXISTS` on privilege
            // grounds *before* the existence check, so a DDL error here does not
            // mean the ledger is missing. Ask the catalog before failing: an
            // already-migrated database must keep polling.
            let present: bool =
                sqlx::query_scalar("SELECT to_regclass('core.tb_observer_dispatch') IS NOT NULL")
                    .fetch_one(&self.config.pool)
                    .await
                    .unwrap_or(false);

            if !present {
                return Err(ObserverError::DatabaseError {
                    reason: format!(
                        "the observer dispatch ledger core.tb_observer_dispatch is missing and \
                         could not be created (apply fraiseql-observers migration \
                         14_create_observer_dispatch.sql, or grant CREATE on schema core): \
                         {ddl_error}"
                    ),
                });
            }
            debug!(
                "Dispatch ledger DDL was refused but the table exists; \
                 continuing (least-privilege role)"
            );
        }

        self.ledger_ready = true;
        Ok(())
    }

    /// Drop in-flight entries older than the commit-lag window: past it the
    /// windowed scan no longer reaches them, so the durable ledger (or a sweep)
    /// is the only thing that can surface them again.
    fn prune_in_flight(&mut self) {
        let horizon = self.config.commit_lag_window;
        while let Some(&(handed_out, _)) = self.in_flight.front() {
            if handed_out.elapsed() > horizon {
                self.in_flight.pop_front();
            } else {
                break;
            }
        }
    }

    /// Record that a batch's actions have been dispatched, so the rows are never
    /// handed out again — the durable half of the #935 anti-join.
    ///
    /// Call this **after** the batch's observers have run: the ordering is what
    /// makes delivery at-least-once rather than at-most-once. Idempotent
    /// (`ON CONFLICT DO NOTHING`), so re-recording after a partial failure is
    /// safe.
    ///
    /// # Errors
    ///
    /// Returns [`ObserverError::DatabaseError`] if the write fails.
    pub async fn record_dispatched(&mut self, entries: &[ChangeLogEntry]) -> Result<()> {
        if entries.is_empty() {
            return Ok(());
        }
        if !self.ledger_ready {
            self.init_dispatch_ledger().await?;
        }

        let pks: Vec<i64> = entries.iter().map(|e| e.id).collect();

        // The ledger key and `created_at` are read from the authoritative
        // change-log row rather than round-tripped through the entry's string
        // fields. The pk is only the lookup — what is stored is the row's stable
        // UUID, so a rebuilt change log cannot alias a previous incarnation.
        sqlx::query(
            r"
                INSERT INTO core.tb_observer_dispatch
                    (listener_id, change_log_id, created_at)
                SELECT $1, e.id, e.created_at
                FROM core.tb_entity_change_log e
                WHERE e.pk_entity_change_log = ANY($2::bigint[])
                ON CONFLICT (listener_id, change_log_id) DO NOTHING
                ",
        )
        .bind(&self.config.listener_id)
        .bind(&pks)
        .execute(&self.config.pool)
        .await
        .map_err(|e| ObserverError::DatabaseError {
            reason: format!("Failed to record dispatched change-log rows: {e}"),
        })?;

        // Now durable — the in-process suppression set no longer needs them.
        self.in_flight.retain(|(_, pk)| !pks.contains(pk));

        Ok(())
    }

    /// Fetch next batch of entries from change log
    ///
    /// # Errors
    ///
    /// Returns [`ObserverError::DatabaseError`] if the database query fails.
    pub async fn next_batch(&mut self) -> Result<Vec<ChangeLogEntry>> {
        if !self.ledger_ready {
            self.init_dispatch_ledger().await?;
        }
        self.prune_in_flight();

        // The first poll always sweeps (initial reconciliation), then every
        // `sweep_every` polls.
        //
        // A poll's pk bound is the only thing a sweep changes: normally it is the
        // advancing scan bound, and a sweep drops back to the **resume floor** —
        // the checkpoint a previous incarnation left, below which that incarnation
        // already dispatched everything. So a sweep re-examines every undispatched
        // row this process could still owe, including one whose transaction
        // outlived `commit_lag_window` and so fell out of the windowed clause.
        //
        // It deliberately does NOT drop below the floor. Dropping to 0 would make
        // the first poll after an upgrade (checkpoint present, ledger empty)
        // re-dispatch the entire history. Rows below the floor are covered by the
        // window clause instead, so the worst case is a bounded, one-time replay
        // of the last `commit_lag_window` — at-least-once, as documented.
        let sweeping =
            self.config.sweep_every == 0 || self.polls.is_multiple_of(self.config.sweep_every);
        self.polls = self.polls.wrapping_add(1);
        let pk_bound = if sweeping {
            self.resume_floor
        } else {
            self.last_processed_id
        };

        #[allow(clippy::cast_possible_wrap)]
        // Reason: batch_size is bounded by config and won't exceed i64::MAX
        let batch_size_i64 = self.config.batch_size as i64;
        let in_flight_pks: Vec<i64> = self.in_flight.iter().map(|&(_, pk)| pk).collect();
        #[allow(clippy::cast_precision_loss)]
        // Reason: the window is an operator-set duration; sub-microsecond precision is irrelevant
        let window_secs = self.config.commit_lag_window.as_secs_f64();
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
                    object_data_before,
                    extra_metadata,
                    created_at,
                    tenant_id,
                    duration_ms,
                    seq,
                    actor_type,
                    acting_for,
                    schema_version
                FROM core.tb_entity_change_log e
                WHERE NOT EXISTS (
                        SELECT 1
                        FROM core.tb_observer_dispatch d
                        WHERE d.listener_id = $3
                          AND d.change_log_id = e.id
                      )
                  AND NOT (e.pk_entity_change_log = ANY($5::bigint[]))
                  AND (
                        e.pk_entity_change_log > $1
                        OR e.created_at > now() - make_interval(secs => $4)
                      )
                ORDER BY e.pk_entity_change_log ASC
                LIMIT $2
                ",
        )
        .bind(pk_bound)
        .bind(batch_size_i64)
        .bind(&self.config.listener_id)
        .bind(window_secs)
        .bind(&in_flight_pks)
        .fetch_all(&self.config.pool)
        .await
        .map_err(|e| ObserverError::DatabaseError {
            reason: format!("Failed to query change log: {e}"),
        })?;

        let mut entries = Vec::new();
        // Rows only a sweep could have reached: their transaction outlived the
        // commit-lag window. Delivery is still correct, but the window is now
        // known to be too small for this workload, so say so.
        let mut late_recovered = 0_u32;
        let scan_floor = self.last_processed_id;
        let window_horizon = Utc::now()
            - chrono::Duration::from_std(self.config.commit_lag_window)
                .unwrap_or_else(|_| chrono::Duration::zero());

        for ChangeLogRow {
            pk_entity_change_log: pk,
            id,
            fk_customer_org: org,
            fk_contact: contact,
            object_type: obj_type,
            object_id: obj_id,
            modification_type: mod_type,
            change_status: status,
            object_data: data,
            object_data_before: data_before,
            extra_metadata: meta,
            created_at: created,
            tenant_id: tenant,
            duration_ms,
            seq,
            actor_type,
            acting_for,
            schema_version,
        } in rows
        {
            let created_at_str =
                created.map_or_else(|| Utc::now().to_rfc3339(), |dt| dt.to_rfc3339());

            // Below the scan floor AND older than the window: nothing but the
            // full sweep could have surfaced this row.
            if pk <= scan_floor && created.is_some_and(|dt| dt <= window_horizon) {
                late_recovered += 1;
            }

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
                object_data_before: data_before,
                extra_metadata: meta,
                created_at: created_at_str,
                // Trinity: tenant_id is the public-facing UUID partition stamp,
                // kept distinct from fk_customer_org above.
                tenant_id: tenant.map(|t| t.to_string()),
                duration_ms,
                seq,
                // #390 actor envelope. acting_for is a UUID column; project it to
                // a string like tenant_id so downstream readers stay string-typed.
                actor_type,
                acting_for: acting_for.map(|u| u.to_string()),
                // #377 producer schema version (TEXT) — already string-typed.
                schema_version,
            });

            // Advance the scan bound and suppress in-process re-delivery until
            // the driver records the dispatch (or the window lapses).
            self.last_processed_id = self.last_processed_id.max(pk);
            self.in_flight.push_back((Instant::now(), pk));
        }

        if late_recovered > 0 {
            warn!(
                listener_id = %self.config.listener_id,
                late_recovered,
                commit_lag_window_secs = self.config.commit_lag_window.as_secs(),
                "Recovered change-log rows whose transaction outlived the commit-lag window; \
                 they were delivered late. Raise commit_lag_window above the longest \
                 change-log-writing transaction."
            );
        }

        debug!("Fetched {} entries from change log", entries.len());

        Ok(entries)
    }

    /// Get the current checkpoint (last processed ID)
    #[must_use]
    pub const fn checkpoint(&self) -> i64 {
        self.last_processed_id
    }

    /// Set checkpoint (for recovery).
    ///
    /// Declares that everything at or below `id` was already dispatched, so it
    /// moves the sweep floor as well as the scan bound (#935) — a recovery point
    /// a sweep could drop below would replay history.
    pub const fn set_checkpoint(&mut self, id: i64) {
        self.last_processed_id = id;
        self.resume_floor = id;
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
