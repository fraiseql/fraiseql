//! The outbox drain worker.
//!
//! Reads the executor-written `core.tb_entity_change_log` outbox (this crate
//! never appends — the executor is the writer), fans each matching row out to a
//! per-sink delivery-tracking row, and publishes due rows to the broker with
//! at-least-once semantics and exponential-backoff retry / dead-lettering.
//!
//! # Delivery contract
//!
//! - **At-least-once.** Consumers dedup on `(object_type, seq)` — the `Nats-Msg-Id` the NATS sink
//!   stamps. A crash between publish and outcome recording re-publishes that row after its claim
//!   lease expires.
//! - **Ordered from the head.** Rows are published in `seq` order, and a row that fails
//!   *transiently* blocks its successors (head-of-line blocking) instead of being overtaken — a
//!   later `UPDATE` can never reach the broker before the `INSERT` it depends on is either
//!   delivered or dead-lettered. A *dead-lettered* row releases its successors; that is the
//!   documented escape hatch, and the `dead` state is the alert surface for it.
//! - **No row is ever silently skipped.** The enqueue step is an anti-join against the tracking
//!   table, not a seq watermark: a row whose transaction commits *after* higher-seq rows were
//!   already enqueued is picked up on the next tick. The anti-join scans the recent outbox window
//!   ([`DrainWorker::with_commit_lag_window`]); a periodic full sweep (first tick and every
//!   [`DrainWorker::with_sweep_every`] ticks) catches rows whose writing transaction outlived the
//!   window, counts them in [`DrainStats::late_recovered`] and warns.
//!
//! # Tick shape
//!
//! A tick is **enqueue, claim, publish, record** — with *no database
//! transaction held across broker I/O*. The claim is one atomic `UPDATE` that
//! marks a contiguous-from-head prefix of due rows `in_flight` under a lease;
//! publishing then runs on an idle connection, and each outcome is recorded as
//! its own short statement. A slow broker therefore cannot pin the vacuum
//! horizon or hold row locks (#814). The lease (default 10 minutes,
//! [`DrainWorker::with_lease`]) is the crash-recovery bound: rows claimed by a
//! worker that died become claimable again when it lapses.

use std::{
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};

use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::postgres::PgPool;
use uuid::Uuid;

use crate::{
    error::Result,
    event::{ChangeEvent, ChangeOp},
    sink::{CdcSink, CdcSinkConfig, PublishOutcome, next_attempt_delay},
};

/// Default bound on how long after `created_at` a writing transaction may
/// commit and still be caught by the cheap per-tick enqueue scan. Rows that
/// commit later than this are only picked up by the periodic full sweep.
const DEFAULT_COMMIT_LAG_WINDOW: Duration = Duration::from_secs(15 * 60);

/// Default tick period of the full recovery sweep (first tick always sweeps).
const DEFAULT_SWEEP_EVERY: u64 = 256;

/// Default claim lease: rows a dead worker left `in_flight` become claimable
/// again after this long.
const DEFAULT_LEASE: Duration = Duration::from_secs(10 * 60);

/// Idempotent enqueue: materialise a `pending` tracking row for every new,
/// matching outbox row visible in the recent window. An anti-join against the
/// tracking table — **not** a `MAX(seq)` watermark, which permanently dropped
/// any row whose transaction committed after a higher-seq row was already
/// enqueued (#797). `$1` sink name, `$2` table allow-list (or NULL), `$3`
/// tenant allow-list (or NULL), `$4` max attempts, `$5` window seconds.
///
/// The `created_at > now() - window` bound keeps the scan cheap (indexed by
/// migration 08's `idx_entity_log_created`); [`sweep_sql`] covers the
/// complement.
const fn enqueue_sql() -> &'static str {
    "\
INSERT INTO core.tb_cdc_sink_state
    (sink_name, pk_entity_change_log, seq, tenant_id, table_name, op, max_attempts)
