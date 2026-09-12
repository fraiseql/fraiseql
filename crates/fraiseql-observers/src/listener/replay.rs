//! Durable resume for a streaming consumer: read back what a client already
//! received, in the order it received it (#1310).
//!
//! # Why the delivery order is not the sequence order
//!
//! The obvious resume — "give me every row with `seq` greater than the last one I
//! saw" — is wrong here, and wrong in the way that is hardest to see: it looks
//! correct and loses rows silently.
//!
//! `seq` comes from a plain `nextval` default (migration 08), so it is allocated when
//! the writing transaction *inserts* and becomes visible when that transaction
//! *commits*. Under any concurrent write the two orders diverge. Measured against a
//! real database:
//!
//! ```text
//! tx A: INSERT → seq 60003, transaction still open
//! tx B: INSERT → seq 60004, COMMITs first
//! poller (sees only B):  delivers seq 60004 → the client stores Last-Event-ID: 60004
//! tx A COMMITs
//! poller (next batch):   delivers seq 60003 → arrives AFTER the higher seq
//! ```
//!
//! A client that disconnects in between holds `60004`, and `WHERE seq > 60004` returns
//! nothing: `60003` is lost for ever, under a healthy-looking `200`. The client's
//! stored id is the *last* event it received, never the *highest* — so it is not a
//! watermark and cannot be used as one. This is the defect family #935 and #797 already
//! cost this project twice, and migration 14 states the remedy: answer the question
//! with a recorded fact rather than with an ordering assumption that does not hold.
//!
//! # The fact this reader uses
//!
//! `core.tb_observer_dispatch` records every row the poller dispatched, stamped with
//! the batch that dispatched it. The event fan-out a streaming client reads from is fed
//! from inside that same batch loop, in `pk_entity_change_log` order, so
//! `(dispatched_at, pk_entity_change_log)` **is** the order the client saw. Everything
//! strictly after the client's anchor in that order is exactly what it missed — no
//! window, no assumption about transaction duration, and nothing re-sent that it
//! already had.
//!
//! # What it costs
//!
//! Two additive indexes, both installed by the migrations that own their tables:
//! `idx_observer_dispatch_dispatched` (migration 14) makes the ledger side an index
//! range over just the rows since the anchor, and `idx_entity_log_id` (migration 08)
//! gives the join its referenced side. Without them the same query hash-joins the whole
//! ledger against every row of the entity type — measured at 13 ms on a 60 000-row log,
//! and linear in the log from there.
//!
//! **Requires the `postgres` Cargo feature.**

#[cfg(not(feature = "postgres"))]
compile_error!(
    "`fraiseql-observers::listener::replay` requires the `postgres` feature. \
     Enable it with: fraiseql-observers = { features = [\"postgres\"] }"
);

use chrono::{DateTime, Utc};
use sqlx::postgres::PgPool;

use super::change_log::{CHANGE_LOG_PROJECTION, ChangeLogEntry, ChangeLogRow};
use crate::{
    error::{ObserverError, Result},
    event::EntityEvent,
};

/// Which events a resumed stream may see — the same two gates the live stream applies.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplayScope {
    /// The change log's `object_type`, which is the **GraphQL type name** (`User`),
    /// never a transport's route name (`users`). Same string the live gate matches.
    pub object_type: String,

    /// The tenant whose events this stream carries, or `None` for a single-tenant
    /// deployment that subscribes unscoped.
    ///
    /// Compared as **text** (`tenant_id::text = $tenant`) rather than parsed to a
    /// `Uuid`, because that is precisely what the live gate compares: the fanned-out
    /// event carries `tenant_id` already projected to its canonical string, and a
    /// principal whose tenant is spelled any other way matches no live event. Parsing
    /// here would make a spelling that the live stream rejects match on replay — two
    /// gates, two answers, for the same subscription.
    pub tenant: Option<String>,
}

/// Where in the delivery order a resumed stream continues from.
///
/// Ordered by `(dispatched_at, pk)`, with an unrecorded dispatch sorting *after* every
/// recorded one — see [`ChangeLogReplayReader::anchor`] for when that happens and why
/// it is the safe direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResumePosition {
    /// When the batch carrying this row was recorded as dispatched, or `None` when the
    /// row has been delivered but its batch has not been recorded yet.
    pub dispatched_at: Option<DateTime<Utc>>,
    /// The row's `pk_entity_change_log`, which breaks ties inside one batch — and is
    /// the batch's own delivery order.
    pub pk:            i64,
}

/// What the reader could make of a client's `Last-Event-ID`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResumeAnchor {
    /// The named event is in the change log, at this position in the delivery order.
    Found(ResumePosition),
    /// No row in this scope carries that sequence.
    ///
    /// Either it was pruned from the change log, or the id came from somewhere other
    /// than this stream. Both are unresumable for the same reason: with the anchor gone
    /// there is no way to establish what came after it, and answering anyway would be
    /// the silent gap this whole mechanism exists to refuse.
    Unknown,
}

