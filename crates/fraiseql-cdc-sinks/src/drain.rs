//! The outbox drain worker.
//!
//! Reads the executor-written `core.tb_entity_change_log` outbox (this crate
//! never appends — the executor is the writer), fans each matching row out to a
//! per-sink delivery-tracking row, and publishes due rows to the broker with
//! at-least-once semantics and exponential-backoff retry / dead-lettering.
//!
//! A tick is **enqueue then publish**, decoupled so a broker outage never blocks
//! the mutation path: the executor's in-transaction outbox write already
//! committed; the drain is a separate loop. Publishing happens inside the same
//! transaction that holds the `FOR UPDATE SKIP LOCKED` row lock, so a crash
//! mid-publish rolls back to the prior state and the row is re-published on
//! restart (at-least-once; consumers dedup on `(object_type, seq)`).

use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::postgres::PgPool;
use uuid::Uuid;

use crate::{
    error::Result,
    event::{ChangeEvent, ChangeOp},
    sink::{CdcSink, CdcSinkConfig, PublishOutcome, next_attempt_delay},
};

/// Idempotent enqueue: materialise a `pending` tracking row for every new,
/// matching outbox row. The cursor is `MAX(seq)` in the tracking table for this
/// sink — durable and restart-safe with no separate cursor table. The
/// `tables`/`tenants` allow-lists are pushed to SQL (mirroring
/// [`CdcSinkConfig::matches`]). `$1` sink name, `$2` table allow-list (or NULL),
/// `$3` tenant allow-list (or NULL), `$4` max attempts.
const fn enqueue_sql() -> &'static str {
    "\
INSERT INTO core.tb_cdc_sink_state
    (sink_name, pk_entity_change_log, seq, tenant_id, table_name, op, max_attempts)
SELECT $1, e.pk_entity_change_log, e.seq, e.tenant_id, e.object_type, e.modification_type, $4
FROM core.tb_entity_change_log e
WHERE e.seq > COALESCE(
        (SELECT MAX(seq) FROM core.tb_cdc_sink_state WHERE sink_name = $1), 0)
  AND ($2::text[] IS NULL OR e.object_type = ANY($2::text[]))
  AND ($3::uuid[] IS NULL OR e.tenant_id = ANY($3::uuid[]))
ORDER BY e.seq
ON CONFLICT (sink_name, pk_entity_change_log) DO NOTHING"
}

/// Select due tracking rows (pending/retrying past their backoff) joined to the
/// outbox payload, locked for update so concurrent workers skip them. `$1` sink
/// name, `$2` batch limit.
const fn publish_select_sql() -> &'static str {
    "\
SELECT s.pk_cdc_sink_state, s.seq, s.tenant_id, s.table_name, s.op,
       s.attempt_count, s.max_attempts,
       e.object_data        AS after_data,
       e.object_data_before AS before_data,
       e.object_id,
       e.commit_time
FROM core.tb_cdc_sink_state s
JOIN core.tb_entity_change_log e ON e.pk_entity_change_log = s.pk_entity_change_log
WHERE s.sink_name = $1
  AND s.status IN ('pending', 'retrying')
  AND (s.next_attempt_at IS NULL OR s.next_attempt_at <= now())
ORDER BY s.seq
FOR UPDATE OF s SKIP LOCKED
LIMIT $2"
}

/// A due tracking row joined to its outbox payload.
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
    pub enqueued:  u64,
    /// Rows the broker acknowledged.
    pub published: u64,
    /// Rows that failed transiently and were scheduled for retry.
    pub retried:   u64,
    /// Rows that exhausted retries or failed permanently (dead-lettered).
    pub dead:      u64,
}

/// Drains the change-log outbox to a single [`CdcSink`].
///
/// Generic over the sink type (no `dyn CdcSink`), so the trait's native
/// `async fn` is sufficient and no `async_trait` macro is introduced. Multi-sink
/// fan-out is a later phase.
pub struct DrainWorker<S> {
    pool:       PgPool,
    sink:       S,
    config:     CdcSinkConfig,
    batch_size: i64,
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
        }
    }

    /// Override the per-tick publish batch size (default 256).
    #[must_use]
    pub const fn with_batch_size(mut self, batch_size: i64) -> Self {
        self.batch_size = batch_size;
        self
    }

    /// Borrow the underlying sink (e.g. to inspect health, or recorded state in
    /// tests).
    #[must_use]
    pub const fn sink(&self) -> &S {
        &self.sink
    }

    /// Run one drain tick: enqueue new outbox rows, then publish due rows.
    ///
    /// # Errors
    ///
    /// Returns [`crate::CdcError::Database`] on any database failure.
    pub async fn tick(&self) -> Result<DrainStats> {
        let enqueued = self.enqueue().await?;
        let stats = self.publish_due().await?;
        tracing::debug!(
            sink = %self.config.name,
            enqueued,
            published = stats.published,
            retried = stats.retried,
            dead = stats.dead,
            "cdc drain tick",
        );
        Ok(DrainStats { enqueued, ..stats })
    }

    async fn enqueue(&self) -> Result<u64> {
        let result = sqlx::query(enqueue_sql())
            .bind(&self.config.name)
            .bind(self.config.tables.as_deref())
            .bind(self.config.tenants.as_deref())
            .bind(self.config.max_attempts)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected())
    }

    async fn publish_due(&self) -> Result<DrainStats> {
        let mut tx = self.pool.begin().await?;
        let rows: Vec<DueRow> = sqlx::query_as(publish_select_sql())
            .bind(&self.config.name)
            .bind(self.batch_size)
            .fetch_all(&mut *tx)
            .await?;

        let mut published = 0u64;
        let mut retried = 0u64;
        let mut dead = 0u64;

        for row in &rows {
            let event = row.to_change_event();
            match self.sink.publish(&event).await {
                PublishOutcome::Published => {
                    sqlx::query(
                        "UPDATE core.tb_cdc_sink_state \
                         SET status = 'published', published_at = now() \
                         WHERE pk_cdc_sink_state = $1",
                    )
                    .bind(row.pk_cdc_sink_state)
                    .execute(&mut *tx)
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
                             last_error = $5 \
                         WHERE pk_cdc_sink_state = $1",
                    )
                    .bind(row.pk_cdc_sink_state)
                    .bind(status)
                    .bind(attempt)
                    .bind(delay.as_secs_f64())
                    .bind(&error)
                    .execute(&mut *tx)
                    .await?;
                    if is_dead {
                        dead += 1;
                    } else {
                        retried += 1;
                    }
                },
                PublishOutcome::Permanent(error) => {
                    sqlx::query(
                        "UPDATE core.tb_cdc_sink_state \
                         SET status = 'dead', attempt_count = attempt_count + 1, last_error = $2 \
                         WHERE pk_cdc_sink_state = $1",
                    )
                    .bind(row.pk_cdc_sink_state)
                    .bind(&error)
                    .execute(&mut *tx)
                    .await?;
                    dead += 1;
                },
            }
        }

        tx.commit().await?;
        Ok(DrainStats {
            enqueued: 0,
            published,
            retried,
            dead,
        })
    }
}

#[cfg(test)]
mod tests;
