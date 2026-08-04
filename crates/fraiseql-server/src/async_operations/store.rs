//! Durable async-operation state (`_system.async_operations`, #391).
//!
//! The store IS the state machine: every transition is an atomic conditional
//! `UPDATE … WHERE state = <legal source>` (the P19 rule — enforcing
//! transitions in application code let a replay re-run completed saga steps),
//! and every completion is guarded by the claim token of the attempt that did
//! the work, so a worker presumed dead that finishes anyway cannot clobber the
//! outcome of the retry that superseded it.

use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::{Row, postgres::PgPool};
use uuid::Uuid;

use crate::{Result, ServerError};

/// One durable operation row.
#[derive(Debug, Clone)]
pub struct AsyncOperation {
    /// Operation id — the submit response's handle and the idempotent identity.
    pub op_id:                  Uuid,
    /// Resolved tenant key at submission (`None` single-tenant); execution
    /// dispatches to THIS tenant's executor (P19's wrong-database replay mode).
    pub tenant_key:             Option<String>,
    /// The submitting principal (`SecurityContext.user_id`); status/cancel are
    /// scoped to it.
    pub submitter:              String,
    /// Root operation name (allowlist member).
    pub operation:              String,
    /// The GraphQL document re-issued at execution — the same pipeline as
    /// `/graphql`, never a second execution path.
    pub document:               String,
    /// The document's variables.
    pub variables:              Option<Value>,
    /// The submitter's serialized `SecurityContext` snapshot.
    pub security_context:       Value,
    /// `queued` / `running` / `succeeded` / `failed` / `cancelled`.
    pub state:                  String,
    /// Cancellation was requested while the operation was not cancellable
    /// outright; the worker honours it at the next safe point.
    pub cancellation_requested: bool,
    /// Execution attempts so far.
    pub attempts:               i32,
    /// The stored GraphQL response envelope, for `succeeded` (and `failed`
    /// executions that produced a partial envelope).
    pub result:                 Option<Value>,
    /// Failure detail, for `failed`.
    pub error:                  Option<String>,
    /// Submission time.
    pub created_at:             DateTime<Utc>,
    /// First claim of the latest attempt.
    pub started_at:             Option<DateTime<Utc>>,
    /// Terminal-state time.
    pub finished_at:            Option<DateTime<Utc>>,
}

fn row_to_op(row: &sqlx::postgres::PgRow) -> AsyncOperation {
    AsyncOperation {
        op_id:                  row.get("op_id"),
        tenant_key:             row.get("tenant_key"),
        submitter:              row.get("submitter"),
        operation:              row.get("operation"),
        document:               row.get("document"),
        variables:              row.get("variables"),
        security_context:       row.get("security_context"),
        state:                  row.get("state"),
        cancellation_requested: row.get("cancellation_requested"),
        attempts:               row.get("attempts"),
        result:                 row.get("result"),
        error:                  row.get("error"),
        created_at:             row.get("created_at"),
        started_at:             row.get("started_at"),
        finished_at:            row.get("finished_at"),
    }
}

fn db_err(what: &str, e: &sqlx::Error) -> ServerError {
    ServerError::Database(format!("async-operations: failed to {what}: {e}"))
}

/// A successful claim: the operation plus the token that guards its completion.
#[derive(Debug)]
pub struct ClaimedOperation {
    /// The claimed row.
    pub op:          AsyncOperation,
    /// This attempt's completion guard — `complete`/`fail`/`cancel_unstarted`
    /// only apply while the row still carries it.
    pub claim_token: Uuid,
}

/// PostgreSQL-backed operation store.
#[derive(Debug, Clone)]
pub struct AsyncOperationStore {
    db: PgPool,
}

impl AsyncOperationStore {
    /// Create a store over an existing pool. Call [`init`](Self::init) at boot.
    #[must_use]
    pub const fn new(db: PgPool) -> Self {
        Self { db }
    }