SELECT $1, e.pk_entity_change_log, e.seq, e.tenant_id, e.object_type, e.modification_type, $4
FROM core.tb_entity_change_log e
LEFT JOIN core.tb_cdc_sink_state s
    ON s.sink_name = $1 AND s.pk_entity_change_log = e.pk_entity_change_log
WHERE s.pk_cdc_sink_state IS NULL
  AND e.created_at > now() - make_interval(secs => $5)
  AND ($2::text[] IS NULL OR e.object_type = ANY($2::text[]))
  AND ($3::uuid[] IS NULL OR e.tenant_id = ANY($3::uuid[]))
ORDER BY e.seq
ON CONFLICT (sink_name, pk_entity_change_log) DO NOTHING"
}

/// The full-sweep complement of [`enqueue_sql`]: untracked matching rows *older*
/// than the window. On a sink's first tick this is the initial backfill; on any
/// later tick a hit here means a writing transaction outlived the window and is
/// reported loudly ([`DrainStats::late_recovered`]).
const fn sweep_sql() -> &'static str {
    "\
INSERT INTO core.tb_cdc_sink_state
    (sink_name, pk_entity_change_log, seq, tenant_id, table_name, op, max_attempts)
SELECT $1, e.pk_entity_change_log, e.seq, e.tenant_id, e.object_type, e.modification_type, $4
FROM core.tb_entity_change_log e
LEFT JOIN core.tb_cdc_sink_state s
    ON s.sink_name = $1 AND s.pk_entity_change_log = e.pk_entity_change_log
WHERE s.pk_cdc_sink_state IS NULL
  AND e.created_at <= now() - make_interval(secs => $5)
  AND ($2::text[] IS NULL OR e.object_type = ANY($2::text[]))
  AND ($3::uuid[] IS NULL OR e.tenant_id = ANY($3::uuid[]))
ORDER BY e.seq
ON CONFLICT (sink_name, pk_entity_change_log) DO NOTHING"
}

/// Atomically claim a contiguous-from-head prefix of due rows.
///
/// `unpub` is every unfinished row for the sink; `is_due` is per-status
/// (pending/retrying past backoff, or `in_flight` with a lapsed lease). The
/// `bool_and(...) OVER (ORDER BY seq)` prefix means a row is claimable only if
/// **every** unfinished row before it is also due — head-of-line blocking
/// (#815), which doubles as the multi-worker ordering guard: a second worker
/// sees the first worker's unexpired `in_flight` head and claims nothing, so
/// batches can never interleave out of order. The re-stated predicate on the
/// final `UPDATE` makes the claim race-safe under `READ COMMITTED` re-checks.
///
/// `$1` sink name, `$2` batch limit, `$3` lease seconds.
const fn claim_sql() -> &'static str {
    "\
WITH unpub AS (
    SELECT s.pk_cdc_sink_state, s.seq,
           CASE
               WHEN s.status IN ('pending', 'retrying')
                   THEN (s.next_attempt_at IS NULL OR s.next_attempt_at <= now())
               WHEN s.status = 'in_flight'
                   THEN s.lease_expires_at IS NOT NULL AND s.lease_expires_at <= now()
               ELSE false
           END AS is_due
    FROM core.tb_cdc_sink_state s
    WHERE s.sink_name = $1
      AND s.status IN ('pending', 'retrying', 'in_flight')
),
pref AS (
    SELECT pk_cdc_sink_state, seq,
           bool_and(is_due) OVER (
               ORDER BY seq ROWS BETWEEN UNBOUNDED PRECEDING AND CURRENT ROW
           ) AS head_clear
    FROM unpub
),
pick AS (
    SELECT pk_cdc_sink_state FROM pref WHERE head_clear ORDER BY seq LIMIT $2
)
UPDATE core.tb_cdc_sink_state t
SET status = 'in_flight',
    lease_expires_at = now() + make_interval(secs => $3)
