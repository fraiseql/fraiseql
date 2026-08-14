//! Portal-backed streaming reads (#958).
//!
//! A read reaching this module is delivered as PostgreSQL produces it, rather
//! than collected into a `Vec` at the adapter boundary. That buys two things the
//! `LIMIT n OFFSET k` re-execution loop the export surfaces used cannot have at
//! any batch size:
//!
//! - **One snapshot.** Every row comes from a single statement, so a concurrent insert or delete
//!   cannot shift a row across a batch boundary — the failure mode where an export silently emits
//!   one row twice and another not at all.
//! - **Linear cost.** `OFFSET k` makes PostgreSQL walk and discard `k` rows, so paging through `N`
//!   rows scans `O(N²)`.
//!
//! # What it costs
//!
//! The portal lives inside a transaction, and the transaction lives on a pooled
//! connection, so the read holds that connection for as long as the caller holds
//! the stream. That is the whole duration of an export — potentially minutes of a
//! client reading slowly, not the milliseconds a query takes. Two mechanisms keep
//! that from becoming an availability problem:
//!
//! - a permit from [`PostgresAdapter`]'s streaming semaphore, sized well below
//!   the pool, bounds how many connections streams may hold at once (see
//!   [`PoolPrewarmConfig::max_streaming_reads`](crate::postgres::PoolPrewarmConfig::max_streaming_reads));
//! - the rows are moved through a **bounded** channel, so a client that stops reading stops the
//!   delivery: the channel fills, the pump stops, tokio-postgres stops draining its connection, and
//!   PostgreSQL blocks on the socket. Nothing between the table and the client accumulates rows.
//!
//! The open transaction also holds `ACCESS SHARE` on the relations it reads for its
//! whole life, which is not free: DDL against a view under active export waits for
//! the export to finish. That is the ordinary PostgreSQL rule for any long read,
//! but a streamed export is the first read in this codebase that can be long by
//! *design* rather than by accident, so it is worth stating: schedule migrations
//! against exported views the way you would against any long-running query.
//!
//! # Why a task and a channel rather than the `RowStream` directly
//!
//! `tokio_postgres::Transaction` borrows its `Client`, so a struct holding both a
//! pooled client and a transaction over it is self-referential and cannot be
//! returned from a function. Moving both into one task sidesteps that entirely,
//! and it makes the failure path exact: when the receiver drops, the send fails,
//! the task breaks out, the transaction's `Drop` rolls back and the client is
//! returned to the pool — no cleanup for the caller to forget.
//!
//! The first item is awaited before this module returns, so the errors an
//! ordinary read reports synchronously — bad SQL, a missing relation, a
//! permission or RLS refusal — still reach the caller as an `Err` from the call
//! rather than as an error frame inside a `200` response body.

use std::pin::Pin;

use fraiseql_error::{FraiseQLError, Result};
use futures::{Stream, StreamExt as _, TryStreamExt as _};
use tokio::sync::{Semaphore, mpsc};
use tokio_postgres::Row;

use super::PostgresAdapter;
use crate::{postgres::pg_detail, types::QueryParam};

/// Rows the pump may run ahead of the consumer.
///
/// Small on purpose. The buffer exists to keep the connection busy while the
/// consumer serialises the previous rows, not to hold a page: every row buffered
/// here is a row held in memory, which is the cost this module exists to avoid.
/// One row would serialise the two sides into lockstep; a few thousand would
/// re-introduce a page.
const STREAM_BUFFER_ROWS: usize = 64;