    /// Create `_system.async_operations`. Idempotent; a failure aborts the boot
    /// (a configured surface whose table is unreachable must not half-mount).
    ///
    /// # Errors
    ///
    /// Returns [`ServerError::Database`] when the DDL cannot be applied.
    pub async fn init(&self) -> Result<()> {
        sqlx::raw_sql(
            r"
            CREATE SCHEMA IF NOT EXISTS _system;

            CREATE TABLE IF NOT EXISTS _system.async_operations (
                op_id                  UUID PRIMARY KEY DEFAULT gen_random_uuid(),
                tenant_key             TEXT,
                submitter              TEXT        NOT NULL,
                operation              TEXT        NOT NULL,
                document               TEXT        NOT NULL,
                variables              JSONB,
                security_context       JSONB       NOT NULL,
                state                  TEXT        NOT NULL DEFAULT 'queued'
                    CHECK (state IN ('queued','running','succeeded','failed','cancelled')),
                cancellation_requested BOOLEAN     NOT NULL DEFAULT false,
                claim_token            UUID,
                attempts               INTEGER     NOT NULL DEFAULT 0,
                max_attempts           INTEGER     NOT NULL,
                result                 JSONB,
                error                  TEXT,
                created_at             TIMESTAMPTZ NOT NULL DEFAULT now(),
                heartbeat_at           TIMESTAMPTZ NOT NULL DEFAULT now(),
                started_at             TIMESTAMPTZ,
                finished_at            TIMESTAMPTZ
            );

            CREATE INDEX IF NOT EXISTS idx_async_operations_claim
                ON _system.async_operations (state, heartbeat_at);
            ",
        )
        .execute(&self.db)
        .await
        .map_err(|e| db_err("initialize _system.async_operations", &e))?;
        Ok(())
    }

    /// Persist a new `queued` operation and return its id.
    ///
    /// # Errors
    ///
    /// Returns [`ServerError::Database`] on a storage failure.
    #[allow(clippy::too_many_arguments)] // Reason: submission is one write of one row; a builder would obscure that
    pub async fn submit(
        &self,
        tenant_key: Option<&str>,
        submitter: &str,
        operation: &str,
        document: &str,
        variables: Option<&Value>,
        security_context: &Value,
        max_attempts: u32,
    ) -> Result<Uuid> {
        let row = sqlx::query(
            "INSERT INTO _system.async_operations
               (tenant_key, submitter, operation, document, variables, security_context,
                max_attempts)
             VALUES ($1, $2, $3, $4, $5, $6, $7)
             RETURNING op_id",
        )
        .bind(tenant_key)
        .bind(submitter)
        .bind(operation)
        .bind(document)
        .bind(variables)
        .bind(security_context)
        .bind(i32::try_from(max_attempts).unwrap_or(1))
        .fetch_one(&self.db)
        .await
        .map_err(|e| db_err("submit operation", &e))?;
        Ok(row.get("op_id"))
    }

    /// Fetch one operation **scoped to its submitter** (and tenant): another
    /// principal's id — even a guessed valid UUID — reads as absent, the same
    /// non-oracle shape the MCP allowlist uses.
    ///
    /// # Errors
    ///
    /// Returns [`ServerError::Database`] on a storage failure.
    pub async fn get_scoped(
        &self,
        op_id: Uuid,
        submitter: &str,
        tenant_key: Option<&str>,
    ) -> Result<Option<AsyncOperation>> {
        let row = sqlx::query(
            "SELECT * FROM _system.async_operations
             WHERE op_id = $1 AND submitter = $2 AND tenant_key IS NOT DISTINCT FROM $3",
        )
        .bind(op_id)
        .bind(submitter)
        .bind(tenant_key)
        .fetch_optional(&self.db)
        .await
        .map_err(|e| db_err("get operation", &e))?;
        Ok(row.as_ref().map(row_to_op))
    }