/// One row of a replay: the event, and the position to continue from after it.
#[derive(Debug, Clone)]
pub struct ReplayedEvent {
    /// The position of this event in the delivery order — pass the last one back to
    /// [`ChangeLogReplayReader::page`] to continue.
    pub position: ResumePosition,
    /// The event itself, decoded by the same projection the poller uses, so a replayed
    /// event and the live event for the same row are one value.
    pub event:    EntityEvent,
}

/// Reads `core.tb_entity_change_log` in the order a streaming consumer received it.
///
/// Read-only, and a consumer of nothing: it takes no lease, marks no row dispatched and
/// removes nothing from any other reader's view. A client replaying through it cannot
/// take an event away from the observer executor — the property #1309 had to introduce
/// a broadcast fan-out to obtain for the *live* path, obtained here by construction.
#[derive(Debug, Clone)]
pub struct ChangeLogReplayReader {
    pool:        PgPool,
    listener_id: String,
}

/// One replay row: the contract columns plus the ledger position that ordered them.
#[derive(sqlx::FromRow)]
struct DispatchedRow {
    #[sqlx(flatten)]
    row:           ChangeLogRow,
    dispatched_at: DateTime<Utc>,
}

impl ChangeLogReplayReader {
    /// Create a reader over one deployment's dispatch ledger.
    ///
    /// `listener_id` must be the identity the observer runtime polls under
    /// (`ObserverRuntimeConfig::listener_id`), because that is whose ledger records the
    /// order this deployment's clients were served in. All replicas of one deployment
    /// share it — they are the same logical consumer — so a client that reconnects
    /// through a load balancer to a different replica resumes from the same recorded
    /// order, which is what an in-process replay buffer could never offer (#874).
    #[must_use]
    pub const fn new(pool: PgPool, listener_id: String) -> Self {
        Self { pool, listener_id }
    }

    /// Resolve a client's `Last-Event-ID` to a position in the delivery order.
    ///
    /// Two outcomes worth stating:
    ///
    /// - **The row has no ledger entry yet.** The ledger is written *after* a batch is forwarded,
    ///   so a client that reconnects inside that window names a real event with no recorded
    ///   position. That is [`ResumePosition::dispatched_at`] `None`, and the page below falls back
    ///   to continuing by `pk`. It re-sends rather than skips, and the caller's seam de-duplication
    ///   absorbs the overlap — the safe direction, taken without a timing dependency or a refusal a
    ///   browser could not retry.
    /// - **Two rows share a sequence.** A duplicate `seq` is a producer contract violation (the
    ///   column defaults from a sequence). The earliest row wins, because resuming from the earlier
    ///   of two candidates re-sends and resuming from the later one skips.
    ///
    /// # Errors
    ///
    /// Returns [`ObserverError::DatabaseError`] if the query fails.
    pub async fn anchor(&self, scope: &ReplayScope, seq: i64) -> Result<ResumeAnchor> {
        let row: Option<(i64, Option<DateTime<Utc>>)> = sqlx::query_as(
            r"
            SELECT e.pk_entity_change_log, d.dispatched_at
            FROM core.tb_entity_change_log e
            LEFT JOIN core.tb_observer_dispatch d
                   ON d.change_log_id = e.id
                  AND d.listener_id = $1
            WHERE e.object_type = $2
              AND e.seq = $3
              AND ($4::text IS NULL OR e.tenant_id::text = $4)
            ORDER BY e.pk_entity_change_log ASC
            LIMIT 1
            ",
        )
        .bind(&self.listener_id)
        .bind(&scope.object_type)
        .bind(seq)
        .bind(scope.tenant.as_ref())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| ObserverError::DatabaseError {
            reason: format!("Failed to resolve the resume anchor: {e}"),
        })?;

