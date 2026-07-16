//! Postgres-backed function-dispatch dead-letter store (#598).
//!
//! The durable counterpart to `InMemoryDlq` for
//! **function-trigger dispatch** failures. Where the in-memory store loses every
//! dead-lettered dispatch on restart, this store persists them to
//! `_fraiseql_function_dlq` (DDL:
//! [`dlq_migration_sql`](fraiseql_functions::migrations::dlq_migration_sql)) so an
//! operator can still list and replay a failed money/mail dispatch after a process
//! restart. Selected via `[functions] dlq_store = "postgres"` (memory is the dev
//! default); a Postgres pool must be available.
//!
//! **Scope: function dispatch only.** This store backs `hooks.dlq`, which the
//! `DurableDispatcher` uses solely through
//! [`push_function`](fraiseql_observers::DeadLetterQueue::push_function) /
//! [`get_pending_functions`](fraiseql_observers::DeadLetterQueue::get_pending_functions).
//! The observer-action DLQ (`EntityEvent` + `ActionConfig`) is a separate concern
//! kept in the observer runtime's own `InMemoryDlq`;
//! the observer-action trait methods here are deliberately un-backed (they warn and
//! no-op) because this store is never wired as the observer DLQ. If observer-action
//! durability is wanted later, it needs its own table — it is not silently folded in
//! here.

use fraiseql_observers::{
    ActionConfig, DeadLetterQueue, DispatchSource, DlqItem, EntityEvent, FunctionDispatchRecord,
    ObserverError, Result,
};
use sqlx::PgPool;
use uuid::Uuid;

/// A Postgres-backed dead-letter store for function-trigger dispatch failures.
pub struct PgFunctionDlq {
    pool:     PgPool,
    /// Retention cap (drop-newest at capacity), mirroring the in-memory store so
    /// `FRAISEQL_FUNCTIONS_DLQ_MAX_SIZE` means the same thing for both. `None` =
    /// unbounded.
    max_size: Option<usize>,
}

/// Map a driver error to the observer DLQ error, without leaking the query text.
fn dlq_err(op: &str, error: &sqlx::Error) -> ObserverError {
    ObserverError::DlqError {
        reason: format!("function DLQ {op}: {error}"),
    }
}

impl PgFunctionDlq {
    /// Wrap a pool with a retention cap (`None` = unbounded).
    #[must_use]
    pub const fn new(pool: PgPool, max_size: Option<usize>) -> Self {
        Self { pool, max_size }
    }

    /// Create the `_fraiseql_function_dlq` table (idempotent DDL).
    ///
    /// Call once on startup. Safe to re-run — the DDL uses `CREATE … IF NOT EXISTS`.
    ///
    /// # Errors
    ///
    /// Returns the driver error if the DDL cannot be applied.
    pub async fn init(&self) -> std::result::Result<(), sqlx::Error> {
        sqlx::raw_sql(fraiseql_functions::migrations::dlq_migration_sql())
            .execute(&self.pool)
            .await
            .map(|_| ())
    }

    /// Current stored record count (for the cap check and the `/metrics` gauge).
    async fn count(&self) -> Result<i64> {
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM _fraiseql_function_dlq")
            .fetch_one(&self.pool)
            .await
            .map_err(|error| dlq_err("count", &error))
    }
}