    /// Claim up to `limit` executable operations for this worker.
    ///
    /// Claimable: `queued` without a pending cancellation, or `running` whose
    /// heartbeat is older than `stale_secs` (the worker died — P19: "stuck"
    /// means STALE, never merely claimed). Terminal states are never claimable
    /// — recovery cannot re-execute completed work. `FOR UPDATE SKIP LOCKED`
    /// keeps concurrent workers from double-claiming.
    ///
    /// # Errors
    ///
    /// Returns [`ServerError::Database`] on a storage failure.
    #[allow(clippy::cast_precision_loss)] // Reason: seconds from config; far below f64's 52-bit mantissa
    pub async fn claim(&self, limit: i64, stale_secs: u64) -> Result<Vec<ClaimedOperation>> {
        let claim_token = Uuid::new_v4();
        let rows = sqlx::query(
            "UPDATE _system.async_operations o
             SET state = 'running', claim_token = $1, attempts = o.attempts + 1,
                 started_at = now(), heartbeat_at = now()
             WHERE o.op_id IN (
               SELECT op_id FROM _system.async_operations
               WHERE (state = 'queued' AND NOT cancellation_requested)
                  OR (state = 'running' AND heartbeat_at < now() - make_interval(secs => $2))
               ORDER BY created_at
               LIMIT $3
               FOR UPDATE SKIP LOCKED
             )
             RETURNING *",
        )
        .bind(claim_token)
        .bind(stale_secs as f64)
        .bind(limit)
        .fetch_all(&self.db)
        .await
        .map_err(|e| db_err("claim operations", &e))?;
        Ok(rows
            .iter()
            .map(|r| ClaimedOperation {
                op: row_to_op(r),
                claim_token,
            })
            .collect())
    }

    /// Refresh the claim heartbeat while the execution runs. A `false` return
    /// means the claim was lost (superseded after a stall) — the caller must
    /// stop treating the operation as its own.
    ///
    /// # Errors
    ///
    /// Returns [`ServerError::Database`] on a storage failure.
    pub async fn heartbeat(&self, op_id: Uuid, claim_token: Uuid) -> Result<bool> {
        let result = sqlx::query(
            "UPDATE _system.async_operations
             SET heartbeat_at = now()
             WHERE op_id = $1 AND claim_token = $2 AND state = 'running'",
        )
        .bind(op_id)
        .bind(claim_token)
        .execute(&self.db)
        .await
        .map_err(|e| db_err("heartbeat", &e))?;
        Ok(result.rows_affected() == 1)
    }

    /// Record a successful execution. Claim-guarded: a superseded worker's late
    /// completion is a no-op (`false`), never a clobber.
    ///
    /// # Errors
    ///
    /// Returns [`ServerError::Database`] on a storage failure.
    pub async fn complete(&self, op_id: Uuid, claim_token: Uuid, result: &Value) -> Result<bool> {
        let done = sqlx::query(
            "UPDATE _system.async_operations
             SET state = 'succeeded', result = $3, finished_at = now(), claim_token = NULL
             WHERE op_id = $1 AND claim_token = $2 AND state = 'running'",
        )
        .bind(op_id)
        .bind(claim_token)
        .bind(result)
        .execute(&self.db)
        .await
        .map_err(|e| db_err("complete operation", &e))?;
        Ok(done.rows_affected() == 1)
    }

    /// Record a failed attempt. Below `max_attempts` the operation re-queues;
    /// at the ceiling it is terminally `failed` with the error recorded (never
    /// silently dropped — P19's discarded-result mode). Claim-guarded like
    /// [`complete`](Self::complete).
    ///
    /// # Errors
    ///
    /// Returns [`ServerError::Database`] on a storage failure.
    pub async fn fail(
        &self,
        op_id: Uuid,
        claim_token: Uuid,
        error: &str,
        partial_result: Option<&Value>,
    ) -> Result<bool> {
        let done = sqlx::query(
            "UPDATE _system.async_operations
             SET state = CASE WHEN attempts >= max_attempts THEN 'failed' ELSE 'queued' END,
                 error = $3,
                 result = $4,
                 finished_at = CASE WHEN attempts >= max_attempts THEN now() ELSE NULL END,
                 claim_token = NULL,
                 heartbeat_at = now()
             WHERE op_id = $1 AND claim_token = $2 AND state = 'running'",
        )
        .bind(op_id)
        .bind(claim_token)
        .bind(error)
        .bind(partial_result)
        .execute(&self.db)
        .await
        .map_err(|e| db_err("fail operation", &e))?;
        Ok(done.rows_affected() == 1)
    }

