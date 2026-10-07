//! A per-process tail of the change log, for subscription fan-out (#1503).
//!
//! The dispatch poller ([`super::ChangeLogListener`]) is shared: replicas of one
//! deployment poll under one listener id and one dispatch ledger, and only the
//! holder of the poll lease reads at all (#1500), so each row's actions run once. A
//! subscriber, though, is connected to one replica, and that replica must see every
//! change. This reader is the other half: every process runs its own, it records
//! nothing in the database, and it hands out each committed row once per process.
//!
//! # Why not LISTEN/NOTIFY
//!
//! A `pg_notify` trigger on the change log was measured against this reader on
//! PostgreSQL 18 with durable commits (2026-10-07). NOTIFY takes a cluster-wide lock
//! at commit, so it serialises every committing transaction that notified: a
//! mutation-shaped transaction went from 53 078 to 3 640 TPS at 64 clients and from
//! 12 878 to 3 766 at 8. Sixteen of these readers polling at 10 Hz cost the same
//! write load nothing measurable (52 571 vs 51 682 TPS), at 0.3 ms per poll.
//!
//! # Not skipping late commits
//!
//! `pk_entity_change_log` is allocated at INSERT and becomes visible at COMMIT, so a
//! row can appear below a pk this reader has already seen (#935). The dispatch
//! poller answers that with its durable ledger; this reader answers it with
//! PostgreSQL's transaction horizon instead:
//!
//! - Every read runs in one REPEATABLE READ snapshot, which also yields the oldest running
//!   transaction (`xmin`), the next unassigned one (`xmax`) and the highest visible pk.
//! - Rows above a floor that are not yet delivered are handed out; the delivered ones are
//!   remembered.
//! - The floor rises to a poll's highest visible pk only once a later poll's `xmin` has passed that
//!   poll's `xmax` — every transaction that was open then has ended, so no row at or below that pk
//!   can still appear — and at least [`SETTLE`] later. The delay covers a transaction whose first
//!   write is the change-log INSERT, which takes its pk a moment before its transaction id.
//!
//! At start the reader begins at the head of the log: rows written in the last
//! [`SETTLE`] count as already seen, so a transaction already open longer than that
//! when the process starts is not delivered to this process's subscribers. Nobody is
//! subscribed to a process that has not started.

use std::{
    collections::{BTreeSet, VecDeque},
    time::{Duration, Instant},
};

use sqlx::{PgPool, Postgres, Transaction};
use tracing::warn;

use super::change_log::{CHANGE_LOG_PROJECTION, ChangeLogEntry, ChangeLogRow};
use crate::error::{ObserverError, Result};

/// How long a floor candidate waits before it may be applied, and how far back the
/// start of the reader treats rows as already seen.
pub const SETTLE: Duration = Duration::from_secs(10);

/// Floor candidates kept before the newest replaces the last one. Replacing is
/// conservative: the newer candidate demands a later horizon and a later time.
const MAX_PENDING: usize = 1024;

/// Delivered rows above the floor past which the reader warns: the floor is held
/// down by a long-running transaction, and every poll sends this set.
const DELIVERED_WARN: usize = 50_000;

/// One poll's claim: once every transaction below `xmax` has ended, every row at
/// or below `max_pk` that will ever exist is visible.
#[derive(Debug, Clone, Copy)]
struct FloorCandidate {
    xmax:   u64,
    max_pk: i64,
    at:     Instant,
}

/// The snapshot facts one poll reads alongside its rows.
struct Horizon {
    xmin:   u64,
    xmax:   u64,
    max_pk: Option<i64>,
}

/// Per-process reader of `core.tb_entity_change_log` that hands out each committed
/// row once, independently of the dispatch ledger. See the module docs.
pub struct ChangeLogTail {
    pool:       PgPool,
    batch_size: i64,
    settle:     Duration,
    floor:      i64,
    delivered:  BTreeSet<i64>,
    pending:    VecDeque<FloorCandidate>,
    warned:     bool,
}

impl ChangeLogTail {
    /// Start at the head of the change log.
    ///
    /// # Errors
    ///
    /// Returns [`ObserverError::DatabaseError`] if the change log cannot be read.
    pub async fn start(pool: PgPool, batch_size: usize) -> Result<Self> {
        Self::start_with_settle(pool, batch_size, SETTLE).await
    }

