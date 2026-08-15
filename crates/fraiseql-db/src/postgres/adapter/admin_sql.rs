//! The operator SQL console's execution path (#962).
//!
//! One statement, one transaction, and every bound enforced by PostgreSQL rather
//! than by looking at the statement text. This is the only place in FraiseQL that
//! runs SQL an operator typed, so the containment is the feature — see
//! [`AdminSqlRequest`] for what each bound is for and
//! [`execute_admin_sql_impl`] for the two properties that come from the protocol
//! rather than from anything written here.

use fraiseql_error::{FraiseQLError, Result};
use futures::TryStreamExt as _;
use tokio_postgres::types::ToSql;

use super::PostgresAdapter;
use crate::{
    postgres::pg_detail,
    traits::{AdminSqlOutcome, AdminSqlRequest},
};

/// Run one operator-supplied statement under `request`'s bounds.
///
/// # What the database enforces, and what this function only asks for
///
/// Everything containing this statement is a PostgreSQL mechanism:
/// `READ ONLY` on the transaction, `statement_timeout`, `ROLLBACK`. Nothing here
/// inspects the SQL, because a check that reads the text is a claim about a
/// dialect this code does not own — an operator who can write `UPDATE` can also
/// write `WITH x AS (UPDATE …) SELECT`, `SELECT nextval(…)` or a `VOLATILE`
/// function that writes, and none of those look like a write to a parser.
///
/// # Two properties that come from the wire protocol
///
/// The statement is sent with the **extended** query protocol, which parses
/// exactly one command. That is load-bearing twice over:
///
/// * A caller cannot append `; COMMIT` to escape rollback-by-default, or `; DROP …` past a review
///   of the first statement — PostgreSQL rejects the Parse itself.
/// * Any `$1` in the text is a parameter placeholder with no binding, so it is an error rather than
///   a hole. No parameters are ever bound.
///
/// # Where it runs
///
/// The primary, always. A console answer that came from a replica would report a
/// write as affecting no rows and a read from a moment that has passed; neither
/// is a preview of anything.
///
/// # Errors
///
/// [`FraiseQLError::Database`] for anything PostgreSQL rejects, with its SQLSTATE
/// attached — including `25006` for a write refused by the read-only transaction
/// and `57014` for a statement the timeout cancelled. Those are not special-cased
/// here: the refusal an operator needs to see is the server's own.
pub(super) async fn execute_admin_sql_impl(
    adapter: &PostgresAdapter,
    request: &AdminSqlRequest,
) -> Result<AdminSqlOutcome> {
    let mut client = adapter.acquire_connection_with_retry().await?;

    let txn =
        client
            .build_transaction()
            .read_only(request.read_only)
            .start()
            .await
            .map_err(|e| FraiseQLError::Database {
                message:   format!("Failed to start admin SQL transaction: {}", pg_detail(&e)),
                sql_state: e.code().map(|c| c.code().to_string()),
            })?;

    // `set_config(…, true)` is `SET LOCAL`: scoped to this transaction, parameter-
    // bound rather than interpolated, and permitted inside a READ ONLY transaction
    // (it changes no data). Applied before the statement so it governs the parse
    // and the plan as well as the execution.
    let timeout = format!("{}ms", request.statement_timeout_ms);
    txn.execute("SELECT set_config('statement_timeout', $1, true)", &[&timeout])
        .await
        .map_err(|e| FraiseQLError::Database {
            message:   format!("Failed to set admin SQL statement timeout: {}", pg_detail(&e)),
            sql_state: e.code().map(|c| c.code().to_string()),
        })?;

    let pairs: Vec<(&str, &str)> =
        request.session_vars.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    super::database::apply_session_vars(&txn, &pairs).await?;

    let outcome = read_statement(&txn, request).await;

    // Commit only on the explicit opt-in, and only if the statement succeeded —
    // committing a transaction whose statement errored would be committing
    // whatever ran before it, which on a single-statement transaction is nothing,
    // but the intent should not depend on that.
    let committed = if request.commit && outcome.is_ok() {
        txn.commit().await.map_err(|e| FraiseQLError::Database {
            message:   format!("Failed to commit admin SQL transaction: {}", pg_detail(&e)),
            sql_state: e.code().map(|c| c.code().to_string()),
        })?;
        true
    } else {
        // Rolling back is the default and the containment. A failure to roll back
        // is logged rather than raised: the connection is discarded either way,
        // and the caller's own error is the one worth returning.
        if let Err(e) = txn.rollback().await {
            tracing::debug!(error = %e, "admin SQL transaction rollback failed");
        }
        false
    };

    let (columns, rows, truncated, rows_affected) = outcome?;
    Ok(AdminSqlOutcome {
        columns,
        rows,
        truncated,
        rows_affected,
        committed,
    })
}

/// Read at most `max_rows` rows, then look for one more.
///
/// Truncation is decided by *asking for the row after the budget*, not by
/// comparing the row count to the budget: a result of exactly `max_rows` rows is
/// complete, and reporting it as truncated would tell an operator their answer is
/// partial when it is whole.
type StatementRead = (Vec<String>, Vec<Vec<serde_json::Value>>, bool, Option<u64>);

async fn read_statement(
    txn: &tokio_postgres::Transaction<'_>,
    request: &AdminSqlRequest,
) -> Result<StatementRead> {
    let no_params: [&(dyn ToSql + Sync); 0] = [];
    let stream = txn.query_raw(request.sql.as_str(), no_params).await.map_err(|e| {
        FraiseQLError::Database {
            message:   format!("Admin SQL statement failed: {}", pg_detail(&e)),
            sql_state: e.code().map(|c| c.code().to_string()),
        }
    })?;
    let mut stream = std::pin::pin!(stream);

    let mut columns: Vec<String> = Vec::new();
    let mut rows: Vec<Vec<serde_json::Value>> = Vec::new();
    let mut truncated = false;

    while let Some(row) = stream.try_next().await.map_err(|e| FraiseQLError::Database {
        message:   format!("Admin SQL statement failed mid-read: {}", pg_detail(&e)),
        sql_state: e.code().map(|c| c.code().to_string()),
    })? {
        if columns.is_empty() {
            columns = row.columns().iter().map(|c| c.name().to_string()).collect();
        }
        if rows.len() >= request.max_rows {
            truncated = true;
            break;
        }
        rows.push(
            (0..row.columns().len())
                .map(|i| super::database::decode_cell(&row, i))
                .collect(),
        );
    }

    // `rows_affected` is populated only once the stream is exhausted, which is
    // exactly the condition under which it means anything: a count of rows nobody
    // read is not a count.
    let rows_affected = if truncated {
        None
    } else {
        stream.rows_affected()
    };
    Ok((columns, rows, truncated, rows_affected))
}
