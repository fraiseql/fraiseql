//! The scheduling envelope for a [`PullSource`] — the generic orchestration that
//! turns a native pull source into a rock-solid, single-firing, at-least-once
//! ingress on a schedule (#573).
//!
//! One tick ([`run_source_once`]) does, under the source's advisory lease so it
//! runs on exactly one replica:
//!
//! 1. load the durable cursor,
//! 2. [`poll`](PullSource::poll) the source for everything new since it,
//! 3. hand a non-empty batch to the [`IngestSink`], which emits it onto the durable spine and
//!    advances the cursor **in one transaction** (atomic — no reprocess window) and dispatches
//!    `after:ingest` once committed.
//!
//! ## Layering — why the sink is a seam
//!
//! This envelope owns *scheduling* (the lease, the cursor read, the poll), all of
//! which this crate can express with no database driver. The *transactional* part —
//! the spine write, the cursor advance, and the `after:ingest` dispatch — lives in
//! the server (that is where the spine is) and is reached through [`IngestSink`],
//! so the envelope stays driver-free and unit-testable with a stub sink. Keeping
//! the transaction inside the sink (rather than owning it here) is what lets this
//! crate avoid a hard `sqlx` dependency.

use fraiseql_error::{FraiseQLError, Result};
use fraiseql_observers::{
    CursorSnapshot, LeaseGuardedRunner, ObserverError, RunOutcome, SourceCursorStore,
};

use super::ingest::{PullBatch, PullContext, PullSource};

#[cfg(test)]
mod tests;

/// The durable-ingest seam the envelope drives.
///
/// One call must, **atomically**: emit the batch's messages onto the durable spine
/// (deduplicated by idempotency key) and advance the cursor from `from` to the
/// batch's `next_cursor`, in a single transaction; then, once committed, dispatch
/// `after:ingest` for the newly-persisted messages (durable, at-least-once).
///
/// Implemented by the server, where the spine and the `after:ingest` dispatcher
/// live. Splitting it out keeps this crate free of the database driver and lets the
/// envelope be tested with an in-memory stub.
#[allow(async_fn_in_trait)] // Reason: concrete-type use by the envelope; no dyn dispatch.
pub trait IngestSink {
    /// Transactionally emit `batch` onto the spine and advance the cursor from
    /// `from`, then dispatch `after:ingest` post-commit.
    ///
    /// Returns `true` when the batch committed and the cursor advanced, `false`
    /// when the cursor advance lost a lease-boundary race (another replica had
    /// already moved it on) — in which case the transaction is rolled back and
    /// nothing is ingested or dispatched.
    ///
    /// # Errors
    ///
    /// Returns a [`FraiseQLError`] if the spine write, the cursor advance, or the
    /// commit fails; the caller then leaves the watermark unmoved for a retry.
    async fn ingest(
        &self,
        source_name: &str,
        batch: PullBatch,
        from: &CursorSnapshot,
    ) -> Result<bool>;
}

/// The outcome of one [`run_source_once`] tick.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceOutcome {
    /// Another replica held the lease; this tick did not poll.
    SkippedNotLeader,
    /// Polled, but the source returned nothing new and left the cursor where it
    /// was; nothing to do.
    NoData,
    /// Advanced the cursor and ingested `messages` messages. `messages` is `0` when
    /// the source advanced the watermark without emitting — e.g. it skipped a poison
    /// message and moved past it.
    Ingested {
        /// How many messages the poll returned (before spine dedup); may be `0`.
        messages: usize,
    },
    /// The cursor advance was rejected — another replica had already moved it on
    /// (a lease-boundary race). The transaction rolled back; nothing ingested.
    CursorRaceLost,
}

/// Run one tick of a native [`PullSource`] under its single-firing lease.
///
/// The cursor and lease are keyed on [`runner.source_name()`](LeaseGuardedRunner::source_name)
/// — the unique per-instance name (e.g. the mailbox name), **not** the source's
/// [`IngestSource`](super::ingest::IngestSource) routing discriminant, so two
/// mailboxes of the same kind get distinct cursors and never coordinate on one
/// lease. `runner`, `store`, and the sink's own spine/cursor handles must all
/// target the same PostgreSQL database.
///
/// # Errors
///
/// Returns a [`FraiseQLError`] if acquiring the lease, loading the cursor, polling
/// the source, or ingesting the batch fails. On any error nothing commits, so the
/// cursor stays put and the next tick retries the same window (at-least-once).
// Reason: the future is Send for concrete source/store/sink types (all do
// Send sqlx/IMAP I/O); the source scheduler spawns those concrete instantiations,
// where rustc enforces Send. clippy cannot prove it for the fully generic form.
#[allow(clippy::future_not_send)]
pub async fn run_source_once<S, P, K>(
    runner: &LeaseGuardedRunner,
    store: &S,
    source: &P,
    sink: &K,
) -> Result<SourceOutcome>
where
    S: SourceCursorStore,
    P: PullSource,
    K: IngestSink,
{
    let name = runner.source_name();

    let outcome = runner
        .run(|| ingest_tick(store, source, sink, name))
        .await
        .map_err(|e| obs_err(&e))?;

    match outcome {
        RunOutcome::SkippedNotLeader => Ok(SourceOutcome::SkippedNotLeader),
        RunOutcome::Ran(result) => result,
    }
}

/// The body of one tick, run under the lease: load the cursor, poll, and hand a
/// non-empty batch to the sink to ingest transactionally.
#[allow(clippy::future_not_send)] // Reason: see `run_source_once`.
async fn ingest_tick<S, P, K>(store: &S, source: &P, sink: &K, name: &str) -> Result<SourceOutcome>
where
    S: SourceCursorStore,
    P: PullSource,
    K: IngestSink,
{
    let cursor = store.load(name).await.map_err(|e| obs_err(&e))?;

    let batch = source
        .poll(&PullContext {
            cursor: cursor.value.clone(),
        })
        .await
        .map_err(|error| {
            FraiseQLError::internal(format!("source '{name}' poll failed: {error}"))
        })?;

    // Advance the cursor whenever the source made progress — either it fetched
    // messages, or it moved the watermark past input it consumed without emitting
    // (e.g. a source that skips a poison message must still advance past it, or the
    // next poll re-fetches it forever). A poll that returns nothing *and* leaves the
    // cursor where it was is genuinely idle.
    let current = cursor.value.clone().unwrap_or(serde_json::Value::Null);
    let progressed = batch.next_cursor != current;
    if batch.messages.is_empty() && !progressed {
        return Ok(SourceOutcome::NoData);
    }
    let message_count = batch.messages.len();

    if sink.ingest(name, batch, &cursor).await? {
        Ok(SourceOutcome::Ingested {
            messages: message_count,
        })
    } else {
        Ok(SourceOutcome::CursorRaceLost)
    }
}

/// Map an observer-coordination error onto the canonical framework error.
fn obs_err(error: &ObserverError) -> FraiseQLError {
    FraiseQLError::database(format!("source coordination: {error}"))
}