FROM pick
WHERE t.pk_cdc_sink_state = pick.pk_cdc_sink_state
  AND ((t.status IN ('pending', 'retrying')
            AND (t.next_attempt_at IS NULL OR t.next_attempt_at <= now()))
       OR (t.status = 'in_flight'
            AND t.lease_expires_at IS NOT NULL AND t.lease_expires_at <= now()))
RETURNING t.pk_cdc_sink_state"
}

/// Fetch the claimed rows joined to their outbox payload, in publish order.
/// No lock: the rows are ours by lease. `$1` claimed pks.
const fn claimed_payload_sql() -> &'static str {
    "\
SELECT s.pk_cdc_sink_state, s.seq, s.tenant_id, s.table_name, s.op,
       s.attempt_count, s.max_attempts,
       e.object_data        AS after_data,
       e.object_data_before AS before_data,
       e.object_id,
       e.commit_time
FROM core.tb_cdc_sink_state s
JOIN core.tb_entity_change_log e ON e.pk_entity_change_log = s.pk_entity_change_log
WHERE s.pk_cdc_sink_state = ANY($1)
ORDER BY s.seq"
}

/// A claimed tracking row joined to its outbox payload.
#[derive(sqlx::FromRow)]
struct DueRow {
    pk_cdc_sink_state: i64,
    seq:               i64,
    tenant_id:         Option<Uuid>,
    table_name:        String,
    op:                String,
    attempt_count:     i32,
    max_attempts:      i32,
    after_data:        Option<Value>,
    before_data:       Option<Value>,
    object_id:         Option<Uuid>,
    commit_time:       Option<DateTime<Utc>>,
}

impl DueRow {
    fn to_change_event(&self) -> ChangeEvent {
        let mut ev = ChangeEvent::new(
            self.seq,
            self.table_name.clone(),
            ChangeOp::from_modification_type(&self.op),
        );
        if let Some(t) = self.tenant_id {
            ev = ev.with_tenant(t);
        }
        if let Some(id) = self.object_id {
            ev = ev.with_object_id(id);
        }
        if let Some(after) = &self.after_data {
            ev = ev.with_after(after.clone());
        }
        if let Some(before) = &self.before_data {
            ev = ev.with_before(before.clone());
        }
        ev.commit_time = self.commit_time;
        ev
    }
}

/// What one drain tick did.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct DrainStats {
    /// New tracking rows materialised this tick.
    pub enqueued:       u64,
    /// Rows the broker acknowledged.
    pub published:      u64,
    /// Rows that failed transiently and were scheduled for retry.
    pub retried:        u64,
    /// Rows that exhausted retries or failed permanently (dead-lettered).
    pub dead:           u64,
    /// Rows the full sweep recovered after their writing transaction outlived
    /// the commit-lag window (loud: each sweep hit is also logged at `warn`).
    /// Zero on the first tick, whose sweep is the normal initial backfill.
    pub late_recovered: u64,
}

/// Drains the change-log outbox to a single [`CdcSink`].
///
/// Generic over the sink type (no `dyn CdcSink`), so the trait's native
/// `async fn` is sufficient and no `async_trait` macro is introduced. Multi-sink
/// fan-out is a later phase. Multiple workers may drain the *same* sink: the
/// head-of-line claim keeps delivery ordered by letting at
/// most one worker hold the head at a time.
pub struct DrainWorker<S> {
    pool:              PgPool,
    sink:              S,
    config:            CdcSinkConfig,
    batch_size:        i64,
    commit_lag_window: Duration,
    sweep_every:       u64,
    lease:             Duration,
    ticks:             AtomicU64,
}

impl<S: CdcSink + Send + Sync> DrainWorker<S> {
    /// Create a worker for one sink. The `config` filters and template govern
    /// which outbox rows reach the sink and how their subjects render.
    #[must_use]
    pub const fn new(pool: PgPool, sink: S, config: CdcSinkConfig) -> Self {
        Self {
            pool,
            sink,
            config,
            batch_size: 256,
            commit_lag_window: DEFAULT_COMMIT_LAG_WINDOW,
            sweep_every: DEFAULT_SWEEP_EVERY,
            lease: DEFAULT_LEASE,
            ticks: AtomicU64::new(0),
        }
    }