/// Run `sql` as a streaming read and return its rows, decoded by `decode`.
///
/// `session_vars` are applied transaction-locally on the connection that runs the
/// read, exactly as the collecting read paths do (#329) — a streamed read under
/// RLS must see the same rows as the buffered one, or the export becomes a way to
/// read past a policy.
///
/// # Errors
///
/// Returns [`FraiseQLError::ConnectionPool`] if no connection or streaming slot
/// can be acquired, and [`FraiseQLError::Database`] for a failure raised before
/// the first row. A failure after that point arrives as an `Err` item in the
/// stream.
pub(super) async fn stream_rows<T, F>(
    adapter: &PostgresAdapter,
    sql: String,
    params: Vec<QueryParam>,
    session_vars: &[(&str, &str)],
    routing: crate::types::ReadRouting,
    decode: F,
) -> Result<Pin<Box<dyn Stream<Item = Result<T>> + Send>>>
where
    T: Send + 'static,
    F: Fn(&Row, &str) -> Result<T> + Send + 'static,
{
    // Take the streaming slot BEFORE the connection. The other order lets a
    // stream hold a pooled connection while it waits for a permit, which is the
    // exact resource the permit is rationing.
    let permit = Semaphore::acquire_owned(std::sync::Arc::clone(&adapter.streaming_permits))
        .await
        .map_err(|_| FraiseQLError::ConnectionPool {
            message: "Streaming read slots are closed; the adapter is shutting down".to_string(),
        })?;

    let mut client = adapter.acquire_read_connection_with_retry(routing).await?;
    let owned_vars: Vec<(String, String)> =
        session_vars.iter().map(|(k, v)| ((*k).to_string(), (*v).to_string())).collect();

    let (tx, mut rx) = mpsc::channel::<Result<T>>(STREAM_BUFFER_ROWS);

    tokio::spawn(async move {
        // Held for the life of the delivery, released when this task ends —
        // which is also when the connection goes back to the pool.
        let _permit = permit;

        let txn = match client.build_transaction().read_only(true).start().await {
            Ok(txn) => txn,
            Err(e) => {
                let _ = tx
                    .send(Err(FraiseQLError::Database {
                        message:   format!(
                            "Failed to start streaming read transaction: {}",
                            pg_detail(&e)
                        ),
                        sql_state: e.code().map(|c| c.code().to_string()),
                    }))
                    .await;
                return;
            },
        };

        let pairs: Vec<(&str, &str)> =
            owned_vars.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
        if let Err(e) = super::database::apply_session_vars(&txn, &pairs).await {
            let _ = tx.send(Err(e)).await;
            return;
        }

        let param_refs = crate::types::as_sql_param_refs(&params);
        let rows = match txn.query_raw(&sql, param_refs).await {
            Ok(rows) => rows,
            Err(e) => {
                let _ = tx
                    .send(Err(FraiseQLError::Database {
                        message:   format!("Streaming query failed: {}", pg_detail(&e)),
                        sql_state: e.code().map(|c| c.code().to_string()),
                    }))
                    .await;
                return;
            },
        };
        let mut rows = std::pin::pin!(rows);

        loop {
            let item = match rows.try_next().await {
                Ok(Some(row)) => decode(&row, &sql),
                Ok(None) => break,
                Err(e) => Err(FraiseQLError::Database {
                    message:   format!("Streaming query failed mid-read: {}", pg_detail(&e)),
                    sql_state: e.code().map(|c| c.code().to_string()),
                }),
            };
            let failed = item.is_err();
            // A send failure means the consumer dropped the stream: stop pumping,
            // roll back and hand the connection back. This is the ordinary end of
            // a client that disconnects mid-export, not an error.
            if tx.send(item).await.is_err() || failed {
                break;
            }
        }
        // The transaction is read-only, so rolling back is exactly as correct as
        // committing and does not depend on having reached the end of the rows.
        if let Err(e) = txn.rollback().await {
            tracing::debug!(error = %e, "streaming read transaction rollback failed");
        }
    });

    // Surface a setup failure as this call's error rather than as the stream's
    // first item — a caller that has already sent response headers cannot turn
    // "relation does not exist" back into a 4xx.
    let Some(first) = rx.recv().await else {
        return Ok(Box::pin(futures::stream::empty()));
    };
    let first = first?;

    let rest =
        futures::stream::unfold(rx, |mut rx| async move { rx.recv().await.map(|i| (i, rx)) });
    Ok(Box::pin(futures::stream::once(async move { Ok(first) }).chain(rest)))
}