#[async_trait::async_trait]
impl DeadLetterQueue for PgFunctionDlq {
    async fn push_function(&self, record: FunctionDispatchRecord) -> Result<Uuid> {
        let id = record.id;

        // Drop-newest at capacity — same policy and metrics as the in-memory store,
        // so `max_dlq_size` behaves identically whichever store is selected.
        if let Some(max) = self.max_size {
            // usize→i64 is safe for any realistic cap; saturate defensively.
            let cap = i64::try_from(max).unwrap_or(i64::MAX);
            let current = self.count().await?;
            if current >= cap {
                tracing::warn!(
                    max_dlq_size = max,
                    function = %record.function_name,
                    trigger = %record.trigger_type,
                    "function DLQ full; dropping failed function dispatch entry"
                );
                crate::function_metrics::record_dlq_eviction();
                crate::function_metrics::set_dlq_size(
                    usize::try_from(current).unwrap_or(usize::MAX),
                );
                return Ok(id);
            }
        }

        // The payload is bound as text and cast `::jsonb` (the inbound-spine idiom),
        // so this store needs no sqlx `json` binding feature.
        let payload = record.payload.to_string();
        sqlx::query(
            "INSERT INTO _fraiseql_function_dlq \
                 (id, source, function_name, trigger_type, idempotency_token, \
                  payload, error_message, attempts) \
             VALUES ($1, $2, $3, $4, $5, $6::jsonb, $7, $8)",
        )
        .bind(id)
        .bind(record.source.label())
        .bind(&record.function_name)
        .bind(&record.trigger_type)
        .bind(&record.idempotency_token)
        .bind(payload)
        .bind(&record.error_message)
        .bind(i64::from(record.attempts))
        .execute(&self.pool)
        .await
        .map_err(|error| dlq_err("push", &error))?;

        // Reflect the new depth on `/metrics` (this replica's view).
        let current = self.count().await?;
        crate::function_metrics::set_dlq_size(usize::try_from(current).unwrap_or(usize::MAX));
        Ok(id)
    }

    async fn get_pending_functions(&self, limit: i64) -> Result<Vec<FunctionDispatchRecord>> {
        // `payload::text` reads the JSONB back as a string we parse — no sqlx `json`
        // decode feature needed. Oldest-first so a drain replays in arrival order.
        let rows =
            sqlx::query_as::<_, (Uuid, String, String, String, String, String, String, i64)>(
                "SELECT id, source, function_name, trigger_type, idempotency_token, \
                    payload::text, error_message, attempts \
             FROM _fraiseql_function_dlq \
             ORDER BY created_at ASC, pk_function_dlq ASC \
             LIMIT $1",
            )
            .bind(limit)
            .fetch_all(&self.pool)
            .await
            .map_err(|error| dlq_err("list", &error))?;

        Ok(rows
            .into_iter()
            .map(
                |(
                    id,
                    source,
                    function_name,
                    trigger_type,
                    idempotency_token,
                    payload_text,
                    error_message,
                    attempts,
                )| {
                    let source = DispatchSource::from_label(&source).unwrap_or_else(|| {
                        // A row this server did not write (newer server, or corruption):
                        // surface it in the neutral background bucket rather than guess.
                        tracing::warn!(%source, "function DLQ row has an unknown source label");
                        DispatchSource::Source
                    });
                    let payload = serde_json::from_str(&payload_text).unwrap_or_else(|error| {
                        tracing::warn!(%error, "function DLQ row payload is not valid JSON");
                        serde_json::Value::Null
                    });
                    FunctionDispatchRecord {
                        id,
                        source,
                        function_name,
                        trigger_type,
                        idempotency_token,
                        payload,
                        error_message,
                        // Stored as BIGINT; a real attempt count never exceeds u32.
                        attempts: u32::try_from(attempts).unwrap_or(u32::MAX),
                    }
                },
            )
            .collect())
    }

    // ── Observer-action DLQ: un-backed here (see the module docs). ───────────────
    //
    // This store is function-dispatch only. The observer runtime keeps its own
    // `InMemoryDlq` for `EntityEvent` + `ActionConfig` failures, so these methods
    // are never reached in the wired path; they warn and no-op rather than pretend
    // to persist an observer action they have no table for.

    async fn push(&self, event: EntityEvent, action: ActionConfig, error: String) -> Result<Uuid> {
        let id = Uuid::new_v4();
        tracing::warn!(
            action_type = action.action_type(),
            event_id = %event.id,
            %error,
            "PgFunctionDlq is function-dispatch only — observer-action failure not persisted"
        );
        Ok(id)
    }

    async fn get_pending(&self, _limit: i64) -> Result<Vec<DlqItem>> {
        Ok(Vec::new())
    }

    async fn mark_success(&self, _id: Uuid) -> Result<()> {
        Ok(())
    }

    async fn mark_retry_failed(&self, _id: Uuid, _error: &str) -> Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests;