    /// Override the per-tick publish batch size (default 256).
    #[must_use]
    pub const fn with_batch_size(mut self, batch_size: i64) -> Self {
        self.batch_size = batch_size;
        self
    }

    /// Override how long after `created_at` a writing transaction may commit
    /// and still be caught by the cheap per-tick scan (default 15 minutes).
    /// Rows that commit later are recovered by the periodic sweep instead.
    #[must_use]
    pub const fn with_commit_lag_window(mut self, window: Duration) -> Self {
        self.commit_lag_window = window;
        self
    }

    /// Override how often (in ticks) the full recovery sweep runs (default
    /// 256; the first tick always sweeps, which is also the initial backfill
    /// for a newly configured sink).
    #[must_use]
    pub const fn with_sweep_every(mut self, every: u64) -> Self {
        self.sweep_every = every;
        self
    }

    /// Override the claim lease (default 10 minutes): how long a crashed
    /// worker's `in_flight` rows stay unclaimable before another worker may
    /// re-publish them.
    #[must_use]
    pub const fn with_lease(mut self, lease: Duration) -> Self {
        self.lease = lease;
        self
    }

    /// Borrow the underlying sink (e.g. to inspect health, or recorded state in
    /// tests).
    #[must_use]
    pub const fn sink(&self) -> &S {
        &self.sink
    }

    /// Run one drain tick: enqueue new outbox rows, then claim and publish due
    /// rows. No database transaction is held while the broker is called.
    ///
    /// # Errors
    ///
    /// Returns [`crate::CdcError::Database`] on any database failure.
    pub async fn tick(&self) -> Result<DrainStats> {
        let tick_index = self.ticks.fetch_add(1, Ordering::Relaxed);
        let (enqueued, late_recovered) = self.enqueue(tick_index).await?;
        let stats = self.publish_due().await?;
        tracing::debug!(
            sink = %self.config.name,
            enqueued,
            late_recovered,
            published = stats.published,
            retried = stats.retried,
            dead = stats.dead,
            "cdc drain tick",
        );
        Ok(DrainStats {
            enqueued,
            late_recovered,
            ..stats
        })
    }

    /// Enqueue new outbox rows: the windowed anti-join every tick, plus the
    /// full sweep on the first tick and every `sweep_every`-th tick.
    /// Returns `(total enqueued, late-recovered)`.
    async fn enqueue(&self, tick_index: u64) -> Result<(u64, u64)> {
        let window_secs = self.commit_lag_window.as_secs_f64();
        let recent = sqlx::query(enqueue_sql())
            .bind(&self.config.name)
            .bind(self.config.tables.as_deref())
            .bind(self.config.tenants.as_deref())
            .bind(self.config.max_attempts)
            .bind(window_secs)
            .execute(&self.pool)
            .await?
            .rows_affected();

        let sweep_due = self.sweep_every == 0 || tick_index.is_multiple_of(self.sweep_every);
        if !sweep_due {
            return Ok((recent, 0));
        }

        let swept = sqlx::query(sweep_sql())
            .bind(&self.config.name)
            .bind(self.config.tables.as_deref())
            .bind(self.config.tenants.as_deref())
            .bind(self.config.max_attempts)
            .bind(window_secs)
            .execute(&self.pool)
            .await?
            .rows_affected();

        if tick_index == 0 {
            if swept > 0 {
                tracing::info!(
                    sink = %self.config.name,
                    rows = swept,
                    "cdc first-tick sweep enqueued pre-existing outbox rows (initial backfill)",
                );
            }
            Ok((recent + swept, 0))
        } else {
            if swept > 0 {
                tracing::warn!(
                    sink = %self.config.name,
                    rows = swept,
                    window_secs,
                    "cdc sweep recovered outbox rows whose writing transaction \
                     outlived the commit-lag window — they are enqueued now, but \
                     consider raising the window",
                );
            }
            Ok((recent + swept, swept))
        }
    }