    /// Cancel a still-`queued` operation outright (scoped to its submitter).
    /// `true` means it WAS cancelled; `false` means it was no longer `queued`
    /// (running or terminal) — the caller must not report a cancellation that
    /// did not happen (P19 `#746`).
    ///
    /// # Errors
    ///
    /// Returns [`ServerError::Database`] on a storage failure.
    pub async fn cancel_queued(
        &self,
        op_id: Uuid,
        submitter: &str,
        tenant_key: Option<&str>,
    ) -> Result<bool> {
        let done = sqlx::query(
            "UPDATE _system.async_operations
             SET state = 'cancelled', finished_at = now()
             WHERE op_id = $1 AND submitter = $2 AND tenant_key IS NOT DISTINCT FROM $3
               AND state = 'queued'",
        )
        .bind(op_id)
        .bind(submitter)
        .bind(tenant_key)
        .execute(&self.db)
        .await
        .map_err(|e| db_err("cancel queued operation", &e))?;
        Ok(done.rows_affected() == 1)
    }

    /// Request cancellation of a `running` operation (scoped to its submitter).
    /// This only sets the flag — the worker honours it at its next safe point;
    /// status keeps reporting `running` until then.
    ///
    /// # Errors
    ///
    /// Returns [`ServerError::Database`] on a storage failure.
    pub async fn request_cancel(
        &self,
        op_id: Uuid,
        submitter: &str,
        tenant_key: Option<&str>,
    ) -> Result<bool> {
        let done = sqlx::query(
            "UPDATE _system.async_operations
             SET cancellation_requested = true
             WHERE op_id = $1 AND submitter = $2 AND tenant_key IS NOT DISTINCT FROM $3
               AND state = 'running'",
        )
        .bind(op_id)
        .bind(submitter)
        .bind(tenant_key)
        .execute(&self.db)
        .await
        .map_err(|e| db_err("request cancellation", &e))?;
        Ok(done.rows_affected() == 1)
    }

    /// Cancel a claimed operation that has NOT started executing (the worker's
    /// pre-execution check of `cancellation_requested`). Claim-guarded.
    ///
    /// # Errors
    ///
    /// Returns [`ServerError::Database`] on a storage failure.
    pub async fn cancel_unstarted(&self, op_id: Uuid, claim_token: Uuid) -> Result<bool> {
        let done = sqlx::query(
            "UPDATE _system.async_operations
             SET state = 'cancelled', finished_at = now(), claim_token = NULL
             WHERE op_id = $1 AND claim_token = $2 AND state = 'running'",
        )
        .bind(op_id)
        .bind(claim_token)
        .execute(&self.db)
        .await
        .map_err(|e| db_err("cancel unstarted operation", &e))?;
        Ok(done.rows_affected() == 1)
    }

    /// Remove terminal operations older than `ttl_secs`; returns the count.
    ///
    /// # Errors
    ///
    /// Returns [`ServerError::Database`] on a storage failure.
    #[allow(clippy::cast_precision_loss)] // Reason: seconds from config; far below f64's 52-bit mantissa
    pub async fn sweep_finished(&self, ttl_secs: u64) -> Result<u64> {
        let done = sqlx::query(
            "DELETE FROM _system.async_operations
             WHERE state IN ('succeeded','failed','cancelled')
               AND finished_at < now() - make_interval(secs => $1)",
        )
        .bind(ttl_secs as f64)
        .execute(&self.db)
        .await
        .map_err(|e| db_err("sweep finished operations", &e))?;
        Ok(done.rows_affected())
    }
}