    /// [`start`](Self::start) with an explicit settle delay (tests shorten it).
    ///
    /// # Errors
    ///
    /// Returns [`ObserverError::DatabaseError`] if the change log cannot be read.
    pub async fn start_with_settle(
        pool: PgPool,
        batch_size: usize,
        settle: Duration,
    ) -> Result<Self> {
        let mut tx = snapshot(&pool).await?;
        #[allow(clippy::cast_precision_loss)]
        // Reason: a settle delay of seconds; sub-microsecond precision is irrelevant
        let settle_secs = settle.as_secs_f64();
        let floor: i64 = sqlx::query_scalar(
            "SELECT coalesce(max(pk_entity_change_log), 0) FROM core.tb_entity_change_log \
             WHERE created_at < now() - make_interval(secs => $1)",
        )
        .bind(settle_secs)
        .fetch_one(&mut *tx)
        .await
        .map_err(db_error)?;
        let seen: Vec<i64> = sqlx::query_scalar(
            "SELECT pk_entity_change_log FROM core.tb_entity_change_log \
             WHERE pk_entity_change_log > $1",
        )
        .bind(floor)
        .fetch_all(&mut *tx)
        .await
        .map_err(db_error)?;
        tx.commit().await.map_err(db_error)?;

        Ok(Self {
            pool,
            batch_size: i64::try_from(batch_size.max(1)).unwrap_or(i64::MAX),
            settle,
            floor,
            delivered: seen.into_iter().collect(),
            pending: VecDeque::new(),
            warned: false,
        })
    }

    /// The committed rows this process has not handed out yet, in pk order, at most
    /// `batch_size` of them. Empty when there is nothing new.
    ///
    /// # Errors
    ///
    /// Returns [`ObserverError::DatabaseError`] if the change log cannot be read.
    pub async fn next_batch(&mut self) -> Result<Vec<ChangeLogEntry>> {
        let mut tx = snapshot(&self.pool).await?;
        let (xmin, xmax, max_pk): (String, String, Option<i64>) = sqlx::query_as(
            "SELECT pg_snapshot_xmin(pg_current_snapshot())::text, \
                    pg_snapshot_xmax(pg_current_snapshot())::text, \
                    (SELECT max(pk_entity_change_log) FROM core.tb_entity_change_log)",
        )
        .fetch_one(&mut *tx)
        .await
        .map_err(db_error)?;
        let delivered: Vec<i64> = self.delivered.iter().copied().collect();
        let rows: Vec<ChangeLogRow> = sqlx::query_as(&format!(
            "SELECT {CHANGE_LOG_PROJECTION}
             FROM core.tb_entity_change_log e
             WHERE e.pk_entity_change_log > $1
               AND NOT (e.pk_entity_change_log = ANY($2::bigint[]))
             ORDER BY e.pk_entity_change_log ASC
             LIMIT $3"
        ))
        .bind(self.floor)
        .bind(&delivered)
        .bind(self.batch_size)
        .fetch_all(&mut *tx)
        .await
        .map_err(db_error)?;
        tx.commit().await.map_err(db_error)?;

        let horizon = Horizon {
            xmin: parse_xid(&xmin)?,
            xmax: parse_xid(&xmax)?,
            max_pk,
        };
        let entries = rows
            .into_iter()
            .map(|row| {
                self.delivered.insert(row.pk_entity_change_log);
                ChangeLogEntry::from(row)
            })
            .collect();
        self.advance(&horizon, Instant::now());
        Ok(entries)
    }

    /// Record this poll's floor candidate and apply every candidate the horizon has
    /// passed. Pure bookkeeping, so the rule is testable without a database.
    fn advance(&mut self, horizon: &Horizon, now: Instant) {
        if let Some(max_pk) = horizon.max_pk.filter(|&pk| pk > self.floor) {
            let candidate = FloorCandidate {
                xmax: horizon.xmax,
                max_pk,
                at: now,
            };
            match self.pending.back() {
                // Nothing new is visible: the older candidate already covers this pk.
                Some(last) if last.max_pk >= max_pk => {},
                Some(_) if self.pending.len() >= MAX_PENDING => {
                    if let Some(last) = self.pending.back_mut() {
                        *last = candidate;
                    }
                },
                _ => self.pending.push_back(candidate),
            }
        }

        while let Some(front) = self.pending.front() {
            if front.xmax > horizon.xmin || now.duration_since(front.at) < self.settle {
                break;
            }
            self.floor = self.floor.max(front.max_pk);
            self.pending.pop_front();
        }
        self.delivered = self.delivered.split_off(&(self.floor.saturating_add(1)));

        if self.delivered.len() > DELIVERED_WARN && !self.warned {
            self.warned = true;
            warn!(
                delivered = self.delivered.len(),
                "The change-log tail is holding many delivered rows above its floor: a \
                 long-running transaction is keeping the transaction horizon back."
            );
        } else if self.delivered.len() <= DELIVERED_WARN {
            self.warned = false;
        }
    }
}

async fn snapshot(pool: &PgPool) -> Result<Transaction<'static, Postgres>> {
    let mut tx = pool.begin().await.map_err(db_error)?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY")
        .execute(&mut *tx)
        .await
        .map_err(db_error)?;
    Ok(tx)
}

fn parse_xid(text: &str) -> Result<u64> {
    text.parse().map_err(|e| ObserverError::DatabaseError {
        reason: format!("unreadable transaction id {text:?}: {e}"),
    })
}

#[allow(clippy::needless_pass_by_value)] // Reason: used as a map_err adapter
fn db_error(e: sqlx::Error) -> ObserverError {
    ObserverError::DatabaseError {
        reason: format!("Failed to read the change log for fan-out: {e}"),
    }
}

#[cfg(test)]
mod tests;