    /// Claim a head-contiguous batch of due rows, publish them with no open
    /// database transaction, and record each outcome as its own statement.
    async fn publish_due(&self) -> Result<DrainStats> {
        let claimed: Vec<i64> = sqlx::query_scalar(claim_sql())
            .bind(&self.config.name)
            .bind(self.batch_size)
            .bind(self.lease.as_secs_f64())
            .fetch_all(&self.pool)
            .await?;
        if claimed.is_empty() {
            return Ok(DrainStats::default());
        }

        let rows: Vec<DueRow> = sqlx::query_as(claimed_payload_sql())
            .bind(&claimed)
            .fetch_all(&self.pool)
            .await?;

        let mut published = 0u64;
        let mut retried = 0u64;
        let mut dead = 0u64;

        for (idx, row) in rows.iter().enumerate() {
            let event = row.to_change_event();
            match self.sink.publish(&event).await {
                PublishOutcome::Published => {
                    sqlx::query(
                        "UPDATE core.tb_cdc_sink_state \
                         SET status = 'published', published_at = now(), \
                             lease_expires_at = NULL \
                         WHERE pk_cdc_sink_state = $1",
                    )
                    .bind(row.pk_cdc_sink_state)
                    .execute(&self.pool)
                    .await?;
                    published += 1;
                },
                PublishOutcome::Transient(error) => {
                    let attempt = row.attempt_count + 1;
                    let is_dead = attempt >= row.max_attempts;
                    let status = if is_dead { "dead" } else { "retrying" };
                    let delay = next_attempt_delay(u32::try_from(attempt).unwrap_or(u32::MAX));
                    sqlx::query(
                        "UPDATE core.tb_cdc_sink_state \
                         SET status = $2, attempt_count = $3, \
                             next_attempt_at = now() + make_interval(secs => $4), \
                             last_error = $5, lease_expires_at = NULL \
                         WHERE pk_cdc_sink_state = $1",
                    )
                    .bind(row.pk_cdc_sink_state)
                    .bind(status)
                    .bind(attempt)
                    .bind(delay.as_secs_f64())
                    .bind(&error)
                    .execute(&self.pool)
                    .await?;
                    if is_dead {
                        dead += 1;
                    } else {
                        retried += 1;
                    }
                    // Head-of-line blocking: a transiently failed row must not
                    // be overtaken. Release the rest of the batch and stop —
                    // they become claimable again once this row clears.
                    let remaining: Vec<i64> =
                        rows[idx + 1..].iter().map(|r| r.pk_cdc_sink_state).collect();
                    if !remaining.is_empty() {
                        sqlx::query(
                            "UPDATE core.tb_cdc_sink_state \
                             SET status = 'pending', lease_expires_at = NULL \
                             WHERE pk_cdc_sink_state = ANY($1) AND status = 'in_flight'",
                        )
                        .bind(&remaining)
                        .execute(&self.pool)
                        .await?;
                    }
                    break;
                },
                PublishOutcome::Permanent(error) => {
                    // Dead-lettering releases successors (documented escape
                    // hatch): the row can never succeed, so blocking on it
                    // would wedge the stream forever.
                    sqlx::query(
                        "UPDATE core.tb_cdc_sink_state \
                         SET status = 'dead', attempt_count = attempt_count + 1, \
                             last_error = $2, lease_expires_at = NULL \
                         WHERE pk_cdc_sink_state = $1",
                    )
                    .bind(row.pk_cdc_sink_state)
                    .bind(&error)
                    .execute(&self.pool)
                    .await?;
                    dead += 1;
                },
            }
        }

        Ok(DrainStats {
            enqueued: 0,
            published,
            retried,
            dead,
            late_recovered: 0,
        })
    }
}

#[cfg(test)]
mod tests;
