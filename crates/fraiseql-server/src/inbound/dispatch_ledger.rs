//! The `after:ingest` dispatch ledger: what makes inbound dispatch at-least-once (#1175).
//!
//! A message is persisted onto the spine inside the receiver's transaction. In the same
//! transaction, [`record_pending_in_tx`] writes one `pending` row per `after:ingest`
//! function the message triggers into `_fraiseql_inbound_dispatch`, leased to the receiving
//! process for the dispatch it is about to run. When that dispatch finishes, [`settle`]
//! marks the row terminal: `dispatched` on success, `dead_lettered` when it exhausted its
//! retries and was dead-lettered.
//!
//! A process that dies after the commit and before settling leaves the row `pending`, and
//! its lease runs out. The server's sweep ([`run_sweeper`]: once at startup, then on an
//! interval) claims due rows with `FOR UPDATE SKIP LOCKED`, renews their lease, and
//! dispatches them again. So with the ledger `after:ingest` dispatch is at-least-once: one
//! can run twice (a lease that expires while the first run is still in flight), never zero
//! times.
//! Handlers must therefore be idempotent; each dispatch hands them a token derived from the
//! function and the message, which is the same on every attempt.
//!
//! A provider's redelivery of a committed message is still answered `duplicate`: recovering
//! its dispatch is the sweep's job, not the redelivery's.

use std::{future::Future, pin::Pin, time::Duration};

use fraiseql_functions::InboundMessage;
use sqlx::{PgPool, Postgres, Row, Transaction};

/// How the ledger leases, sweeps and batches. See `docs/architecture/webhooks.md` for the
/// operator's view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LedgerSettings {
    /// How long a dispatch may run before the sweep takes it over. Longer than a
    /// dispatch's own retries normally take, or a slow dispatch runs twice.
    pub lease:          Duration,
    /// How often the sweep looks for dispatches whose lease ran out.
    pub sweep_interval: Duration,
    /// How many due dispatches one sweep claims.
    pub batch_size:     u32,
}

impl Default for LedgerSettings {
    fn default() -> Self {
        Self {
            lease:          Duration::from_mins(5),
            sweep_interval: Duration::from_secs(30),
            batch_size:     100,
        }
    }
}

/// How a dispatch ended, for its ledger row: `Dispatched` → `dispatched`, `DeadLettered` →
/// `dead_lettered`, `Unsettled` (it could not even be dead-lettered) → left `pending`, so the
/// sweep dispatches it again when the lease runs out.
pub use crate::routes::after_mutation::DispatchOutcome;

fn db_err(context: &str, error: &sqlx::Error) -> fraiseql_error::FraiseQLError {
    fraiseql_error::FraiseQLError::database(format!("inbound dispatch ledger: {context}: {error}"))
}

const fn secs(duration: Duration) -> f64 {
    duration.as_secs_f64()
}

/// Record a `pending` dispatch of each of `functions` for the spine message `message_id`,
/// in the receiver's transaction, leased for `lease` to the dispatch about to run.
///
/// # Errors
///
/// `FraiseQLError::Database` when the insert fails (the receiver's transaction then rolls
/// back with the spine row: neither is recorded).
pub async fn record_pending_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    message_id: uuid::Uuid,
    functions: &[String],
    lease: Duration,
) -> fraiseql_error::Result<()> {
    if functions.is_empty() {
        return Ok(());
    }
    sqlx::query(
        "INSERT INTO _fraiseql_inbound_dispatch (fk_inbound_message, function_name, lease_until) \
         SELECT m.pk_inbound_message, f, now() + make_interval(secs => $3) \
         FROM _fraiseql_inbound_message m CROSS JOIN unnest($2::text[]) AS f \
         WHERE m.id = $1 \
         ON CONFLICT DO NOTHING",
    )
    .bind(message_id)
    .bind(functions)
    .bind(secs(lease))
    .execute(&mut **tx)
    .await
    .map_err(|error| db_err("record", &error))?;
    Ok(())
}

/// Mark the dispatch of `function_name` for `message_id` terminal, per `outcome`. A row
/// already terminal is left as it is; [`DispatchOutcome::Unsettled`] changes nothing.
///
/// # Errors
///
/// `FraiseQLError::Database` when the update fails.
pub async fn settle(
    pool: &PgPool,
    message_id: uuid::Uuid,
    function_name: &str,
    outcome: DispatchOutcome,
) -> fraiseql_error::Result<()> {
    let state = match outcome {
        DispatchOutcome::Dispatched => "dispatched",
        DispatchOutcome::DeadLettered => "dead_lettered",
        DispatchOutcome::Unsettled => return Ok(()),
    };
    sqlx::query(
        "UPDATE _fraiseql_inbound_dispatch d SET state = $3, settled_at = now() \
         FROM _fraiseql_inbound_message m \
         WHERE d.fk_inbound_message = m.pk_inbound_message AND m.id = $1 \
           AND d.function_name = $2 AND d.state = 'pending'",
    )
    .bind(message_id)
    .bind(function_name)
    .bind(state)
    .execute(pool)
    .await
    .map_err(|error| db_err("settle", &error))?;
    Ok(())
}

