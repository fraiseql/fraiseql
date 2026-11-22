//! The function-dispatch ledger on PostgreSQL (#1628). See
//! [`DispatchLedger`](crate::traits::DispatchLedger) for the protocol.

use std::time::Duration;

use async_trait::async_trait;
use fraiseql_error::{FraiseQLError, Result};
use tokio_postgres::Row;

use super::PostgresAdapter;
use crate::{
    postgres::pg_detail,
    traits::{
        DispatchLedger, DispatchSettlement, DueDispatch, FUNCTION_DISPATCH_LEDGER_DDL,
        PendingDispatch,
    },
};

const INSERT_SQL: &str = "\
INSERT INTO _fraiseql_function_dispatch
    (source, ref_key, function_name, payload, idempotency_token, caller, tenant, lease_until)
SELECT s, r, f, p, t, c, tn, now() + make_interval(secs => $8)
FROM unnest($1::text[], $2::text[], $3::text[], $4::jsonb[], $5::text[], $6::jsonb[], $7::text[])
    AS u(s, r, f, p, t, c, tn)
ON CONFLICT (source, ref_key, function_name) DO NOTHING";

const CLAIM_SQL: &str = "\
WITH due AS (
    SELECT pk_function_dispatch FROM _fraiseql_function_dispatch
    WHERE state = 'pending' AND lease_until <= now() AND tenant IS NOT DISTINCT FROM $3
    ORDER BY lease_until LIMIT $1
    FOR UPDATE SKIP LOCKED)
UPDATE _fraiseql_function_dispatch d
SET lease_until = now() + make_interval(secs => $2), attempts = d.attempts + 1
FROM due WHERE d.pk_function_dispatch = due.pk_function_dispatch
RETURNING d.source, d.ref_key, d.function_name, d.payload, d.idempotency_token, d.caller,
          d.tenant, d.attempts";

const DELETE_SQL: &str = "\
DELETE FROM _fraiseql_function_dispatch
WHERE source = $1 AND ref_key = $2 AND function_name = $3 AND state = 'pending'";

const DEAD_LETTER_SQL: &str = "\
UPDATE _fraiseql_function_dispatch
SET state = 'dead_lettered', reason = $4, settled_at = now()
WHERE source = $1 AND ref_key = $2 AND function_name = $3 AND state = 'pending'";

fn ledger_error(step: &str, error: &tokio_postgres::Error) -> FraiseQLError {
    crate::postgres::database_error(
        format!("function-dispatch ledger: {step}: {}", pg_detail(error)),
        error,
    )
}

/// Insert the durable ones of `dispatches` on `client`, leased for `lease`: inside a write's
/// transaction when `client` is one.
pub(super) async fn insert_pending<C>(
    client: &C,
    dispatches: &[PendingDispatch],
    lease: Duration,
) -> Result<()>
where
    C: tokio_postgres::GenericClient + Sync,
{
    let durable: Vec<&PendingDispatch> = dispatches.iter().filter(|d| d.durable).collect();
    if durable.is_empty() {
        return Ok(());
    }
    let column = |pick: fn(&PendingDispatch) -> &str| -> Vec<&str> {
        durable.iter().map(|d| pick(d)).collect()
    };
    let sources = column(|d| &d.source);
    let ref_keys = column(|d| &d.ref_key);
    let functions = column(|d| &d.function_name);
    let tokens = column(|d| &d.idempotency_token);
    let payloads: Vec<&serde_json::Value> = durable.iter().map(|d| &d.payload).collect();
    let callers: Vec<Option<&serde_json::Value>> =
        durable.iter().map(|d| d.caller.as_ref()).collect();
    let tenants: Vec<Option<&str>> = durable.iter().map(|d| d.tenant.as_deref()).collect();
    client
        .execute(
            INSERT_SQL,
            &[
                &sources,
                &ref_keys,
                &functions,
                &payloads,
                &tokens,
                &callers,
                &tenants,
                &lease.as_secs_f64(),
            ],
        )
        .await
        .map_err(|e| ledger_error("record", &e))?;
    Ok(())
}

fn due(row: &Row) -> DueDispatch {
    DueDispatch {
        source:            row.get("source"),
        ref_key:           row.get("ref_key"),
        function_name:     row.get("function_name"),
        payload:           row.get("payload"),
        idempotency_token: row.get("idempotency_token"),
        caller:            row.get("caller"),
        tenant:            row.get("tenant"),
        attempts:          row.get("attempts"),
    }
}

// async_trait: dyn-dispatch required; remove when RTN + Send is stable (RFC 3425)
#[async_trait]
impl DispatchLedger for PostgresAdapter {
    async fn provision_dispatch_ledger(&self) -> Result<()> {
        let client = self.acquire_connection_with_retry().await?;
        client
            // One simple-query script, so one transaction: the lock is held until the
            // DDL commits, and two registrations at once do not race on the catalogue.
            .batch_execute(&fraiseql_guard::ddl::lock_prefixed(FUNCTION_DISPATCH_LEDGER_DDL))
            .await
            .map_err(|e| ledger_error("provision", &e))
    }

    async fn record_dispatches(
        &self,
        dispatches: &[PendingDispatch],
        lease: Duration,
    ) -> Result<()> {
        let client = self.acquire_connection_with_retry().await?;
        insert_pending(&**client, dispatches, lease).await
    }

    async fn claim_due_dispatches(
        &self,
        tenant: Option<&str>,
        batch: u32,
        lease: Duration,
    ) -> Result<Vec<DueDispatch>> {
        let client = self.acquire_connection_with_retry().await?;
        let rows = client
            .query(CLAIM_SQL, &[&i64::from(batch), &lease.as_secs_f64(), &tenant])
            .await
            .map_err(|e| ledger_error("claim", &e))?;
        Ok(rows.iter().map(due).collect())
    }

    async fn settle_dispatch(
        &self,
        source: &str,
        ref_key: &str,
        function_name: &str,
        settlement: &DispatchSettlement,
    ) -> Result<()> {
        let client = self.acquire_connection_with_retry().await?;
        let settled = match settlement {
            DispatchSettlement::Dispatched => {
                client.execute(DELETE_SQL, &[&source, &ref_key, &function_name]).await
            },
            DispatchSettlement::DeadLettered(reason) => {
                client
                    .execute(DEAD_LETTER_SQL, &[&source, &ref_key, &function_name, reason])
                    .await
            },
        };
        settled.map_err(|e| ledger_error("settle", &e))?;
        Ok(())
    }
}