        Ok(row.map_or(ResumeAnchor::Unknown, |(pk, dispatched_at)| {
            ResumeAnchor::Found(ResumePosition { dispatched_at, pk })
        }))
    }

    /// How many dispatched rows lie between a position and the head, counting at most
    /// `limit`.
    ///
    /// Counted **across every entity type**, not just this stream's, and deliberately:
    /// this is the bound on how much *work* a resume may cost, and the work is the walk
    /// through the ledger, whatever fraction of it the caller's resource turns out to
    /// be. A caller that refuses above its cap has then bounded the page reads that
    /// follow as well.
    ///
    /// Stops counting at `limit`, so the query never reads more than the caller is
    /// willing to serve — a client resuming from a week ago costs one bounded index
    /// scan, not a week of rows.
    ///
    /// # Errors
    ///
    /// Returns [`ObserverError::DatabaseError`] if the query fails.
    pub async fn count_since(&self, from: &ResumePosition, limit: u64) -> Result<u64> {
        let bound = i64::try_from(limit).unwrap_or(i64::MAX);

        let count: i64 = if let Some(dispatched_at) = from.dispatched_at {
            sqlx::query_scalar(
                r"
                SELECT count(*) FROM (
                    SELECT 1
                    FROM core.tb_observer_dispatch d
                    WHERE d.listener_id = $1
                      AND d.dispatched_at >= $2
                    ORDER BY d.dispatched_at
                    LIMIT $3
                ) bounded
                ",
            )
            .bind(&self.listener_id)
            .bind(dispatched_at)
            .bind(bound)
            .fetch_one(&self.pool)
            .await
        } else {
            sqlx::query_scalar(
                r"
                SELECT count(*) FROM (
                    SELECT 1
                    FROM core.tb_entity_change_log e
                    WHERE e.pk_entity_change_log > $1
                    ORDER BY e.pk_entity_change_log
                    LIMIT $2
                ) bounded
                ",
            )
            .bind(from.pk)
            .bind(bound)
            .fetch_one(&self.pool)
            .await
        }
        .map_err(|e| ObserverError::DatabaseError {
            reason: format!("Failed to measure the resume backlog: {e}"),
        })?;

        Ok(u64::try_from(count).unwrap_or(0))
    }

    /// How many rows after `from` the ledger has not placed yet, counting at most
    /// `limit`.
    ///
    /// The companion bound to [`count_since`](Self::count_since), for the rows
    /// [`page_in_flight`](Self::page_in_flight) serves.
    ///
    /// # Errors
    ///
    /// Returns [`ObserverError::DatabaseError`] if the query fails.
    pub async fn count_in_flight(
        &self,
        scope: &ReplayScope,
        from: &ResumePosition,
        limit: u64,
    ) -> Result<u64> {
        let count: i64 = sqlx::query_scalar(
            r"
            SELECT count(*) FROM (
                SELECT 1
                FROM core.tb_entity_change_log e
                WHERE e.pk_entity_change_log > $1
                  AND e.object_type = $2
                  AND ($3::text IS NULL OR e.tenant_id::text = $3)
                  AND NOT EXISTS (
                        SELECT 1 FROM core.tb_observer_dispatch d
                        WHERE d.listener_id = $4 AND d.change_log_id = e.id
                      )
                ORDER BY e.pk_entity_change_log
                LIMIT $5
            ) bounded
            ",
        )
        .bind(from.pk)
        .bind(&scope.object_type)
        .bind(scope.tenant.as_ref())
        .bind(&self.listener_id)
        .bind(i64::try_from(limit).unwrap_or(i64::MAX))
        .fetch_one(&self.pool)
        .await
        .map_err(|e| ObserverError::DatabaseError {
            reason: format!("Failed to measure the in-flight tail: {e}"),
        })?;

        Ok(u64::try_from(count).unwrap_or(0))
    }

    /// The rows after `from` that the ledger has **not** placed in the delivery order
    /// yet, in insertion order.
    ///
    /// This closes a race that is otherwise invisible, and therefore the dangerous kind.
    /// The poller publishes a batch to the fan-out and records it *afterwards* — that
    /// ordering is what makes delivery at-least-once. So an event can be published while
    /// a client is disconnected and recorded after that client has reconnected and read
    /// its catch-up pages: the live receiver never saw it (it subscribed later) and the
    /// ledger could not yet place it (it was not recorded yet). Neither path carries it,
    /// the stream looks healthy, and the client's id advances past it on the next event
    /// — the exact silent gap this endpoint has now been corrected for three times
    /// (#873.4, #1113, #1310).
    ///
    /// Reading the tail by insertion order **over-sends** in the ordinary case: rows the
    /// poller simply has not reached yet are delivered here and again by the live stream
    /// once it does. That is at-least-once, it is what the caller's seam de-duplication
    /// is for, and it is the correct direction to err in.
    ///
    /// # Errors
    ///
    /// Returns [`ObserverError::DatabaseError`] if the query fails, or
    /// [`ObserverError::TemplateRenderingFailed`] if a row cannot be decoded.
    pub async fn page_in_flight(
        &self,
        scope: &ReplayScope,
        from: &ResumePosition,
        batch_size: u32,
    ) -> Result<Vec<ReplayedEvent>> {
        let rows: Vec<ChangeLogRow> = sqlx::query_as(&format!(
            r"
            SELECT {CHANGE_LOG_PROJECTION}
            FROM core.tb_entity_change_log e
            WHERE e.pk_entity_change_log > $1
              AND e.object_type = $2
              AND ($3::text IS NULL OR e.tenant_id::text = $3)
              AND NOT EXISTS (
                    SELECT 1 FROM core.tb_observer_dispatch d
                    WHERE d.listener_id = $4 AND d.change_log_id = e.id
                  )
            ORDER BY e.pk_entity_change_log
            LIMIT $5
            "
        ))
        .bind(from.pk)
        .bind(&scope.object_type)
        .bind(scope.tenant.as_ref())
        .bind(&self.listener_id)
        .bind(i64::from(batch_size))
        .fetch_all(&self.pool)
        .await
        .map_err(|e| ObserverError::DatabaseError {
            reason: format!("Failed to read the in-flight tail: {e}"),
        })?;

        rows.into_iter()
            .map(|row| {
                let pk = row.pk_entity_change_log;
                Ok(ReplayedEvent {
                    position: ResumePosition {
                        dispatched_at: None,
                        pk,
                    },
                    event:    ChangeLogEntry::from(row).to_entity_event()?,
                })
            })
            .collect()
    }

    /// The next page of events after `from`, in the order they were delivered.
    ///
    /// A page shorter than `batch_size` means the replay has reached the head as of
    /// this read. Call again with the last returned position to continue.
    ///
    /// # Errors
    ///
    /// Returns [`ObserverError::DatabaseError`] if the query fails, or
    /// [`ObserverError::TemplateRenderingFailed`] if a row cannot be decoded into an
    /// event — the same decode the poller performs, so a row that fails here would have
    /// failed the live path too.
    pub async fn page(
        &self,
        scope: &ReplayScope,
        from: &ResumePosition,
        batch_size: u32,
    ) -> Result<Vec<ReplayedEvent>> {
        let limit = i64::from(batch_size);

        if let Some(dispatched_at) = from.dispatched_at {
            // The ledger drives: an index range over exactly the batches recorded at or
            // after the anchor's own, tie-broken inside that batch by pk. The two
            // predicates are deliberately not one row-wise comparison — a tuple
            // comparison spanning both tables cannot be an index condition on either,
            // so the planner falls back to hashing the whole ledger.
            let rows: Vec<DispatchedRow> = sqlx::query_as(&format!(
                r"
                SELECT {CHANGE_LOG_PROJECTION}, d.dispatched_at
                FROM core.tb_observer_dispatch d
                JOIN core.tb_entity_change_log e ON e.id = d.change_log_id
                WHERE d.listener_id = $1
                  AND d.dispatched_at >= $2
                  AND (d.dispatched_at > $2 OR e.pk_entity_change_log > $3)
                  AND e.object_type = $4
                  AND ($5::text IS NULL OR e.tenant_id::text = $5)
                ORDER BY d.dispatched_at, e.pk_entity_change_log
                LIMIT $6
                "
            ))
            .bind(&self.listener_id)
            .bind(dispatched_at)
            .bind(from.pk)
            .bind(&scope.object_type)
            .bind(scope.tenant.as_ref())
            .bind(limit)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| ObserverError::DatabaseError {
                reason: format!("Failed to read the resume page: {e}"),
            })?;

            rows.into_iter()
                .map(|DispatchedRow { row, dispatched_at }| {
                    let pk = row.pk_entity_change_log;
                    Ok(ReplayedEvent {
                        position: ResumePosition {
                            dispatched_at: Some(dispatched_at),
                            pk,
                        },
                        event:    ChangeLogEntry::from(row).to_entity_event()?,
                    })
                })
                .collect()
        } else {
            // The anchor's batch was not recorded yet, so there is no delivery order to
            // read — only insertion order, which for everything after the anchor is the
            // order the poller will deliver in anyway. Over-sends by at most the rows
            // in flight; never under-sends.
            let rows: Vec<ChangeLogRow> = sqlx::query_as(&format!(
                r"
                SELECT {CHANGE_LOG_PROJECTION}
                FROM core.tb_entity_change_log e
                WHERE e.pk_entity_change_log > $1
                  AND e.object_type = $2
                  AND ($3::text IS NULL OR e.tenant_id::text = $3)
                ORDER BY e.pk_entity_change_log
                LIMIT $4
                "
            ))
            .bind(from.pk)
            .bind(&scope.object_type)
            .bind(scope.tenant.as_ref())
            .bind(limit)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| ObserverError::DatabaseError {
                reason: format!("Failed to read the resume page: {e}"),
            })?;

            rows.into_iter()
                .map(|row| {
                    let pk = row.pk_entity_change_log;
                    Ok(ReplayedEvent {
                        position: ResumePosition {
                            dispatched_at: None,
                            pk,
                        },
                        event:    ChangeLogEntry::from(row).to_entity_event()?,
                    })
                })
                .collect()
        }
    }
}

#[cfg(test)]
mod tests;