/// A dispatch the sweep claimed: its message, re-read from the spine, and its function.
#[derive(Debug, Clone)]
pub struct DueDispatch {
    /// The spine message's id.
    pub message_id:    uuid::Uuid,
    /// The `after:ingest` function to dispatch it to.
    pub function_name: String,
    /// The persisted message.
    pub message:       InboundMessage,
    /// How many times it has now been dispatched, this one included.
    pub attempts:      i32,
}

/// Claim up to `batch` `pending` dispatches whose lease has run out, renewing it for `lease`.
///
/// Concurrent sweepers skip each other's rows (`FOR UPDATE SKIP LOCKED`), and a renewed lease
/// keeps the next sweep off a row until it runs out again.
///
/// # Errors
///
/// `FraiseQLError::Database` when the claim fails or a persisted message cannot be read.
pub async fn claim_due(
    pool: &PgPool,
    batch: u32,
    lease: Duration,
) -> fraiseql_error::Result<Vec<DueDispatch>> {
    let rows = sqlx::query(
        "WITH due AS ( \
             SELECT fk_inbound_message, function_name FROM _fraiseql_inbound_dispatch \
             WHERE state = 'pending' AND lease_until <= now() \
             ORDER BY lease_until LIMIT $1 \
             FOR UPDATE SKIP LOCKED) \
         UPDATE _fraiseql_inbound_dispatch d \
         SET lease_until = now() + make_interval(secs => $2), attempts = d.attempts + 1 \
         FROM due, _fraiseql_inbound_message m \
         WHERE d.fk_inbound_message = due.fk_inbound_message \
           AND d.function_name = due.function_name \
           AND m.pk_inbound_message = d.fk_inbound_message \
         RETURNING m.id, d.function_name, m.payload, d.attempts",
    )
    .bind(i64::from(batch))
    .bind(secs(lease))
    .fetch_all(pool)
    .await
    .map_err(|error| db_err("claim", &error))?;

    rows.iter()
        .map(|row| {
            let payload: serde_json::Value = row.get("payload");
            Ok(DueDispatch {
                message_id:    row.get("id"),
                function_name: row.get("function_name"),
                message:       serde_json::from_value(payload).map_err(|error| {
                    fraiseql_error::FraiseQLError::database(format!(
                        "inbound dispatch ledger: read message: {error}"
                    ))
                })?,
                attempts:      row.get("attempts"),
            })
        })
        .collect()
}

/// Runs one `after:ingest` function on one message, to completion, and says how it ended.
pub trait IngestDispatcher: Send + Sync {
    /// Dispatch `message` to `function_name`.
    fn dispatch<'a>(
        &'a self,
        message: &'a InboundMessage,
        function_name: &'a str,
    ) -> Pin<Box<dyn Future<Output = DispatchOutcome> + Send + 'a>>;
}

/// Claim the due dispatches, run each, and settle it. Returns how many were claimed.
///
/// # Errors
///
/// `FraiseQLError::Database` when the claim fails; a failed settle is logged (the row stays
/// `pending` and is swept again).
pub async fn sweep_once(
    pool: &PgPool,
    dispatcher: &dyn IngestDispatcher,
    settings: LedgerSettings,
) -> fraiseql_error::Result<usize> {
    let due = claim_due(pool, settings.batch_size, settings.lease).await?;
    let claimed = due.len();
    futures::future::join_all(due.iter().map(|dispatch| async move {
        tracing::info!(
            message_id = %dispatch.message_id,
            function = %dispatch.function_name,
            attempts = dispatch.attempts,
            "after:ingest dispatch replayed: its lease ran out before it settled"
        );
        let function = &dispatch.function_name;
        let outcome = dispatcher.dispatch(&dispatch.message, function).await;
        if let Err(error) = settle(pool, dispatch.message_id, function, outcome).await {
            tracing::warn!(
                %error,
                "after:ingest dispatch ran but could not be settled; it will run again"
            );
        }
    }))
    .await;
    Ok(claimed)
}

/// Sweep once now (the startup scan), then every `settings.sweep_interval`, forever.
///
/// Run on the server's task set, so graceful shutdown stops it; a dispatch it leaves
/// unsettled is swept again after the next start.
pub async fn run_sweeper(
    pool: PgPool,
    dispatcher: std::sync::Arc<dyn IngestDispatcher>,
    settings: LedgerSettings,
) {
    loop {
        if let Err(error) = sweep_once(&pool, dispatcher.as_ref(), settings).await {
            tracing::warn!(%error, "after:ingest dispatch sweep failed; retrying next interval");
        }
        tokio::time::sleep(settings.sweep_interval).await;
    }
}

#[cfg(test)]
mod tests;
