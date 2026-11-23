//! `DatabaseAdapter` and `Writer` implementations for `PostgresAdapter`.

use std::{fmt::Write as _, sync::Arc};

use async_trait::async_trait;
use bytes::BufMut as _;
use fraiseql_error::{FraiseQLError, Result};
use tokio_postgres::{
    Row,
    types::{FromSql, Kind, Type},
};

use super::{
    Decimals, JsonText, PostgresAdapter, build_projection_select_sql, build_where_select_sql,
    build_where_select_sql_ordered,
    composed::build_composed_select_sql,
    document_cell,
    numeric::PgNumericText,
    transaction_retry::{RetryEvent, TransactionRetry},
};
use crate::{
    identifier::quote_postgres_identifier,
    postgres::pg_detail,
    traits::{
        ColumnRowStream, ComposedLevel, DatabaseAdapter, JsonbRowStream, ProjectionRequest,
        RawRows, Writer,
    },
    types::{
        DatabaseType, DescribedColumn, DescribedRows, JsonbValue, NumericType, PoolMetrics,
        QueryParam, ReadRouting,
        sql_hints::{OrderByClause, SqlProjectionHint},
    },
    where_clause::WhereClause,
};

/// One row a write function returned, by column.
type RowMap = std::collections::HashMap<String, serde_json::Value>;

/// One call of a write function, ready to send: its statement, its arguments, and the
/// configuration it runs under.
struct PreparedCall<'r> {
    function_name:  &'r str,
    sql:            String,
    args:           Vec<FlexParam>,
    configuration:  Vec<(&'r str, &'r str)>,
    /// Names the outbox in an error message when the outbox is what ran. The messages are
    /// the strings the error sanitizer's corpus is reduced from, and an operator reading a
    /// failed write wants to know whether the CTE was in it.
    mechanism:      &'static str,
    derived:        Option<&'r crate::traits::DerivedCascade<'r>>,
    /// Report the rows the call changed, at most this many, in the call's own flight
    /// (#1576). Never with `derived`, whose report already says.
    report_changes: Option<i32>,
    decimals:       Option<&'r crate::traits::WriteDecimals>,
}

impl<'r> PreparedCall<'r> {
    fn new(adapter: &'r PostgresAdapter, request: &crate::traits::WriteRequest<'r>) -> Self {
        let crate::traits::WriteRequest {
            function: function_name,
            args,
            session_vars,
            changelog,
            derived_cascade,
            report_changes,
            decimals,
            ..
        } = *request;
        let quoted_fn = quote_postgres_identifier(function_name);

        // One statement when there is an outbox row: run the function once and INSERT
        // its outbox row in the same txn, atomically, with no extra connection acquire
        // (Change Spine). Otherwise the plain call.
        let sql = if let Some(cl) = changelog {
            build_changelog_cte_sql(&quoted_fn, args.len(), cl.pre_image)
        } else {
            let placeholders: Vec<String> = (1..=args.len()).map(|i| format!("${i}")).collect();
            format!("SELECT * FROM {quoted_fn}({})", placeholders.join(", "))
        };

        // Arguments bind as FlexParam: serde_json::Value only accepts JSON/JSONB types, and
        // Option<String> only text-family types, and a function signature can mix any of
        // them. FlexParam accepts every PostgreSQL type and sends each value in the format of
        // the server-resolved parameter type: binary for six, text for the rest (#1547).
        let mut flex_args: Vec<FlexParam> = args
            .iter()
            .map(|v| match v {
                serde_json::Value::Null => FlexParam::Null,
                value => FlexParam::Value(value.clone()),
            })
            .collect();
        if let Some(changelog) = changelog {
            // Function args first; then the threaded change-log envelope params —
            // object_type fallback ($n+1), modification_type verb ($n+2), the
            // tenant_id stamp ($n+3, bound against `::uuid`), the trace_id
            // ($n+4, plain text), the schema_version ($n+5, plain text), the
            // trace_context ($n+6, bound against `::jsonb`), the actor_type ($n+7,
            // plain text), the acting_for ($n+8, bound against `::uuid`) and the
            // transport ($n+9, plain text merged into extra_metadata — #376). Order
            // matches build_changelog_cte_sql's positional contract; appending the
            // envelope params keeps the SQL text stable for prepare_cached.
            flex_args.push(FlexParam::Text(changelog.object_type.to_string()));
            flex_args.push(FlexParam::Text(changelog.modification_type.to_string()));
            // tenant_id: bound as text and serialised by FlexParam's UUID branch
            // (the `::uuid` cast pins the param type); None → SQL NULL.
            flex_args.push(
                changelog.tenant_id.map_or(FlexParam::Null, |t| FlexParam::Text(t.to_string())),
            );
            // trace_id ($n+4): plain text, None → SQL NULL.
            flex_args.push(
                changelog.trace_id.map_or(FlexParam::Null, |t| FlexParam::Text(t.to_string())),
            );
            // schema_version ($n+5): plain text, None → SQL NULL.
            flex_args.push(
                changelog
                    .schema_version
                    .map_or(FlexParam::Null, |s| FlexParam::Text(s.to_string())),
            );
            // trace_context ($n+6): JSON text bound against `::jsonb`, None → SQL NULL.
            flex_args.push(
                changelog
                    .trace_context
                    .map_or(FlexParam::Null, |s| FlexParam::Text(s.to_string())),
            );
            // actor_type ($n+7): plain text, None → SQL NULL.
            flex_args.push(
                changelog.actor_type.map_or(FlexParam::Null, |s| FlexParam::Text(s.to_string())),
            );
            // acting_for ($n+8): bound as text + serialised by FlexParam's UUID branch
            // (the `::uuid` cast pins the param type); None → SQL NULL.
            flex_args.push(
                changelog.acting_for.map_or(FlexParam::Null, |u| FlexParam::Text(u.to_string())),
            );
            // transport ($n+9): plain text merged into extra_metadata.transport by the
            // CTE's deterministic CASE; None → SQL NULL → key omitted (#376).
            flex_args.push(
                changelog.transport.map_or(FlexParam::Null, |s| FlexParam::Text(s.to_string())),
            );
        }

        // Everything the function and the outbox read of their configuration, in one
        // statement: the session variables, the #366 marker, the timing stamp, and the
        // `started_at` the outbox needs. Later pairs win, as when each was its own statement.
        //
        // The marker makes the #366 fallback-capture trigger suppress its row: this write is
        // already logged by the outbox, and on the no-outbox path the opt-out means "no
        // change-log row", not "let the trigger write a degraded one".
        let mut configuration: Vec<(&str, &str)> = Vec::with_capacity(session_vars.len() + 3);
        configuration.extend_from_slice(session_vars);
        configuration.push((crate::changelog::CDC_MEDIATED_VAR, crate::changelog::CDC_MEDIATED_ON));
        if adapter.mutation_timing_enabled {
            configuration.push((
                adapter.timing_variable_name.as_str(),
                crate::changelog::CLOCK_TIMESTAMP_DIRECTIVE,
            ));
        }
        // The outbox INSERT reads `fraiseql.started_at` with current_setting() (no
        // missing_ok), so it must exist on this txn. The mutation runner injects it on the
        // authenticated path; stamp it on every other one (e.g. an unauthenticated mutation
        // that resolves no session vars), and never over a value already set. Only the
        // outbox reads it.
        if changelog.is_some()
            && !configuration.iter().any(|(name, _)| *name == crate::changelog::STARTED_AT_VAR)
        {
            configuration.push((
                crate::changelog::STARTED_AT_VAR,
                crate::changelog::CLOCK_TIMESTAMP_DIRECTIVE,
            ));
        }

        Self {
            function_name,
            sql,
            args: flex_args,
            configuration,
            mechanism: if changelog.is_some() {
                " (with change-log outbox)"
            } else {
                ""
            },
            derived: derived_cascade,
            report_changes: report_changes.filter(|_| derived_cascade.is_none()),
            decimals,
        }
    }

    fn params(&self) -> Vec<&(dyn tokio_postgres::types::ToSql + Sync)> {
        self.args
            .iter()
            .map(|v| v as &(dyn tokio_postgres::types::ToSql + Sync))
            .collect()
    }

    fn configuration_failed(&self, e: &tokio_postgres::Error) -> FraiseQLError {
        let names: Vec<&str> = self.configuration.iter().map(|(name, _)| *name).collect();
        crate::postgres::database_error(
            format!("set_config({names:?}) failed: {}", pg_detail(e)),
            e,
        )
    }

    fn call_failed(&self, e: &tokio_postgres::Error) -> FraiseQLError {
        crate::postgres::database_error(
            format!(
                "Function call {}{} failed: {}",
                self.function_name,
                self.mechanism,
                pg_detail(e)
            ),
            e,
        )
    }
}

/// The one write (ruling AA 6): run write functions inside one explicit transaction, each
/// with its change-log outbox row in the same statement when its request carries one, and
/// let the gate decide the transaction's end.
///
/// `gate` runs on each call's rows, in order, after the call and before the transaction
/// ends. An `Err` from it rolls back and is returned to the caller verbatim, so a refusal
/// that could only be decided from a written row still refuses the write — every call of a
/// batch with it (#1574). In [`WriteMode::DryRun`] everything else happens exactly as in a
/// commit — session variables, CDC marking, the timing stamp, the outbox row, the gate — and
/// the transaction then rolls back: a dry run exercises the write it stands in for, and
/// nothing persists.
///
/// `numbered`: a database error says which call of the batch raised it.
async fn run_functions_in_txn(
    adapter: &PostgresAdapter,
    requests: &[crate::traits::WriteRequest<'_>],
    gate: crate::traits::BatchRowGate<'_>,
    numbered: bool,
) -> Result<Vec<Vec<RowMap>>> {
    let Some(mode) = requests.first().map(|request| request.mode) else {
        return Ok(Vec::new());
    };
    if requests.iter().any(|request| request.mode != mode) {
        return Err(FraiseQLError::Validation {
            message: "a batch of writes commits all of its calls or none: they must share one \
                      write mode"
                .to_string(),
            path:    None,
        });
    }
    let count = requests.len();
    let at = |index: usize, error: FraiseQLError| {
        if numbered {
            error.in_batch_item(index, count)
        } else {
            error
        }
    };
    let commit = matches!(mode, crate::traits::WriteMode::Commit);
    // Arm the read-your-writes pin (entry + post-commit) — #407. A dry run writes nothing
    // for a later read to miss.
    if commit {
        adapter.mark_write();
    }
    let calls: Vec<PreparedCall<'_>> =
        requests.iter().map(|request| PreparedCall::new(adapter, request)).collect();
    let configurations: Vec<_> =
        calls.iter().map(|call| set_config_batch(&call.configuration)).collect();

    let mut client = adapter.acquire_connection_with_retry().await?;
    // Parse/plan each statement once per connection (statement cache): the call (with the
    // outbox, a complex CTE) and the configuration, whose text depends only on how many
    // pairs it sets and which carry the clock directive. A batch of one function prepares
    // each text once.
    let mut statements = Vec::with_capacity(count);
    for (index, (call, (configure_sql, _))) in calls.iter().zip(&configurations).enumerate() {
        let stmt = prepare_cached_stmt(&client, call.sql.as_str())
            .await
            .map_err(|e| at(index, e))?;
        let configure = client
            .prepare_cached(configure_sql)
            .await
            .map_err(|e| at(index, call.configuration_failed(&e)))?;
        let report = match call.report_changes {
            Some(_) => Some(
                client
                    .prepare_cached(CHANGED_ROWS_SQL)
                    .await
                    .map_err(|e| at(index, changes_report_failed(&e)))?,
            ),
            None => None,
        };
        statements.push((stmt, configure, report));
    }
    let mechanism = calls
        .iter()
        .find(|call| !call.mechanism.is_empty())
        .map_or("", |call| call.mechanism);

    // A mutation whose transaction PostgreSQL aborts as a deadlock victim or a serialization
    // failure runs again, as the adapter's policy says: on this connection, with the
    // statements already prepared, from `BEGIN`. Each attempt re-sends its whole flight, so
    // it runs against the data as it is then. Nothing is retried once `COMMIT` succeeds.
    // A batch commits all of its calls or none, so one call that opts out of retry (its
    // function has an effect outside the transaction) makes the whole batch run once.
    let policy = adapter.transaction_retry();
    let opted_out = requests.iter().any(|request| !request.retry_on_abort);
    let mut attempt: u32 = 1;
    loop {
        let txn = client.build_transaction().start().await.map_err(|e| {
            crate::postgres::database_error(
                format!("Failed to start mutation{mechanism} transaction: {}", pg_detail(&e)),
                &e,
            )
        })?;
        let ran: std::result::Result<Vec<Vec<RowMap>>, Ended> = async {
            // The configuration and the call in one flight (#1572): the driver sends both before
            // reading either answer, and PostgreSQL runs them in order, so the function sees the
            // configuration. Only once `BEGIN` is confirmed, so neither can run outside the txn.
            // A failed configuration aborts the txn and the call with it; its own error is the one
            // reported, not the call's "current transaction is aborted".
            //
            // A call that reports what it changed (#1576) sends the report in the same flight,
            // after it: no round trip of its own.
            let flight = |index: usize| {
                let (call, (stmt, configure, report)) = (&calls[index], &statements[index]);
                let configure_params = &configurations[index].1;
                let txn = &txn;
                async move {
                    let params = call.params();
                    let max_entities = call.report_changes.unwrap_or_default();
                    futures::join!(
                        txn.execute(configure, configure_params),
                        txn.query(stmt, params.as_slice()),
                        async {
                            match report {
                                Some(report) => Some(txn.query_one(report, &[&max_entities]).await),
                                None => None,
                            }
                        }
                    )
                }
            };
            // What a flight answered, as the call's rows: its errors, then (#1391) the rows
            // pg_tviews reports this transaction changed, read on this connection
            // before the gate adjudicates the row they are attached to.
            let settle = |index: usize,
                          (configured, called, reported): (
                std::result::Result<u64, tokio_postgres::Error>,
                std::result::Result<Vec<Row>, tokio_postgres::Error>,
                Option<std::result::Result<Row, tokio_postgres::Error>>,
            )| {
                let call = &calls[index];
                let txn = &txn;
                async move {
                    configured.map_err(|e| call.configuration_failed(&e))?;
                    let rows = called.map_err(|e| call.call_failed(&e))?;
                    let mut results: Vec<RowMap> = rows
                        .iter()
                        .map(|row| write_row_to_map(row, call.decimals))
                        .collect::<Result<_>>()?;
                    if let Some(reported) = reported {
                        let report: serde_json::Value =
                            reported.map_err(|e| changes_report_failed(&e))?.get("report");
                        if let Some(first) = results.first_mut() {
                            first.insert(crate::traits::CHANGED_ROWS_KEY.to_string(), report);
                        }
                    }
                    if let Some(derived) = call.derived {
                        if let Some(first) = results.first_mut() {
                            if first.get("succeeded").and_then(serde_json::Value::as_bool)
                                == Some(true)
                            {
                                let report =
                                    derived_cascade_report(txn, derived, call.decimals).await?;
                                first
                                    .insert(crate::traits::DERIVED_CASCADE_KEY.to_string(), report);
                            }
                        }
                    }
                    Ok::<_, FraiseQLError>(results)
                }
            };

            // Every flight at once (#1574), unless a call reports a derived cascade: its report is
            // read after its own call and before the next, so those calls go one flight at a time.
            let answers: Vec<Option<_>> = if calls.iter().all(|call| call.derived.is_none()) {
                in_order((0..count).map(flight).collect()).await.into_iter().map(Some).collect()
            } else {
                std::iter::repeat_with(|| None).take(count).collect()
            };
            let mut results = Vec::with_capacity(count);
            for (index, answer) in answers.into_iter().enumerate() {
                let answer = match answer {
                    Some(answer) => answer,
                    None => flight(index).await,
                };
                let rows =
                    settle(index, answer).await.map_err(|e| Ended::Failed(index, at(index, e)))?;

                // The gate decides the commit. It sees the rows the function returned — the
                // only thing a per-row decision can be keyed on — while they are still
                // uncommitted, so a refusal takes the write with it rather than leaving the
                // caller refused and the side effect standing (#1353).
                if let Err(refusal) = gate(index, &rows) {
                    return Err(Ended::Refused(refusal));
                }
                // The dispatches the gate staged for this call are recorded in this attempt's
                // transaction: they commit with the write or not at all, and a retried
                // attempt records what its own gate staged.
                if let Some(sink) = requests[index].dispatch_ledger {
                    super::dispatch_ledger::insert_pending(&*txn, &sink.staged(), sink.lease())
                        .await
                        .map_err(|e| Ended::Failed(index, at(index, e)))?;
                }
                results.push(rows);
            }
            Ok(results)
        }
        .await;

        let (failed, error) = match ran {
            // A refusal is a decision, not a failure: never retried.
            Err(Ended::Refused(refusal)) => {
                txn.rollback().await.map_err(|e| {
                    crate::postgres::database_error(
                        format!(
                            "Failed to roll back refused mutation transaction: {}",
                            pg_detail(&e)
                        ),
                        &e,
                    )
                })?;
                return Err(refusal);
            },
            Err(Ended::Failed(index, error)) => {
                if !retries(policy, opted_out, attempt, &error, calls[index].function_name) {
                    return Err(error);
                }
                // Ruling (a): end the failed transaction explicitly, on this connection,
                // before the next `BEGIN`. A `ROLLBACK` that fails means the connection is
                // gone and the outcome unknown: the attempt's own error is returned.
                if txn.rollback().await.is_err() {
                    return Err(error);
                }
                (index, error)
            },
            Ok(results) if !commit => {
                // The whole point of a dry run: discard every write, the outbox row included.
                txn.rollback().await.map_err(|e| {
                    crate::postgres::database_error(
                        format!("Failed to roll back dry-run transaction: {}", pg_detail(&e)),
                        &e,
                    )
                })?;
                return Ok(results);
            },
            Ok(results) => match txn.commit().await {
                Ok(()) => {
                    adapter.mark_write();
                    return Ok(results);
                },
                Err(e) => {
                    let error = crate::postgres::database_error(
                        format!(
                            "Failed to commit mutation{mechanism} transaction: {}",
                            pg_detail(&e)
                        ),
                        &e,
                    );
                    // A `COMMIT` names no call: the first one stands for the batch.
                    if !retries(policy, opted_out, attempt, &error, calls[0].function_name) {
                        return Err(error);
                    }
                    // The failed `COMMIT` ended the transaction; the explicit `ROLLBACK`
                    // still proves the connection is there before the next `BEGIN`.
                    if client.batch_execute("ROLLBACK").await.is_err() {
                        return Err(error);
                    }
                    (0, error)
                },
            },
        };

        let backoff = policy.draw_backoff(attempt);
        let sql_state = retryable_sql_state(&error).unwrap_or_default();
        let function = calls[failed].function_name;
        tracing::debug!(function, sql_state, attempt, ?backoff, "retrying mutation transaction");
        policy.observe(RetryEvent::Retrying {
            function,
            sql_state,
            attempt,
            backoff,
        });
        tokio::time::sleep(backoff).await;
        attempt += 1;
    }
}

/// How an attempt at a mutation's transaction ended short of its `COMMIT`.
enum Ended {
    /// The gate refused call `index`'s rows.
    Refused(FraiseQLError),
    /// Call `index` (or its configuration, or its report) failed.
    Failed(usize, FraiseQLError),
}

/// The SQLSTATE that makes `error` worth another attempt: `40P01` (deadlock detected) or
/// `40001` (serialization failure). An error with no SQLSTATE — a lost connection — never
/// is: whether the write committed is unknown.
fn retryable_sql_state(error: &FraiseQLError) -> Option<&str> {
    match error {
        FraiseQLError::Database {
            sql_state: Some(state),
            ..
        } if matches!(state.as_str(), "40P01" | "40001") => Some(state),
        _ => None,
    }
}

/// Whether `attempt`, which failed with `error`, runs again under `policy`: never when a call
/// of the transaction opts out. An attempt that would have been retried but was the last is
/// reported as an exhaustion.
fn retries(
    policy: &TransactionRetry,
    opted_out: bool,
    attempt: u32,
    error: &FraiseQLError,
    function: &str,
) -> bool {
    let Some(sql_state) = retryable_sql_state(error) else {
        return false;
    };
    if opted_out {
        if policy.max_attempts() > 1 {
            tracing::debug!(
                function,
                sql_state,
                "mutation transaction not retried: a call of it opts out of retry"
            );
        }
        return false;
    }
    if policy.allows_after(attempt) {
        return true;
    }
    if policy.max_attempts() > 1 {
        tracing::debug!(
            function,
            sql_state,
            attempts = attempt,
            "mutation transaction retries exhausted"
        );
        policy.observe(RetryEvent::Exhausted {
            function,
            sql_state,
            attempts: attempt,
        });
    }
    false
}

/// Await `futures` together, polling each for the first time in order: what one sends
/// reaches the connection before what the next sends, so PostgreSQL runs them in that order.
async fn in_order<F: std::future::Future>(futures: Vec<F>) -> Vec<F::Output> {
    let mut pending: Vec<_> =
        futures.into_iter().map(|f| Box::pin(futures::future::maybe_done(f))).collect();
    std::future::poll_fn(|cx| {
        let mut done = true;
        for future in &mut pending {
            done &= std::future::Future::poll(future.as_mut(), cx).is_ready();
        }
        if done {
            std::task::Poll::Ready(())
        } else {
            std::task::Poll::Pending
        }
    })
    .await;
    pending.iter_mut().filter_map(|future| future.as_mut().take_output()).collect()
}

/// The report of the rows a call changed (#1576): `pg_tviews`' ids only, no row read.
const CHANGED_ROWS_SQL: &str =
    "SELECT tviews.pg_tviews_flush_and_report($1, false, true) AS report";

fn changes_report_failed(e: &tokio_postgres::Error) -> FraiseQLError {
    crate::postgres::database_error(
        format!(
            "tviews.pg_tviews_flush_and_report() failed, asked for the rows this write changed \
             to evict the cached reads of exactly those (is pg_tviews installed and \
             preloaded?): {}",
            pg_detail(e)
        ),
        e,
    )
}

/// Flush `pg_tviews`' queue and report the rows this transaction changed, each reported
/// updated row read from its type's view on this connection (see
/// [`crate::traits::DerivedCascade`]).
///
/// The rows of a type are read by one prepared statement, `WHERE id = ANY($1)`, and every
/// type's read is sent in one flight (#1573): the cost does not grow with the fan-out.
async fn derived_cascade_report(
    txn: &deadpool_postgres::Transaction<'_>,
    derived: &crate::traits::DerivedCascade<'_>,
    decimals: Option<&crate::traits::WriteDecimals>,
) -> Result<serde_json::Value> {
    let report_failed = |e: &tokio_postgres::Error| {
        crate::postgres::database_error(
            format!(
                "cascade_source = \"pg_tviews\": tviews.pg_tviews_flush_and_report() \
                 failed (is pg_tviews installed and preloaded?): {}",
                pg_detail(e)
            ),
            e,
        )
    };
    let flush = txn
        .prepare_cached(
            "SELECT tviews.pg_tviews_flush_and_report($1, false, true) AS report, \
             to_char(now() AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS.US\"Z\"') AS at",
        )
        .await
        .map_err(|e| report_failed(&e))?;
    let row = txn
        .query_one(&flush, &[&derived.max_entities])
        .await
        .map_err(|e| report_failed(&e))?;
    let mut report: serde_json::Value = row.get("report");
    let at: String = row.get("at");
    let reported_id = |entry: &serde_json::Value| match &entry["id"] {
        serde_json::Value::String(s) => s.clone(),
        other => other.to_string(),
    };

    if let Some(updated) = report.get_mut("updated").and_then(serde_json::Value::as_array_mut) {
        // The reported ids of each type a view serves, in report order.
        let mut ids_by_view: Vec<(&str, &str, Vec<FlexParam>)> = Vec::new();
        for entry in updated.iter() {
            let typename = entry["__typename"].as_str().unwrap_or_default();
            let Some((name, view)) = derived.views.iter().find(|(name, _)| *name == typename)
            else {
                continue;
            };
            let id = FlexParam::Text(reported_id(entry));
            match ids_by_view.iter_mut().find(|(n, ..)| n == name) {
                Some((.., ids)) => ids.push(id),
                None => ids_by_view.push((name, view, vec![id])),
            }
        }
        let reads = ids_by_view.iter().map(|(typename, view, ids)| async move {
            let read_failed = |e: &tokio_postgres::Error| {
                crate::postgres::database_error(
                    format!(
                        "cascade_source = \"pg_tviews\": reading {typename} from {view} \
                         failed: {}",
                        pg_detail(e)
                    ),
                    e,
                )
            };
            // Prepared once per connection: one text per view a schema names.
            let sql = format!(
                "SELECT id::text AS id, data FROM {} WHERE id = ANY($1)",
                quote_postgres_identifier(view)
            );
            let statement = txn.prepare_cached(&sql).await.map_err(|e| read_failed(&e))?;
            let rows = txn.query(&statement, &[ids]).await.map_err(|e| read_failed(&e))?;
            let shape = decimals.map(|decimals| decimals.document(typename));
            let data = |r: &Row| match &shape {
                Some(shape) => json_column(r, "data", Decimals::Document(shape))
                    .unwrap_or(Ok(serde_json::Value::Null)),
                None => Ok(r.get("data")),
            };
            let by_id = rows
                .iter()
                .map(|r| Ok((r.get("id"), data(r)?)))
                .collect::<Result<std::collections::HashMap<String, serde_json::Value>>>()?;
            Ok::<_, FraiseQLError>((*typename, by_id))
        });
        let read: Vec<(&str, std::collections::HashMap<String, serde_json::Value>)> =
            futures::future::try_join_all(reads).await?;

        // A row its view did not return (not visible to this caller), or of a type no view
        // serves, is `null`.
        for entry in updated.iter_mut() {
            let typename = entry["__typename"].as_str().unwrap_or_default();
            let entity = read
                .iter()
                .find(|(name, _)| *name == typename)
                .and_then(|(_, by_id)| by_id.get(&reported_id(entry)))
                .cloned()
                .unwrap_or(serde_json::Value::Null);
            entry["entity"] = entity;
        }
    }
    if let Some(deleted) = report.get_mut("deleted").and_then(serde_json::Value::as_array_mut) {
        for entry in deleted.iter_mut() {
            entry["deletedAt"] = serde_json::Value::String(at.clone());
        }
    }
    Ok(report)
}

/// PostgreSQL SQLSTATE 42703: undefined column.
const PG_UNDEFINED_COLUMN: &str = "42703";

/// A flexible SQL parameter that binds to any PostgreSQL type.
///
/// Solves the impedance mismatch between `serde_json::Value` (only accepts JSON/JSONB)
/// and `Option<String>` (only accepts text-family types) when binding function-call
/// arguments whose types are resolved at runtime from the function signature.
///
/// Serialisation strategy:
/// - `JSONB`: binary, a 1-byte version header (1) + UTF-8 JSON bytes
/// - `JSON`: binary, UTF-8 JSON bytes
/// - `UUID`: binary, 16-byte big-endian UUID
/// - `INT4`: binary, 4-byte big-endian i32
/// - `INT8`: binary, 8-byte big-endian i64
/// - `BOOL`: binary, 1 byte (0 or 1)
/// - Every other type: the **text** format, the value's UTF-8 bytes, which PostgreSQL parses with
///   the type's own input function (#1547). A `float8`, `numeric`, `date`, `timestamp`, `interval`,
///   an array, an enum or a domain reads its text form; the binary form of most of them is not
///   their text (a `float8` is 8 bytes, a `date` 4), so sending the text as binary failed with
///   `insufficient data left in message`.
///
/// A write function's argument is the JSON value the request carries, written for the
/// parameter's type: a `json`/`jsonb` parameter takes it as the JSON it is (a string stays a
/// string), an array parameter a JSON array in PostgreSQL's array syntax, `{"a","b"}`, each
/// item parsed by the element type's input function, and any other parameter its text (a
/// string's own characters, any other value's JSON).
///
/// Inside an array (`id = ANY($1)`) each element is written by [`ToSql::to_sql`] in the binary
/// format whatever [`ToSql::encode_format`] says, so the six binary encodings stay.
#[derive(Debug)]
enum FlexParam {
    /// SQL NULL — accepted by any PostgreSQL type.
    Null,
    /// A text-encoded value; binary-serialised according to the server-resolved type. A
    /// `json`/`jsonb` parameter takes the text as JSON text.
    Text(String),
    /// A request's JSON value, written for the server-resolved type.
    Value(serde_json::Value),
}

/// An aggregate's parameters, each written for the type the server resolves it to, as a
/// write function's arguments are (#1620): a string compared with `data->>'region'` is text,
/// a number compared with `SUM(qty)` (a `numeric`) its text in the text format. They were
/// bound as `QueryParam`, whose number is text bytes sent under the binary format, which a
/// `numeric` parameter cannot read (`insufficient data left in message`).
fn aggregate_params(params: &[serde_json::Value]) -> Vec<FlexParam> {
    bound_values(params)
}

/// `values`, each written for the type the server resolves its parameter to: a function
/// source's arguments and an aggregate's parameters.
/// A projection read's statement and its parameters, built once for every way the read
/// is delivered.
struct ProjectionStatement {
    sql:           String,
    // A function source's arguments are `$1 … $n`, each written for the type of the
    // parameter it binds, as a write function's are.
    source_params: Vec<FlexParam>,
    typed_params:  Vec<QueryParam>,
}

impl ProjectionStatement {
    fn of(request: &ProjectionRequest<'_>) -> Result<Self> {
        let from = super::ReadFrom::of(request);
        let (page, params) = match request.projection {
            Some(projection) => build_projection_select_sql(
                projection,
                from,
                request.where_clause,
                request.limit,
                request.offset,
                request.order_by,
            )?,
            None => super::build_where_select_sql_ordered(
                from,
                request.where_clause,
                request.limit,
                request.offset,
                request.order_by,
            )?,
        };
        // #1314: a page that also counts what its filter matches, in one statement so the
        // count and the page see the same snapshot.
        let (sql, typed_params) = match request.matched_up_to {
            Some(bound) => {
                super::with_matched_count(&page, params, from, request.where_clause, bound)?
            },
            None => (page, params),
        };
        tracing::debug!("SQL with projection = {}", sql);
        Ok(Self {
            sql,
            source_params: request.source_args.map_or_else(Vec::new, bound_values),
            typed_params,
        })
    }

    fn params(&self) -> Vec<&(dyn tokio_postgres::types::ToSql + Sync)> {
        self.source_params
            .iter()
            .map(|p| p as &(dyn tokio_postgres::types::ToSql + Sync))
            .chain(
                self.typed_params
                    .iter()
                    .map(|p| p as &(dyn tokio_postgres::types::ToSql + Sync)),
            )
            .collect()
    }
}

fn bound_values(values: &[serde_json::Value]) -> Vec<FlexParam> {
    values
        .iter()
        .map(|v| match v {
            serde_json::Value::Null => FlexParam::Null,
            value => FlexParam::Value(value.clone()),
        })
        .collect()
}

/// Write `text`, the text form of a value, for the parameter type `ty`: the binary form of
/// the six types `encode_format` sends in binary, the UTF-8 bytes otherwise.
fn write_text(
    text: &str,
    ty: &tokio_postgres::types::Type,
    out: &mut bytes::BytesMut,
) -> std::result::Result<(), Box<dyn std::error::Error + Sync + Send>> {
    use tokio_postgres::types::Type;
    if *ty == Type::JSONB {
        // JSONB binary wire format: 1-byte version (1) + JSON bytes
        out.put_u8(1);
        out.extend_from_slice(text.as_bytes());
    } else if *ty == Type::UUID {
        let uuid = uuid::Uuid::parse_str(text)?;
        out.extend_from_slice(uuid.as_bytes());
    } else if *ty == Type::INT4 {
        let n: i32 = text.parse()?;
        out.put_i32(n);
    } else if *ty == Type::INT8 {
        let n: i64 = text.parse()?;
        out.put_i64(n);
    } else if *ty == Type::BOOL {
        let b: bool = text.parse()?;
        out.put_u8(u8::from(b));
    } else {
        // Sent in the text format (`encode_format`): the UTF-8 bytes, parsed by the type's
        // input function. Also the binary form of `json` and of the text family.
        out.extend_from_slice(text.as_bytes());
    }
    Ok(())
}

/// Write `items` in PostgreSQL's array syntax: every item quoted (an element type's input
/// function reads `"1"` as it reads `1`), `NULL` for a JSON `null`, and a nested JSON array as
/// a sub-array, unless the elements are `json`/`jsonb`, where every item is one JSON value
/// written as its JSON text.
fn write_array_literal(items: &[serde_json::Value], json_elements: bool, out: &mut String) {
    out.push('{');
    for (i, item) in items.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        match item {
            serde_json::Value::Null => out.push_str("NULL"),
            serde_json::Value::Array(inner) if !json_elements => {
                write_array_literal(inner, json_elements, out);
            },
            serde_json::Value::String(s) if !json_elements => quote_array_item(s, out),
            other => quote_array_item(&other.to_string(), out),
        }
    }
    out.push('}');
}

fn quote_array_item(item: &str, out: &mut String) {
    out.push('"');
    for c in item.chars() {
        if c == '"' || c == '\\' {
            out.push('\\');
        }
        out.push(c);
    }
    out.push('"');
}

/// The element type of `ty`, seen through any domains over it, when it is an array type.
fn array_element(ty: &tokio_postgres::types::Type) -> Option<&tokio_postgres::types::Type> {
    use tokio_postgres::types::Kind;
    match ty.kind() {
        Kind::Array(element) => Some(element),
        Kind::Domain(base) => array_element(base),
        _ => None,
    }
}

impl tokio_postgres::types::ToSql for FlexParam {
    fn to_sql(
        &self,
        ty: &tokio_postgres::types::Type,
        out: &mut bytes::BytesMut,
    ) -> std::result::Result<tokio_postgres::types::IsNull, Box<dyn std::error::Error + Sync + Send>>
    {
        use tokio_postgres::types::{IsNull, Type};
        match self {
            Self::Null => Ok(IsNull::Yes),
            Self::Text(s) => {
                write_text(s, ty, out)?;
                Ok(IsNull::No)
            },
            Self::Value(value) => {
                if *ty == Type::JSONB {
                    out.put_u8(1);
                    serde_json::to_writer((&mut *out).writer(), value)?;
                } else if *ty == Type::JSON {
                    serde_json::to_writer((&mut *out).writer(), value)?;
                } else if let (Some(element), serde_json::Value::Array(items)) =
                    (array_element(ty), value)
                {
                    let json_elements = [Type::JSONB, Type::JSON].contains(element);
                    let mut literal = String::new();
                    write_array_literal(items, json_elements, &mut literal);
                    out.extend_from_slice(literal.as_bytes());
                } else if let serde_json::Value::String(s) = value {
                    write_text(s, ty, out)?;
                } else {
                    // Another type reads the JSON text with its own input function, which
                    // accepts it (`9.5` for a `float8`) or refuses it with its own message.
                    write_text(&value.to_string(), ty, out)?;
                }
                Ok(IsNull::No)
            },
        }
    }

    fn accepts(_ty: &tokio_postgres::types::Type) -> bool {
        // Accepts all types; per-type serialisation is handled in `to_sql`.
        true
    }

    fn encode_format(&self, ty: &tokio_postgres::types::Type) -> tokio_postgres::types::Format {
        use tokio_postgres::types::{Format, Type};
        if [
            Type::JSONB,
            Type::JSON,
            Type::UUID,
            Type::INT4,
            Type::INT8,
            Type::BOOL,
        ]
        .contains(ty)
        {
            Format::Binary
        } else {
            Format::Text
        }
    }

    fn to_sql_checked(
        &self,
        ty: &tokio_postgres::types::Type,
        out: &mut bytes::BytesMut,
    ) -> std::result::Result<tokio_postgres::types::IsNull, Box<dyn std::error::Error + Sync + Send>>
    {
        // `accepts()` returns true for all types, so the standard WrongType check is
        // unnecessary.  Delegate directly to `to_sql`.
        self.to_sql(ty, out)
    }
}

/// Enrich a `FraiseQLError::Database` error for PostgreSQL SQLSTATE 42703 (undefined column)
/// when the WHERE clause contains `NativeField` conditions.
///
/// Native columns may be inferred automatically at compile time from `ID`/`UUID`-typed
/// arguments.  If the column does not exist on the target table at runtime, the raw
/// PostgreSQL error is replaced with a diagnostic message that names the native columns
/// involved and explains how to fix the schema.
fn enrich_undefined_column_error(
    err: FraiseQLError,
    view: &str,
    where_clause: Option<&WhereClause>,
) -> FraiseQLError {
    let FraiseQLError::Database { ref sql_state, .. } = err else {
        return err;
    };
    if sql_state.as_deref() != Some(PG_UNDEFINED_COLUMN) {
        return err;
    }
    let native_cols: Vec<&str> =
        where_clause.map(|wc| wc.native_column_names()).unwrap_or_default();
    if native_cols.is_empty() {
        return err;
    }
    FraiseQLError::Database {
        message:    format!(
            "Column(s) {:?} referenced as native column(s) on `{view}` do not exist. \
             These columns were auto-inferred from ID/UUID-typed query arguments. \
             Either add the column(s) to the table/view, or set \
             `native_columns = {{}}` explicitly in your schema to disable inference.",
            native_cols,
        ),
        sql_state:  Some(PG_UNDEFINED_COLUMN.to_string()),
        constraint: None,
    }
}

/// Decodes any PostgreSQL `ENUM` value as its text label.
///
/// `String`'s `FromSql` accepts only the text family (TEXT/VARCHAR/BPCHAR/NAME), not
/// a custom enum OID, so an enum column otherwise falls through [`row_to_map`]'s
/// decode ladder to `Null`. An enum's binary wire representation *is* its label as
/// UTF-8 text, so this reads it directly (#472).
struct PgEnumLabel(String);

impl<'a> FromSql<'a> for PgEnumLabel {
    fn from_sql(
        _ty: &Type,
        raw: &'a [u8],
    ) -> std::result::Result<Self, Box<dyn std::error::Error + Sync + Send>> {
        Ok(Self(std::str::from_utf8(raw)?.to_owned()))
    }

    fn accepts(ty: &Type) -> bool {
        matches!(ty.kind(), Kind::Enum(_))
    }
}

/// A cell's binary form, of any PostgreSQL type, decoded by [`decode_value`].
///
/// Read as `Option<RawCell>`, it also answers whether the cell is SQL `NULL`, once, before
/// any type is tried: each type's `FromSql` refuses a `NULL` with `WasNull`, so a `NULL` of
/// any type would otherwise be refused as not decoded (#1514).
struct RawCell<'a>(&'a [u8]);

impl<'a> FromSql<'a> for RawCell<'a> {
    fn from_sql(
        _ty: &Type,
        raw: &'a [u8],
    ) -> std::result::Result<Self, Box<dyn std::error::Error + Sync + Send>> {
        Ok(Self(raw))
    }

    fn accepts(_ty: &Type) -> bool {
        true
    }
}

/// Decodes a PostgreSQL `time` as the text PostgreSQL writes for it ([`time_text`]).
///
/// Not through `chrono::NaiveTime`: its `FromSql` adds the stored microseconds to midnight,
/// which wraps, so a stored `24:00:00` would be read as `00:00:00`; and its `Display` pads a
/// fraction to 3, 6 or 9 digits, where PostgreSQL trims it.
struct PgTimeText(String);

impl<'a> FromSql<'a> for PgTimeText {
    fn from_sql(
        _ty: &Type,
        raw: &'a [u8],
    ) -> std::result::Result<Self, Box<dyn std::error::Error + Sync + Send>> {
        let micros: [u8; 8] = raw.try_into().map_err(|_| "a time is 8 bytes")?;
        Ok(Self(time_text(i64::from_be_bytes(micros))))
    }

    fn accepts(ty: &Type) -> bool {
        *ty == Type::TIME
    }
}

/// Decodes a PostgreSQL `timetz` as the text PostgreSQL writes for it: its time of day
/// ([`time_text`]), then its zone ([`zone_text`]).
struct PgTimeTzText(String);

impl<'a> FromSql<'a> for PgTimeTzText {
    fn from_sql(
        _ty: &Type,
        raw: &'a [u8],
    ) -> std::result::Result<Self, Box<dyn std::error::Error + Sync + Send>> {
        let (micros, zone) = raw.split_at_checked(8).ok_or("a timetz is 12 bytes")?;
        let micros: [u8; 8] = micros.try_into().map_err(|_| "a timetz is 12 bytes")?;
        let zone: [u8; 4] = zone.try_into().map_err(|_| "a timetz is 12 bytes")?;
        // The wire carries the zone in seconds west of UTC; its text, east.
        let east = -i32::from_be_bytes(zone);
        Ok(Self(time_text(i64::from_be_bytes(micros)) + &zone_text(east)))
    }

    fn accepts(ty: &Type) -> bool {
        *ty == Type::TIMETZ
    }
}

/// A time of day `micros` microseconds after midnight, as PostgreSQL writes it: `HH:MM:SS`,
/// then the fraction with its trailing zeros dropped, if any. `24:00:00`, which a `time`
/// holds, is written as such.
pub(super) fn time_text(micros: i64) -> String {
    let (seconds, fraction) = (micros.div_euclid(1_000_000), micros.rem_euclid(1_000_000));
    let mut text = format!("{:02}:{:02}:{:02}", seconds / 3600, seconds % 3600 / 60, seconds % 60);
    if fraction != 0 {
        let digits = format!("{fraction:06}");
        text.push('.');
        text.push_str(digits.trim_end_matches('0'));
    }
    text
}

/// A zone `east` seconds east of UTC, as PostgreSQL writes it: a sign and the hours, then
/// the minutes when they or the seconds are not zero, then the seconds when they are not.
pub(super) fn zone_text(east: i32) -> String {
    let sign = if east < 0 { '-' } else { '+' };
    let east = east.unsigned_abs();
    let (hours, minutes, seconds) = (east / 3600, east % 3600 / 60, east % 60);
    match (minutes, seconds) {
        (0, 0) => format!("{sign}{hours:02}"),
        (_, 0) => format!("{sign}{hours:02}:{minutes:02}"),
        _ => format!("{sign}{hours:02}:{minutes:02}:{seconds:02}"),
    }
}

/// Why [`decode_cell`] does not decode a cell of some type, and how a view can cast the column
/// so that it is decoded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Refusal {
    /// Why no rendering of the type is served.
    pub(super) reason: &'static str,
    /// The casts that give the column a type that is decoded.
    pub(super) cast:   &'static str,
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}; cast it in the view ({})", self.reason, self.cast)
    }
}

/// The PostgreSQL types whose name is all their refusal needs: their binary form is a system
/// type's, or their text depends on a session setting.
const REFUSED_BY_NAME: &[(&[&str], Refusal)] = &[
    (
        &["interval"],
        Refusal {
            reason: "an interval's text depends on the session's IntervalStyle, so no one rendering \
                 of it is right",
            cast:   "`::text`, `to_char`, `extract`",
        },
    ),
    (
        &["bytea"],
        Refusal {
            reason: "a bytea's text depends on the session's bytea_output, so no one rendering of \
                 it is right",
            cast:   "`encode(col, 'base64')` or `encode(col, 'hex')`",
        },
    ),
    (
        &["money"],
        Refusal {
            reason: "a money's text depends on the session's lc_monetary, so no one rendering of it \
                 is right",
            cast:   "`::numeric`",
        },
    ),
    (
        &[
            "regclass",
            "regcollation",
            "regconfig",
            "regdictionary",
            "regnamespace",
            "regoper",
            "regoperator",
            "regproc",
            "regprocedure",
            "regrole",
            "regtype",
        ],
        Refusal {
            reason: "an object identifier's name depends on the session's search_path, so no one \
                     rendering of it is right",
            cast:   "`::oid`",
        },
    ),
    (
        &["point", "line", "lseg", "box", "path", "polygon", "circle"],
        Refusal {
            reason: "a geometric value's coordinates are written to the session's \
                 extra_float_digits, so no one rendering of it is right",
            cast:   "its coordinates, or `::text`",
        },
    ),
    (
        &["tsvector", "tsquery"],
        Refusal {
            reason: "a text-search value is not decoded",
            cast:   "`::text`",
        },
    ),
    (
        &["txid_snapshot", "pg_snapshot", "int2vector", "oidvector"],
        Refusal {
            reason: "a system type is not decoded",
            cast:   "`::text`",
        },
    ),
];

/// A range or multirange, whatever its bounds' type.
const RANGE_REFUSAL: Refusal = Refusal {
    reason: "a range is written with its bounds' text, which for a range of timestamps follows \
             the session's TimeZone, so ranges are not decoded",
    cast:   "`lower(col)` and `upper(col)`, or `::text`",
};

/// A composite value, or an anonymous `record`.
const COMPOSITE_REFUSAL: Refusal = Refusal {
    reason: "a composite value is not decoded",
    cast:   "`to_jsonb(col)`",
};

/// Any other type no rung of [`decode_cell`] decodes.
const UNKNOWN_REFUSAL: Refusal = Refusal {
    reason: "no decoder reads this type",
    cast:   "`to_jsonb(col)`, or `::text`",
};

/// The refusal [`REFUSED_BY_NAME`] holds for `ty`'s own name, whatever its kind:
/// `int2vector` and `oidvector` are arrays to the driver, and refused as system types. The
/// names are `pg_catalog`'s: a type of the user's named `interval` or `money` is not the
/// built-in, and gets its own kind's handling. Only a superuser allowed to modify the system
/// catalogs can add a type to `pg_catalog`.
fn refused_by_name(ty: &Type) -> Option<Refusal> {
    if ty.schema() != "pg_catalog" {
        return None;
    }
    REFUSED_BY_NAME
        .iter()
        .find(|(names, _)| names.contains(&ty.name()))
        .map(|(_, refusal)| *refusal)
}

/// Why [`decode_cell`] does not decode a cell of `ty`, consulted only for a cell no rung
/// decoded. An array is refused for its element's reason.
pub(super) fn refusal(ty: &Type) -> Refusal {
    if let Some(refusal) = refused_by_name(ty) {
        return refusal;
    }
    match ty.kind() {
        Kind::Array(element) => refusal(element),
        Kind::Range(_) | Kind::Multirange(_) => RANGE_REFUSAL,
        Kind::Composite(_) => COMPOSITE_REFUSAL,
        _ if *ty == Type::RECORD => COMPOSITE_REFUSAL,
        _ => UNKNOWN_REFUSAL,
    }
}

/// `ty` as a view's author writes it: an array as its element's name and `[]`, a type outside
/// `pg_catalog` qualified by its schema. `int2vector` and `oidvector`, arrays to the driver,
/// keep their own names.
pub(super) fn type_name(ty: &Type) -> String {
    match ty.kind() {
        Kind::Array(element) if ty.name().starts_with('_') => format!("{}[]", type_name(element)),
        _ => ty.to_string(),
    }
}

/// A cell [`decode_cell`] does not decode: its column, its type and why, never its value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Undecoded {
    /// The column's name.
    pub(super) column:  String,
    /// The column's type, as [`type_name`] writes it.
    pub(super) pg_type: String,
    /// Why it is not decoded.
    pub(super) refusal: Refusal,
}

impl std::fmt::Display for Undecoded {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "column `{}` of PostgreSQL type `{}` is not decoded: {}",
            self.column, self.pg_type, self.refusal
        )
    }
}

impl From<Undecoded> for FraiseQLError {
    fn from(undecoded: Undecoded) -> Self {
        Self::Database {
            message:    undecoded.to_string(),
            sql_state:  None,
            constraint: None,
        }
    }
}

/// Convert a single `tokio_postgres::Row` into a `HashMap<String, serde_json::Value>`.
///
/// Each cell through [`decode_cell`].
///
/// # Errors
///
/// Returns `FraiseQLError::Database` naming the first column whose cell is not decoded.
fn row_to_map(row: &Row) -> Result<std::collections::HashMap<String, serde_json::Value>> {
    let mut map = std::collections::HashMap::new();
    for (idx, column) in row.columns().iter().enumerate() {
        map.insert(column.name().to_string(), decode_cell(row, idx)?);
    }
    Ok(map)
}

/// [`row_to_map`] for a mutation's response row, each document column `decimals` reads
/// keeping its `Decimal`s' digits; the row's `entity_type` stamps the type its entity and
/// its `error_detail` are read as.
fn write_row_to_map(
    row: &Row,
    decimals: Option<&crate::traits::WriteDecimals>,
) -> Result<std::collections::HashMap<String, serde_json::Value>> {
    let Some(decimals) = decimals else {
        return row_to_map(row);
    };
    let stamp = row.try_get::<_, Option<String>>("entity_type").ok().flatten();
    let mut map = std::collections::HashMap::new();
    for (idx, column) in row.columns().iter().enumerate() {
        let read = decimals.column(column.name(), stamp.as_deref());
        let cell = match &read {
            Some(read) => json_column(row, idx, Decimals::column(read))
                .unwrap_or_else(|| decode_cell(row, idx).map_err(Into::into))?,
            None => decode_cell(row, idx)?,
        };
        map.insert(column.name().to_string(), cell);
    }
    Ok(map)
}

/// A `json`/`jsonb` cell, each number at a `Decimal` position of `decimals` kept as a string
/// of its digits; `None` when the cell is not JSON.
fn json_column<I>(row: &Row, column: I, decimals: Decimals<'_>) -> Option<Result<serde_json::Value>>
where
    I: tokio_postgres::row::RowIndex + std::fmt::Display + Copy,
{
    let cell = match row.try_get::<_, Option<JsonText<'_>>>(column).ok()? {
        Some(JsonText(text)) => decimals.parse(text).map_err(|e| FraiseQLError::Database {
            message:    format!("Failed to parse the `{column}` column as JSON: {e}"),
            sql_state:  None,
            constraint: None,
        }),
        None => Ok(serde_json::Value::Null),
    };
    Some(cell)
}

/// A row of a row-shaped read, in `columns`' order, each `Decimal` column kept as the exact
/// text PostgreSQL printed. The one decode the four row reads (collecting and streaming,
/// with and without session variables) share, so none of them can drift back to
/// [`row_to_map`]'s, which rounds a `NUMERIC` through an `f64`.
fn row_to_columns(
    row: &Row,
    columns: &[crate::types::ColumnSpec],
) -> Result<Vec<crate::types::ColumnValue>> {
    let exact: Vec<&str> = columns
        .iter()
        .filter(|c| c.column_type == crate::dialect::RowViewColumnType::Decimal)
        .map(|c| c.name.as_str())
        .collect();
    crate::traits::row_to_column_values(&row_to_map_exact(row, &exact)?, columns)
}

/// [`row_to_map`], with each `NUMERIC` column named in `exact` kept as a string of the
/// digits PostgreSQL printed: re-parsed as a JSON number, it would round through an `f64`.
///
/// # Errors
///
/// Returns `FraiseQLError::Database` naming the first column whose cell is not decoded.
fn row_to_map_exact(
    row: &Row,
    exact: &[&str],
) -> Result<std::collections::HashMap<String, serde_json::Value>> {
    let mut map = std::collections::HashMap::new();
    for (idx, column) in row.columns().iter().enumerate() {
        let digits = exact
            .contains(&column.name())
            .then(|| row.try_get::<_, Option<PgNumericText>>(idx).ok().flatten())
            .flatten();
        let cell = match digits {
            Some(digits) => serde_json::Value::String(digits.0),
            None => decode_cell(row, idx)?,
        };
        map.insert(column.name().to_string(), cell);
    }
    Ok(map)
}

/// Decode one cell of a `tokio_postgres::Row` to JSON.
///
/// The positional reader the admin SQL console needs (#962) and the name-keyed map every
/// other caller wants share it, so they cannot disagree about what a `numeric`, an enum or a
/// `timestamptz` becomes. A console keyed by column *name* would also collapse `SELECT 1 AS
/// a, 2 AS a` to one column and silently drop the other.
///
/// A SQL `NULL` of any type is `null`; any other cell is [`decode_value`]'s.
///
/// # Errors
///
/// A present cell of a type that is not decoded is refused as [`Undecoded`], naming its
/// column, its type and the [`refusal`], never the value: it is never served as `null`, which
/// would read as a value the database does not hold.
pub(super) fn decode_cell(
    row: &Row,
    idx: usize,
) -> std::result::Result<serde_json::Value, Undecoded> {
    let column = &row.columns()[idx];
    let decoded = match row.try_get::<_, Option<RawCell<'_>>>(idx) {
        // SQL NULL, whatever the column's type: decided once, before any type is tried (#1514).
        Ok(None) => Ok(serde_json::Value::Null),
        Ok(Some(RawCell(raw))) => decode_value(column.type_(), raw),
        Err(_) => Err(refusal(column.type_())),
    };
    decoded.map_err(|refusal| Undecoded {
        column: column.name().to_string(),
        pg_type: type_name(column.type_()),
        refusal,
    })
}

/// `raw` read as `T`, when `T` reads `ty`.
fn read<'a, T: FromSql<'a>>(ty: &Type, raw: &'a [u8]) -> Option<T> {
    if T::accepts(ty) {
        T::from_sql(ty, raw).ok()
    } else {
        None
    }
}

/// One present value of `ty`, from its binary form, rendered as `to_jsonb` renders it where
/// that rendering does not depend on a session setting. A cell and each element of an array
/// are decoded alike.
///
/// Each type is tried in turn; every `FromSql` here accepts only its own types, so their
/// order decides no value, only how soon a common type is found.
///
/// # Errors
///
/// The [`refusal`] for a type that is not decoded; for an array, its element's.
fn decode_value(ty: &Type, raw: &[u8]) -> std::result::Result<serde_json::Value, Refusal> {
    let value = if let Some(v) = read::<i16>(ty, raw) {
        // SMALLINT/int2 columns (e.g. `app.mutation_response.http_status`): `i32` does not
        // read an int2, which once dropped `MutationError`'s `httpStatus` to absent.
        serde_json::json!(v)
    } else if let Some(v) = read::<i32>(ty, raw) {
        serde_json::json!(v)
    } else if let Some(v) = read::<i64>(ty, raw) {
        serde_json::json!(v)
    } else if let Some(v) = read::<f64>(ty, raw) {
        serde_json::json!(v)
    } else if let Some(v) = read::<String>(ty, raw) {
        serde_json::Value::String(v)
    } else if let Some(v) = read::<bool>(ty, raw) {
        serde_json::json!(v)
    } else if let Some(v) = read::<PgNumericText>(ty, raw) {
        // NUMERIC/DECIMAL — PostgreSQL's exact text (#980), re-parsed as a JSON number so
        // the value is not forced through an f64 at construction (H35). NaN, Infinity and
        // -Infinity are not JSON numbers, and stay strings.
        let s = v.0;
        serde_json::from_str::<serde_json::Value>(&s).unwrap_or(serde_json::Value::String(s))
    } else if let Some(v) = read::<uuid::Uuid>(ty, raw) {
        // UUID columns (e.g. `app.mutation_response.entity_id`): the hyphenated text (H35).
        serde_json::Value::String(v.to_string())
    } else if let Some(v) = read::<chrono::DateTime<chrono::Utc>>(ty, raw) {
        // `timestamptz` → RFC 3339 / ISO 8601 text (H35).
        serde_json::Value::String(v.to_rfc3339())
    } else if let Some(v) = read::<chrono::NaiveDateTime>(ty, raw) {
        // `timestamp` (no tz) → a local date-time: `T`, no offset, the fraction in 3, 6 or
        // 9 digits as `to_rfc3339` writes the `timestamptz` one. `to_string` wrote a space,
        // which no date-time reader accepts.
        serde_json::Value::String(v.format("%Y-%m-%dT%H:%M:%S%.f").to_string())
    } else if let Some(v) = read::<chrono::NaiveDate>(ty, raw) {
        // `date` → ISO 8601 date text (H35).
        serde_json::Value::String(v.to_string())
    } else if let Some(v) = read::<PgTimeText>(ty, raw) {
        // `time` → PostgreSQL's own text, as the document path carries it.
        serde_json::Value::String(v.0)
    } else if let Some(v) = read::<PgTimeTzText>(ty, raw) {
        // `timetz` → PostgreSQL's own text, its zone included.
        serde_json::Value::String(v.0)
    } else if let Some(v) = read::<PgEnumLabel>(ty, raw) {
        // ENUM columns (e.g. `app.mutation_response.error_class`) → their label. `String`
        // does not read a custom enum OID, which once dropped `error_class` to absent and
        // left a failed mutation's typed error arm unreachable (#472).
        serde_json::Value::String(v.0)
    } else if let Some(v) = read::<serde_json::Value>(ty, raw) {
        v
    } else if let Some(v) = read::<f32>(ty, raw) {
        float4_json(v)
    } else if let Some(v) = read::<u32>(ty, raw) {
        // `oid` → its decimal text, a string as `to_jsonb` writes it.
        serde_json::Value::String(v.to_string())
    } else if let Some(v) = read::<i8>(ty, raw) {
        // `"char"` → its one byte.
        serde_json::Value::String(char_text(v.to_ne_bytes()[0]))
    } else if let Some(PgText(v)) = read::<PgText>(ty, raw) {
        // `inet`, `cidr`, `macaddr`, `macaddr8`, `bit`, `varbit`, `pg_lsn`, `xml` → the text
        // PostgreSQL writes.
        serde_json::Value::String(v)
    } else {
        return match ty.kind() {
            // An array, not `int2vector` or `oidvector`: those are arrays to the driver,
            // and refused as system types.
            Kind::Array(element) if ty.name().starts_with('_') => decode_array(element, raw),
            // pgvector's `vector`, `halfvec`, `sparsevec` → their text, a string as `to_jsonb`
            // writes it and as a document carrying `embedding::text` holds it. Last, so no
            // other cell pays for the names.
            _ => read::<PgVectorText>(ty, raw)
                .map(|PgVectorText(v)| serde_json::Value::String(v))
                .ok_or_else(|| refusal(ty)),
        };
    };
    Ok(value)
}

/// A `float4` as `to_jsonb` writes it: its shortest text that reads back as the same
/// `float4` (PostgreSQL's at its default `extra_float_digits`), as a JSON number; zero
/// without its sign; `NaN` and the infinities as strings.
fn float4_json(v: f32) -> serde_json::Value {
    if v.is_nan() {
        serde_json::Value::String("NaN".to_string())
    } else if v.is_infinite() {
        let sign = if v.is_sign_negative() { "-" } else { "" };
        serde_json::Value::String(format!("{sign}Infinity"))
    } else if v == 0.0 {
        serde_json::json!(0)
    } else {
        let text = v.to_string();
        serde_json::from_str(&text).unwrap_or(serde_json::Value::String(text))
    }
}

/// A `"char"`'s byte as PostgreSQL writes it: none for a zero byte, an ASCII byte as itself,
/// any other as a backslash and three octal digits.
fn char_text(byte: u8) -> String {
    match byte {
        0 => String::new(),
        1..=127 => char::from(byte).to_string(),
        _ => format!("\\{byte:03o}"),
    }
}

/// The types whose binary form is read into the text PostgreSQL writes for them, which no
/// session setting changes: `inet`, `cidr`, `macaddr`, `macaddr8`, `bit`, `varbit`,
/// `pg_lsn` and `xml`.
struct PgText(String);

impl<'a> FromSql<'a> for PgText {
    fn from_sql(
        ty: &Type,
        raw: &'a [u8],
    ) -> std::result::Result<Self, Box<dyn std::error::Error + Sync + Send>> {
        let text = if *ty == Type::INET || *ty == Type::CIDR {
            inet_text(raw)
        } else if *ty == Type::MACADDR || *ty == Type::MACADDR8 {
            Some(mac_text(raw))
        } else if *ty == Type::BIT || *ty == Type::VARBIT {
            bit_text(raw)
        } else if *ty == Type::PG_LSN {
            let lsn = u64::from_be_bytes(raw.try_into()?);
            Some(format!("{:X}/{:X}", lsn >> 32, lsn & 0xFFFF_FFFF))
        } else {
            Some(std::str::from_utf8(raw)?.to_owned())
        };
        text.map(Self).ok_or_else(|| format!("a malformed {ty}").into())
    }

    fn accepts(ty: &Type) -> bool {
        [
            Type::INET,
            Type::CIDR,
            Type::MACADDR,
            Type::MACADDR8,
            Type::BIT,
            Type::VARBIT,
            Type::PG_LSN,
            Type::XML,
        ]
        .contains(ty)
    }
}

/// pgvector's `vector`, `halfvec` and `sparsevec`, read into the text pgvector writes for
/// them, which no session setting changes: pgvector formats its floats itself, not through
/// `float4out`, so `extra_float_digits` does not reach them.
///
/// The extension's types have no fixed OID, so they are known by name. Resolving their OIDs
/// when the pool starts would change how every pool is set up to guard against a type that
/// is hypothetical: a base type of the same name from another extension. Its binary form
/// would also have to fit the one read here to the byte, header, length and, for a
/// `sparsevec`, ascending in-range indices, or the cell is refused as before. A composite or
/// domain of the same name is not a base type, and is not read here.
pub(super) struct PgVectorText(String);

impl<'a> FromSql<'a> for PgVectorText {
    fn from_sql(
        ty: &Type,
        raw: &'a [u8],
    ) -> std::result::Result<Self, Box<dyn std::error::Error + Sync + Send>> {
        let text = match ty.name() {
            "vector" => dense_vector_text(raw, 4, |b| Some(f32::from_be_bytes(b.try_into().ok()?))),
            "halfvec" => {
                dense_vector_text(raw, 2, |b| half_to_f32(u16::from_be_bytes(b.try_into().ok()?)))
            },
            _ => sparse_vector_text(raw),
        };
        text.map(Self).ok_or_else(|| format!("a malformed {ty}").into())
    }

    fn accepts(ty: &Type) -> bool {
        matches!(ty.kind(), Kind::Simple) && matches!(ty.name(), "vector" | "halfvec" | "sparsevec")
    }
}

/// A `vector` or `halfvec`: an `i16` dimension count, two unused bytes, then that many
/// elements of `width` bytes, written `[a,b,…]`.
pub(super) fn dense_vector_text(
    raw: &[u8],
    width: usize,
    element: impl Fn(&[u8]) -> Option<f32>,
) -> Option<String> {
    let (&[d0, d1, _, _], elements) = raw.split_first_chunk::<4>()?;
    let dimensions = usize::try_from(i16::from_be_bytes([d0, d1])).ok()?;
    if elements.len() != dimensions * width {
        return None;
    }
    let mut text = String::with_capacity(2 + dimensions * 12);
    let mut scratch = String::new();
    text.push('[');
    for (i, bytes) in elements.chunks_exact(width).enumerate() {
        if i != 0 {
            text.push(',');
        }
        push_vector_float(&mut text, &mut scratch, element(bytes)?);
    }
    text.push(']');
    Some(text)
}

/// A `sparsevec`: `i32` dimension count, non-zero count and an unused word, then the
/// non-zero entries' 0-based indices, then their `float4` values, written `{i:v,…}/d` with
/// 1-based indices.
pub(super) fn sparse_vector_text(raw: &[u8]) -> Option<String> {
    let word = |at: usize| -> Option<i32> {
        Some(i32::from_be_bytes(raw.get(at..at + 4)?.try_into().ok()?))
    };
    let dimensions = usize::try_from(word(0)?).ok()?;
    let entries = usize::try_from(word(4)?).ok()?;
    if raw.len() != 12 + entries * 8 {
        return None;
    }
    let mut text = String::from("{");
    let mut scratch = String::new();
    let mut previous: Option<usize> = None;
    for entry in 0..entries {
        let index = usize::try_from(word(12 + entry * 4)?).ok()?;
        if index >= dimensions || previous.is_some_and(|p| index <= p) {
            return None;
        }
        previous = Some(index);
        let value = f32::from_bits(u32::from_be_bytes(
            raw.get(12 + entries * 4 + entry * 4..)?.get(..4)?.try_into().ok()?,
        ));
        if entry != 0 {
            text.push(',');
        }
        let _ = write!(text, "{}:", index + 1);
        push_vector_float(&mut text, &mut scratch, value);
    }
    let _ = write!(text, "}}/{dimensions}");
    Some(text)
}

/// A half-precision float widened to `float4`, exactly; `None` for an infinity or a NaN,
/// which pgvector never stores.
pub(super) fn half_to_f32(half: u16) -> Option<f32> {
    let sign = if half & 0x8000 == 0 { 1.0 } else { -1.0 };
    let exponent = u32::from(half >> 10 & 0x1F);
    let fraction = u32::from(half & 0x3FF);
    match exponent {
        // Zero and the subnormals: `fraction` × 2⁻²⁴, exact in a `float4`.
        0 => Some(sign * f32::from(u16::try_from(fraction).ok()?) * f32::from_bits(0x3380_0000)),
        0x1F => None,
        _ => Some(f32::from_bits(
            u32::from(half & 0x8000) << 16 | (exponent + 112) << 23 | fraction << 13,
        )),
    }
}

/// A `float4` as pgvector writes it, onto `out`: the shortest digits that read back as the
/// same value, in `float4`'s notation, exponential below 1e-4 and from 1e6 (`1e+06`,
/// `1.5e-05`, a signed exponent of at least two digits), else fixed; a negative zero as
/// `-0`. `scratch` holds the digits, one buffer for every element of a vector.
pub(super) fn push_vector_float(out: &mut String, scratch: &mut String, v: f32) {
    if v == 0.0 {
        out.push_str(if v.is_sign_negative() { "-0" } else { "0" });
        return;
    }
    scratch.clear();
    let _ = write!(scratch, "{v:e}");
    let (mantissa, exponent) = scratch.split_once('e').unwrap_or((scratch.as_str(), "0"));
    let exponent: i32 = exponent.parse().unwrap_or(0);
    if let Some(magnitude) = mantissa.strip_prefix('-') {
        out.push('-');
        push_float_digits(out, magnitude, exponent);
    } else {
        push_float_digits(out, mantissa, exponent);
    }
}

/// `d.ddd` × 10^`exponent` in `float4`'s notation; see [`push_vector_float`].
fn push_float_digits(out: &mut String, mantissa: &str, exponent: i32) {
    let (first, rest) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    if !(-4..6).contains(&exponent) {
        out.push_str(first);
        if !rest.is_empty() {
            out.push('.');
            out.push_str(rest);
        }
        let exponent_sign = if exponent < 0 { '-' } else { '+' };
        let _ = write!(out, "e{exponent_sign}{:02}", exponent.unsigned_abs());
    } else if exponent < 0 {
        out.push_str("0.");
        for _ in 1..-exponent {
            out.push('0');
        }
        out.push_str(first);
        out.push_str(rest);
    } else {
        // The digits before the point, after the first.
        let whole = usize::try_from(exponent).unwrap_or(0);
        out.push_str(first);
        if rest.len() <= whole {
            out.push_str(rest);
            for _ in rest.len()..whole {
                out.push('0');
            }
        } else {
            out.push_str(&rest[..whole]);
            out.push('.');
            out.push_str(&rest[whole..]);
        }
    }
}

/// An `inet` or `cidr` as PostgreSQL writes it: the address, then `/` and the prefix length
/// for a `cidr`, and for an `inet` whose prefix is not the whole address.
fn inet_text(raw: &[u8]) -> Option<String> {
    let (&[family, bits, is_cidr, length], address) = raw.split_first_chunk::<4>()?;
    let (mut text, whole) = match (family, length, address) {
        (2, 4, [a, b, c, d]) => (format!("{a}.{b}.{c}.{d}"), 32),
        (3, 16, address) => (ipv6_text(address.try_into().ok()?), 128),
        _ => return None,
    };
    if is_cidr != 0 || bits != whole {
        let _ = write!(text, "/{bits}");
    }
    Some(text)
}

/// An IPv6 address as PostgreSQL writes it: hexadecimal groups without leading zeros, the
/// first longest run of two or more zero groups written `::`, and the last four bytes in
/// dotted form for an address that embeds an IPv4 one (`::a.b.c.d`, `::ffff:a.b.c.d`).
fn ipv6_text(address: [u8; 16]) -> String {
    let words: Vec<u16> =
        address.chunks_exact(2).map(|w| u16::from_be_bytes([w[0], w[1]])).collect();
    let mut best: Option<(usize, usize)> = None;
    let mut run: Option<(usize, usize)> = None;
    for (i, &word) in words.iter().chain(std::iter::once(&1)).enumerate() {
        if word == 0 {
            run = Some(run.map_or((i, 1), |(start, len)| (start, len + 1)));
        } else if let Some(zeros) = run.take() {
            if best.is_none_or(|(_, len)| zeros.1 > len) {
                best = Some(zeros);
            }
        }
    }
    let best = best.filter(|&(_, len)| len >= 2);
    let embeds_ipv4 = best.is_some_and(|(start, len)| {
        start == 0 && (len == 6 || (len == 7 && words[7] != 1) || (len == 5 && words[5] == 0xFFFF))
    });
    let mut text = String::new();
    for (i, word) in words.iter().enumerate() {
        if let Some((start, len)) = best {
            if (start..start + len).contains(&i) {
                if i == start {
                    text.push(':');
                }
                continue;
            }
        }
        if i != 0 {
            text.push(':');
        }
        if i == 6 && embeds_ipv4 {
            let _ = write!(text, "{}.{}.{}.{}", address[12], address[13], address[14], address[15]);
            return text;
        }
        let _ = write!(text, "{word:x}");
    }
    if best.is_some_and(|(start, len)| start + len == 8) {
        text.push(':');
    }
    text
}

/// A `macaddr` or `macaddr8`: its bytes in lowercase hexadecimal, joined by `:`.
fn mac_text(raw: &[u8]) -> String {
    raw.iter().map(|byte| format!("{byte:02x}")).collect::<Vec<_>>().join(":")
}

/// A `bit` or `varbit`: its length in bits, then its bits, most significant first.
fn bit_text(raw: &[u8]) -> Option<String> {
    let (length, bytes) = raw.split_first_chunk::<4>()?;
    let length = usize::try_from(i32::from_be_bytes(*length)).ok()?;
    (bytes.len() * 8 >= length).then(|| {
        (0..length)
            .map(|i| {
                if bytes[i / 8] & (0x80 >> (i % 8)) == 0 {
                    '0'
                } else {
                    '1'
                }
            })
            .collect()
    })
}

/// An array as `to_jsonb` writes it: nested JSON arrays, one level per dimension, whatever
/// its lower bounds; a `NULL` element as `null`; each other element as [`decode_value`]
/// decodes its type.
///
/// # Errors
///
/// The element type's [`refusal`], when it is not decoded or the array is malformed.
fn decode_array(element: &Type, raw: &[u8]) -> std::result::Result<serde_json::Value, Refusal> {
    let malformed = || refusal(element);
    let mut rest = raw;
    let mut take = || -> std::result::Result<i32, Refusal> {
        let (head, tail) = rest.split_first_chunk::<4>().ok_or_else(malformed)?;
        rest = tail;
        Ok(i32::from_be_bytes(*head))
    };
    let dimensions = take()?;
    let _has_nulls = take()?;
    let _element_oid = take()?;
    let mut lengths = Vec::new();
    for _ in 0..dimensions {
        lengths.push(usize::try_from(take()?).map_err(|_| malformed())?);
        let _lower_bound = take()?;
    }
    if lengths.is_empty() {
        return Ok(serde_json::Value::Array(Vec::new()));
    }
    nest(element, &lengths, &mut rest)
}

/// The elements of one level of an array, of `lengths[0]` items each nesting the levels
/// below, read from `elements` in order.
fn nest(
    element: &Type,
    lengths: &[usize],
    elements: &mut &[u8],
) -> std::result::Result<serde_json::Value, Refusal> {
    let malformed = || refusal(element);
    let (&length, below) = lengths.split_first().ok_or_else(malformed)?;
    let mut items = Vec::with_capacity(length);
    for _ in 0..length {
        if !below.is_empty() {
            items.push(nest(element, below, elements)?);
            continue;
        }
        let (size, rest) = elements.split_first_chunk::<4>().ok_or_else(malformed)?;
        let size = i32::from_be_bytes(*size);
        *elements = rest;
        if size < 0 {
            items.push(serde_json::Value::Null);
            continue;
        }
        let size = usize::try_from(size).map_err(|_| malformed())?;
        let (value, rest) = elements.split_at_checked(size).ok_or_else(malformed)?;
        *elements = rest;
        items.push(decode_value(element, value)?);
    }
    Ok(serde_json::Value::Array(items))
}

/// Apply transaction-local session variables on an in-progress transaction.
///
/// Every `(name, value)` pair is set with `set_config(name, value, true)`, all of them in one
/// `SELECT`, so the values are scoped to `txn` and visible to every statement run on it (fixes
/// #329), for one round trip whatever their number. The caller is responsible for committing or
/// rolling back.
pub(super) async fn apply_session_vars(
    txn: &deadpool_postgres::Transaction<'_>,
    session_vars: &[(&str, &str)],
) -> Result<()> {
    if session_vars.is_empty() {
        return Ok(());
    }
    let (sql, params) = set_config_batch(session_vars);
    // Prepared once per connection. Names and values are parameters, so the text depends only
    // on how many variables are set and which carry the clock directive: a handful of texts per
    // schema, none a client can mint, so unlike a read's it needs no cap.
    let applied = match txn.prepare_cached(&sql).await {
        Ok(statement) => txn.execute(&statement, &params).await,
        Err(e) => Err(e),
    };
    applied.map_err(|e| {
        let names: Vec<&str> = session_vars.iter().map(|(name, _)| *name).collect();
        crate::postgres::database_error(
            format!("set_config({names:?}) failed: {}", pg_detail(&e)),
            &e,
        )
    })?;
    Ok(())
}

/// One `SELECT set_config(…), set_config(…), …` setting every pair transaction-locally, and its
/// parameters. Names and values are always parameters, never SQL text.
///
/// A var carrying the clock-timestamp directive (e.g. `fraiseql.started_at`) is stamped with the
/// DB clock at apply time, on this very transaction, so the start timestamp and the
/// close-of-interval at the outbox write share one clock (no app↔DB skew).
fn set_config_batch<'a>(
    session_vars: &'a [(&'a str, &'a str)],
) -> (String, Vec<&'a (dyn tokio_postgres::types::ToSql + Sync)>) {
    let mut sql = String::from("SELECT ");
    let mut params: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> =
        Vec::with_capacity(session_vars.len() * 2);
    for (index, (name, value)) in session_vars.iter().enumerate() {
        if index > 0 {
            sql.push_str(", ");
        }
        params.push(name);
        let name_param = params.len();
        if *value == crate::changelog::CLOCK_TIMESTAMP_DIRECTIVE {
            let _ = write!(sql, "set_config(${name_param}, clock_timestamp()::text, true)");
        } else {
            params.push(value);
            let _ = write!(sql, "set_config(${name_param}, ${}, true)", name_param + 1);
        }
    }
    (sql, params)
}

/// Build the single statement that runs a mutation function and writes its
/// `core.tb_entity_change_log` outbox row in the same transaction (the Change
/// Spine transactional outbox).
///
/// The function call is materialised once (`WITH r AS MATERIALIZED`, so a
/// volatile mutation function executes **exactly once** even though two CTEs
/// read it), a data-modifying CTE INSERTs the change-log row for an effective
/// change (`succeeded AND state_changed`), and the primary `SELECT * FROM r`
/// returns the function's row unchanged to the caller. The changed-entity
/// columns (`object_id`, `object_data`, `updated_fields`, `cascade`) come from
/// the function's own `app.mutation_response` row; `$<n+1>` is the NOT-NULL
/// `object_type` fallback, `$<n+2>` is the `modification_type` verb,
/// `$<n+3>` is the envelope `tenant_id` (the Trinity public-facing UUID, NULL
/// for system / unauthenticated / non-UUID-tenant rows), `$<n+4>` is the
/// `trace_id` (the request's W3C trace id, plain text), `$<n+5>` is the
/// `schema_version` (the compiled schema's content hash, plain text), `$<n+6>`
/// is the `trace_context` (the full W3C trace context, cast `::jsonb`), and
/// `$<n+9>` is the ingress `transport` (#376), merged into `extra_metadata` as
/// `{"transport": …}` by a deterministic `CASE` (NULL omits the key, so the SQL
/// text never depends on the value), where `n` = `n_args` (the number of
/// function arguments). The envelope params are
/// appended **after** the function args so the SQL text stays deterministic per
/// `(function, n_args)` — `prepare_cached` keys the statement by text, so the
/// column list and placeholder positions must never depend on the param *values*.
///
/// `started_at`/`duration_ms` read `fraiseql.started_at` (set txn-locally by the
/// caller) on the DB clock — the canonical computation from
/// [`crate::changelog::duration_ms_sql`], stamped with
/// [`crate::changelog::DURATION_CALC_VERSION`]. `commit_time` is the DB clock at
/// INSERT; `seq` is omitted so the column's `SEQUENCE` default fires (so any
/// INSERTer, incl. cooperative external producers, gets a monotonic value).
pub(super) fn build_changelog_cte_sql(quoted_fn: &str, n_args: usize, pre_image: bool) -> String {
    let placeholders: Vec<String> = (1..=n_args).map(|i| format!("${i}")).collect();
    let object_type_idx = n_args + 1;
    let modification_type_idx = n_args + 2;
    let tenant_id_idx = n_args + 3;
    let trace_id_idx = n_args + 4;
    let schema_version_idx = n_args + 5;
    let trace_context_idx = n_args + 6;
    let actor_type_idx = n_args + 7;
    let acting_for_idx = n_args + 8;
    let transport_idx = n_args + 9;
    let started_var = crate::changelog::STARTED_AT_VAR;
    let duration = crate::changelog::duration_ms_sql(started_var);
    let calc_version = crate::changelog::DURATION_CALC_VERSION;
    // Pre-image (opt-in, #changelog_pre_image): also record the changed entity's
    // before-state from the function's own `entity_before` column, alongside the
    // after-image `entity`. Adds `object_data_before` to the INSERT and
    // `r.entity_before` to the SELECT ONLY when requested — `object_data` stays
    // the after-image for every consumer; an absent flag emits today's SQL
    // byte-for-byte. No new placeholder: the before-image is a column on `r`, like
    // `r.entity`. The two SQL forms differ in text, so `prepare_cached` keys them
    // as separate statements automatically (no extra cache discriminator needed).
    let after_before_cols = if pre_image {
        "object_data, object_data_before"
    } else {
        "object_data"
    };
    let after_before_vals = if pre_image {
        "r.entity, r.entity_before"
    } else {
        "r.entity"
    };
    format!(
        "WITH r AS MATERIALIZED (SELECT * FROM {quoted_fn}({args})), \
         _changelog AS ( \
           INSERT INTO core.tb_entity_change_log \
             (object_type, modification_type, object_id, {after_before_cols}, \
              updated_fields, cascade, started_at, duration_ms, extra_metadata, \
              tenant_id, trace_id, schema_version, trace_context, \
              actor_type, acting_for, commit_time) \
           SELECT \
             COALESCE(r.entity_type, ${object_type_idx}), \
             ${modification_type_idx}, \
             r.entity_id, {after_before_vals}, r.updated_fields, r.cascade, \
             current_setting('{started_var}')::timestamptz, \
             {duration}, \
             jsonb_build_object('duration_calc_version', {calc_version}::int) \
               || CASE WHEN ${transport_idx}::text IS NULL THEN '{{}}'::jsonb \
                       ELSE jsonb_build_object('transport', ${transport_idx}::text) END, \
             ${tenant_id_idx}::uuid, \
             ${trace_id_idx}, \
             ${schema_version_idx}, \
             ${trace_context_idx}::jsonb, \
             ${actor_type_idx}, \
             ${acting_for_idx}::uuid, \
             clock_timestamp() \
           FROM r \
           WHERE r.succeeded AND r.state_changed \
           RETURNING 1 \
         ) \
         SELECT * FROM r",
        args = placeholders.join(", "),
    )
}

/// Prepare `sql` on `client` using deadpool's **per-connection statement cache**.
///
/// The mutation function-call path sends the same statement (a fixed
/// `SELECT * FROM fn(...)` or the change-log CTE) on every call, so without
/// caching PostgreSQL re-parses and re-plans it for each mutation — the dominant
/// hot-path cost (the complex outbox CTE in particular). `prepare_cached` keys
/// the parsed/planned [`tokio_postgres::Statement`] by SQL text per connection
/// and reuses it across calls, so the parse/plan happens once per connection.
///
/// Returns an owned `Statement` (releasing the `&client` borrow), so it can be
/// prepared before `client.build_transaction()` and used inside that transaction.
async fn prepare_cached_stmt(
    client: &deadpool_postgres::Client,
    sql: &str,
) -> Result<tokio_postgres::Statement> {
    client.prepare_cached(sql).await.map_err(|e| {
        // Surface the underlying PostgreSQL diagnostic (SQLSTATE message, e.g.
        // `function "foo" does not exist`) rather than the opaque `db error` the
        // top-level `Display` renders (#451, generalised to every query path by #888).
        let detail = pg_detail(&e);
        let hint = changelog_prepare_hint(&detail);
        crate::postgres::database_error(format!("Failed to prepare statement: {detail}{hint}"), &e)
    })
}

/// Actionable operator guidance for the two prepare failures every freshly
/// authored stack hits (#569). The bare PostgreSQL diagnostics — `relation
/// "core.tb_entity_change_log" does not exist` and `column r.entity_type does
/// not exist` — point at neither of the actual requirements: the change-log
/// outbox table must be installed (`fraiseql setup`), and every mutation
/// function must return the 13-column v2.2 `mutation_response` row the outbox
/// CTE reads (`r.` is that CTE's alias for the function result, so a missing
/// `r.*` column means the function returns some other shape — classically
/// `RETURNS SETOF v_*`).
fn changelog_prepare_hint(detail: &str) -> &'static str {
    if detail.contains("core.tb_entity_change_log") {
        "\nhint: the mutation change-log outbox table is not installed in this database — \
         run `fraiseql setup` (or apply the fraiseql-observers change-log contract \
         migration) before executing mutations"
    } else if detail.contains("column r.") {
        "\nhint: the mutation function does not return the v2.2 mutation_response row \
         (13 columns: succeeded, state_changed, …) that the change-log outbox reads. \
         `RETURNS SETOF v_*` and bare scalar returns are not supported — build the row \
         with fraiseql.mutation_ok()/fraiseql.mutation_err() (installed by `fraiseql \
         setup`); see docs/architecture/mutation-response.md"
    } else {
        ""
    }
}

impl PostgresAdapter {
    /// [`DatabaseAdapter::execute_raw_query`], with each column as PostgreSQL described it
    /// and each `numeric` cell a string of the digits PostgreSQL printed.
    ///
    /// The description comes with the statement, so a result with no rows is described too,
    /// and no catalog is queried: a `numeric`'s precision and scale are its type modifier.
    ///
    /// # Errors
    ///
    /// Returns [`FraiseQLError::Database`] if the statement cannot be prepared or run.
    pub async fn execute_described_query(&self, sql: &str) -> Result<DescribedRows> {
        let client = self.acquire_connection_with_retry().await?;
        let failed = |e: tokio_postgres::Error| {
            crate::postgres::database_error(
                format!("Query execution failed: {}", pg_detail(&e)),
                &e,
            )
        };
        let statement = client.prepare(sql).await.map_err(failed)?;
        let rows: Vec<Row> = client.query(&statement, &[]).await.map_err(failed)?;

        let columns: Vec<DescribedColumn> = statement
            .columns()
            .iter()
            .map(|column| DescribedColumn {
                name:    column.name().to_string(),
                numeric: (*column.type_() == Type::NUMERIC)
                    .then(|| NumericType::from_type_modifier(column.type_modifier())),
            })
            .collect();
        let numerics: Vec<&str> = columns
            .iter()
            .filter(|column| column.numeric.is_some())
            .map(|column| column.name.as_str())
            .collect();
        let rows =
            rows.iter().map(|row| row_to_map_exact(row, &numerics)).collect::<Result<_>>()?;
        Ok(DescribedRows { columns, rows })
    }

    /// A plain `WHERE` read of `request`'s relation, its documents decoded with
    /// `request.decimals`. Its projection, count and function arguments are not read.
    async fn where_documents(
        &self,
        request: &ProjectionRequest<'_>,
        session_vars: &[(&str, &str)],
        routing: ReadRouting,
    ) -> Result<Arc<Vec<JsonbValue>>> {
        let (sql, typed_params) = build_where_select_sql_ordered(
            super::ReadFrom::relation(request.view),
            request.where_clause,
            request.limit,
            request.offset,
            request.order_by,
        )?;
        let param_refs = crate::types::as_sql_param_refs(&typed_params);
        // Session variables or none, the read carries this query's routing: delegating to
        // `execute_where_query_arc`, which has none, reverted every read without session
        // variables to server policy.
        self.execute_documents_with_session(
            &sql,
            &param_refs,
            session_vars,
            routing,
            Decimals::documents(request.decimals),
        )
        .await
        .map(Arc::new)
        .map_err(|e| enrich_undefined_column_error(e, request.view, request.where_clause))
    }

    /// The rows of a compiled aggregate statement, `session_vars` applied on its connection.
    async fn aggregate_rows(
        &self,
        sql: &str,
        params: &[serde_json::Value],
        session_vars: &[(&str, &str)],
        routing: ReadRouting,
    ) -> Result<Vec<Row>> {
        let typed = aggregate_params(params);
        let param_refs: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> =
            typed.iter().map(|v| v as &(dyn tokio_postgres::types::ToSql + Sync)).collect();
        let failed = |e: &tokio_postgres::Error| {
            crate::postgres::database_error(
                format!("Parameterized aggregate query failed: {}", pg_detail(e)),
                e,
            )
        };

        // A compiled aggregate is read-only, so a replica may serve it unless its routing
        // says otherwise; with or without session variables, the routing decides.
        let mut client = self.acquire_read_connection_with_retry(routing).await?;
        if session_vars.is_empty() {
            return client.query(sql, &param_refs).await.map_err(|e| failed(&e));
        }

        let txn = client.build_transaction().start().await.map_err(|e| {
            crate::postgres::database_error(
                format!("Failed to start session-var transaction: {}", pg_detail(&e)),
                &e,
            )
        })?;
        apply_session_vars(&txn, session_vars).await?;
        let rows = txn.query(sql, &param_refs).await.map_err(|e| failed(&e))?;
        txn.commit().await.map_err(|e| {
            crate::postgres::database_error(
                format!("Failed to commit session-var transaction: {}", pg_detail(&e)),
                &e,
            )
        })?;
        Ok(rows)
    }
}

// Reason: DatabaseAdapter is defined with #[async_trait]; all implementations must match
// its transformed method signatures to satisfy the trait contract
// async_trait: dyn-dispatch required; remove when RTN + Send is stable (RFC 3425)
#[async_trait]
impl DatabaseAdapter for PostgresAdapter {
    // Every `*_with_session` read is overridden to apply its variables (#1115).
    fn applies_session_variables(&self) -> bool {
        true
    }

    fn serves_reads_from_standbys(&self) -> bool {
        self.read_replicas.is_some()
    }

    async fn execute_with_projection(
        &self,
        view: &str,
        projection: Option<&SqlProjectionHint>,
        where_clause: Option<&WhereClause>,
        limit: Option<u32>,
        offset: Option<u32>,
        order_by: Option<&[OrderByClause]>,
    ) -> Result<Vec<JsonbValue>> {
        self.execute_with_projection_impl(view, projection, where_clause, limit, offset, order_by)
            .await
    }

    async fn execute_where_query(
        &self,
        view: &str,
        where_clause: Option<&WhereClause>,
        limit: Option<u32>,
        offset: Option<u32>,
        order_by: Option<&[OrderByClause]>,
    ) -> Result<Vec<JsonbValue>> {
        let (sql, typed_params) = build_where_select_sql_ordered(
            super::ReadFrom::relation(view),
            where_clause,
            limit,
            offset,
            order_by,
        )?;

        let param_refs = crate::types::as_sql_param_refs(&typed_params);

        self.execute_raw(&sql, &param_refs, ReadRouting::Any)
            .await
            .map_err(|e| enrich_undefined_column_error(e, view, where_clause))
    }

    async fn explain_where_query(
        &self,
        view: &str,
        where_clause: Option<&WhereClause>,
        limit: Option<u32>,
        offset: Option<u32>,
    ) -> Result<serde_json::Value> {
        let (select_sql, typed_params) = build_where_select_sql(view, where_clause, limit, offset)?;
        // Defense-in-depth: compiler-generated SQL should never contain a
        // semicolon, but guard against it to prevent statement injection.
        if select_sql.contains(';') {
            return Err(FraiseQLError::Validation {
                message: "EXPLAIN SQL must be a single statement".into(),
                path:    None,
            });
        }
        let explain_sql = format!("EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON) {select_sql}");

        let param_refs = crate::types::as_sql_param_refs(&typed_params);

        // EXPLAIN ANALYZE of a compiled SELECT: read-only, and most truthful when
        // it runs where the reads it describes run — the replica route (#407).
        let client = self.acquire_read_connection_with_retry(ReadRouting::Any).await?;
        let rows = client.query(explain_sql.as_str(), &param_refs).await.map_err(|e| {
            crate::postgres::database_error(
                format!("EXPLAIN ANALYZE failed: {}", pg_detail(&e)),
                &e,
            )
        })?;

        if let Some(row) = rows.first() {
            let plan: serde_json::Value = row.try_get(0).map_err(|e| FraiseQLError::Database {
                message:    format!("Failed to parse EXPLAIN output: {e}"),
                sql_state:  None,
                constraint: None,
            })?;
            Ok(plan)
        } else {
            Ok(serde_json::Value::Null)
        }
    }

    fn database_type(&self) -> DatabaseType {
        DatabaseType::PostgreSQL
    }

    /// `execute_composed_with_session` is implemented below.
    fn supports_composed_reads(&self) -> bool {
        true
    }

    async fn tview_profiles(&self, sources: &[String]) -> Result<Vec<crate::traits::TviewProfile>> {
        let client = self.acquire_connection_with_retry().await?;
        crate::postgres::introspector::query_tview_profiles(&client, sources).await
    }

    async fn tview_stats(&self, sources: &[String]) -> Result<crate::traits::TviewStatsReading> {
        let client = self.acquire_connection_with_retry().await?;
        crate::postgres::tview_stats::query_tview_stats(&client, sources).await
    }

    async fn schema_storage_bytes(&self, schema: &str) -> Result<Option<u64>> {
        let client = self.acquire_connection_with_retry().await?;
        // Tables and materialized views only: `pg_total_relation_size` already counts a
        // relation's indexes and TOAST, so listing those too would count them twice. The
        // schema is a bound parameter, never interpolated.
        let row = client
            .query_one(
                "SELECT COALESCE(sum(pg_total_relation_size(c.oid)), 0)::bigint FROM pg_class c \
                 JOIN pg_namespace n ON n.oid = c.relnamespace WHERE n.nspname = $1 \
                 AND c.relkind IN ('r', 'm')",
                &[&schema],
            )
            .await
            .map_err(|e| {
                crate::postgres::database_error(
                    format!("measuring schema {schema:?}: {}", pg_detail(&e)),
                    &e,
                )
            })?;
        let bytes: i64 = row.get(0);
        Ok(Some(u64::try_from(bytes).unwrap_or(0)))
    }

    async fn describe_constraint(
        &self,
        violation: &fraiseql_error::ConstraintViolation,
    ) -> Result<crate::traits::ConstraintDescription> {
        // The catalog is keyed by the relation the error names; without it there is nothing
        // to read, and the violation is reported as given.
        let (Some(schema), Some(table)) = (&violation.schema, &violation.table) else {
            return Ok(crate::traits::ConstraintDescription {
                name:    violation.name.clone(),
                columns: None,
            });
        };
        let client = self.acquire_connection_with_retry().await?;
        // A not-null violation names its column, not its constraint: PostgreSQL 18
        // catalogues the not-null constraint (`contype = 'n'`), so it is found from it.
        let name = match (&violation.name, &violation.column) {
            (Some(name), _) => Some(name.clone()),
            (None, Some(column)) => client
                .query_opt(
                    "SELECT c.conname::text FROM pg_constraint c JOIN pg_attribute a ON \
                     a.attrelid = c.conrelid WHERE c.conrelid = to_regclass(format('%I.%I', \
                     $1::text, $2::text)) AND c.contype = 'n' AND a.attname = $3 AND c.conkey = \
                     ARRAY[a.attnum]",
                    &[schema, table, column],
                )
                .await
                .map_err(|e| {
                    crate::postgres::database_error(
                        format!("Failed to resolve a not-null constraint: {}", pg_detail(&e)),
                        &e,
                    )
                })?
                .map(|row| row.get(0)),
            (None, None) => None,
        };
        let Some(name) = name else {
            return Ok(crate::traits::ConstraintDescription::default());
        };
        // The key columns of the constraint, or of the unique index of that name (a partial
        // unique index has no `pg_constraint` row), in order. An expression column
        // (`attnum = 0`) makes the answer `NULL`: never guessed. `INCLUDE` columns are not
        // key columns.
        let columns: Option<Vec<String>> = client
            .query_one(
                "WITH k AS ( \
                   SELECT u.attnum, u.ord FROM pg_constraint c, \
                          unnest(c.conkey) WITH ORDINALITY u(attnum, ord) \
                    WHERE c.conrelid = to_regclass(format('%I.%I', $1::text, $2::text)) \
                      AND c.conname = $3 \
                   UNION ALL \
                   SELECT u.attnum, u.ord FROM pg_index i, \
                          unnest(i.indkey) WITH ORDINALITY u(attnum, ord) \
                    WHERE i.indexrelid = to_regclass(format('%I.%I', $1::text, $3::text)) \
                      AND i.indrelid = to_regclass(format('%I.%I', $1::text, $2::text)) \
                      AND u.ord <= i.indnkeyatts \
                      AND NOT EXISTS (SELECT 1 FROM pg_constraint c2 \
                                       WHERE c2.conrelid = i.indrelid AND c2.conname = $3)) \
                 SELECT CASE WHEN count(*) = 0 OR bool_or(k.attnum = 0) THEN NULL \
                             ELSE array_agg(a.attname::text ORDER BY k.ord) END \
                   FROM k LEFT JOIN pg_attribute a \
                     ON a.attrelid = to_regclass(format('%I.%I', $1::text, $2::text)) \
                    AND a.attnum = k.attnum",
                &[schema, table, &name],
            )
            .await
            .map_err(|e| {
                crate::postgres::database_error(
                    format!("Failed to resolve a constraint's columns: {}", pg_detail(&e)),
                    &e,
                )
            })?
            .get(0);
        Ok(crate::traits::ConstraintDescription {
            name: Some(name),
            columns,
        })
    }

    async fn health_check(&self) -> Result<()> {
        // Use retry logic for health check to avoid false negatives during pool exhaustion
        let client = self.acquire_connection_with_retry().await?;

        client.query("SELECT 1", &[]).await.map_err(|e| {
            crate::postgres::database_error(format!("Health check failed: {}", pg_detail(&e)), &e)
        })?;

        Ok(())
    }

    #[allow(clippy::cast_possible_truncation)] // Reason: value is bounded; truncation cannot occur in practice
    fn pool_metrics(&self) -> PoolMetrics {
        let status = self.pool.status();

        PoolMetrics {
            total_connections:  status.size as u32,
            idle_connections:   status.available as u32,
            active_connections: (status.size - status.available) as u32,
            waiting_requests:   status.waiting as u32,
        }
    }

    /// # Security
    ///
    /// `sql` **must** be compiler-generated. Never pass user-supplied strings
    /// directly — doing so would open SQL-injection vulnerabilities.
    async fn execute_raw_query(
        &self,
        sql: &str,
    ) -> Result<Vec<std::collections::HashMap<String, serde_json::Value>>> {
        // Use retry logic for connection acquisition
        let client = self.acquire_connection_with_retry().await?;

        let rows: Vec<Row> = client.query(sql, &[]).await.map_err(|e| {
            crate::postgres::database_error(
                format!("Query execution failed: {}", pg_detail(&e)),
                &e,
            )
        })?;

        // Convert each row to HashMap<String, Value>
        rows.iter().map(row_to_map).collect()
    }

    async fn execute_parameterized_aggregate(
        &self,
        sql: &str,
        params: &[serde_json::Value],
    ) -> Result<Vec<std::collections::HashMap<String, serde_json::Value>>> {
        let rows = self.aggregate_rows(sql, params, &[], ReadRouting::Any).await?;
        rows.iter().map(row_to_map).collect()
    }

    async fn execute_parameterized_aggregate_with_session(
        &self,
        sql: &str,
        params: &[serde_json::Value],
        session_vars: &[(&str, &str)],
        routing: ReadRouting,
    ) -> Result<Vec<std::collections::HashMap<String, serde_json::Value>>> {
        let rows = self.aggregate_rows(sql, params, session_vars, routing).await?;
        rows.iter().map(row_to_map).collect()
    }

    async fn execute_parameterized_aggregate_exact(
        &self,
        sql: &str,
        params: &[serde_json::Value],
        session_vars: &[(&str, &str)],
        routing: ReadRouting,
        exact: &[&str],
    ) -> Result<Vec<std::collections::HashMap<String, serde_json::Value>>> {
        let rows = self.aggregate_rows(sql, params, session_vars, routing).await?;
        rows.iter().map(|row| row_to_map_exact(row, exact)).collect()
    }

    // PostgreSQL session variables are applied connection-affinely by the
    // `*_with_session` methods below: `set_config(..., true)` and the operation
    // share one transaction on one connection, so transaction-local GUCs are
    // visible to the function / view (fixes #329).

    async fn execute_where_query_arc_with_session(
        &self,
        view: &str,
        where_clause: Option<&WhereClause>,
        limit: Option<u32>,
        offset: Option<u32>,
        order_by: Option<&[OrderByClause]>,
        session_vars: &[(&str, &str)],
        routing: ReadRouting,
    ) -> Result<Arc<Vec<JsonbValue>>> {
        let request = ProjectionRequest {
            view,
            source_args: None,
            projection: None,
            where_clause,
            order_by,
            limit,
            offset,
            matched_up_to: None,
            decimals: None,
            scalar_keys: None,
        };
        self.where_documents(&request, session_vars, routing).await
    }

    /// `SELECT COUNT(*)`, pushed into the database rather than counted in Rust.
    ///
    /// The trait's default fetches every matching row and takes `.len()`, which
    /// is correct but materialises the whole filtered set — on an unfiltered
    /// list that is the entire table in memory to answer a single integer.
    async fn count_where_query(
        &self,
        view: &str,
        where_clause: Option<&WhereClause>,
        session_vars: &[(&str, &str)],
        routing: ReadRouting,
    ) -> Result<u64> {
        let (sql, typed_params) = super::build_count_sql(view, where_clause)?;
        let param_refs = crate::types::as_sql_param_refs(&typed_params);

        // Structurally read-only: replica-eligible, and `set_config` + SELECT are
        // both fine inside a hot-standby read-only transaction (#407).
        let row = if session_vars.is_empty() {
            let client = self.acquire_read_connection_with_retry(routing).await?;
            client.query_one(&sql, &param_refs).await.map_err(|e| {
                crate::postgres::database_error(
                    format!("Count query failed: {}", pg_detail(&e)),
                    &e,
                )
            })?
        } else {
            // Session variables must land on the *same* connection as the count,
            // or an RLS-filtered page gets an unfiltered total beside it (#329).
            let mut client = self.acquire_read_connection_with_retry(routing).await?;
            let txn = client.build_transaction().start().await.map_err(|e| {
                crate::postgres::database_error(
                    format!("Failed to start count session-var transaction: {}", pg_detail(&e)),
                    &e,
                )
            })?;
            super::database::apply_session_vars(&txn, session_vars).await?;
            let row = txn.query_one(&sql, &param_refs).await.map_err(|e| {
                crate::postgres::database_error(
                    format!("Count query failed: {}", pg_detail(&e)),
                    &e,
                )
            })?;
            txn.commit().await.map_err(|e| {
                crate::postgres::database_error(
                    format!("Failed to commit count session-var transaction: {}", pg_detail(&e)),
                    &e,
                )
            })?;
            row
        };

        let total: i64 = row.get(0);
        // COUNT(*) is never negative, so the bit pattern is the value; `cast_unsigned`
        // states that intent where `as u64` would only imply it.
        Ok(total.cast_unsigned())
    }

    async fn execute_with_projection_arc_with_session(
        &self,
        request: &ProjectionRequest<'_>,
        session_vars: &[(&str, &str)],
        routing: ReadRouting,
    ) -> Result<Arc<Vec<JsonbValue>>> {
        // No projection, no count and a relation => behave like a plain WHERE query,
        // matching execute_with_projection_impl's fallback.
        if request.projection.is_none()
            && request.matched_up_to.is_none()
            && request.source_args.is_none()
        {
            return self.where_documents(request, session_vars, routing).await;
        }

        let statement = ProjectionStatement::of(request)?;
        self.execute_documents_with_session(
            &statement.sql,
            &statement.params(),
            session_vars,
            routing,
            Decimals::documents(request.decimals),
        )
        .await
        .map(Arc::new)
    }

    async fn execute_with_projection_raw_with_session(
        &self,
        request: &ProjectionRequest<'_>,
        session_vars: &[(&str, &str)],
        routing: ReadRouting,
    ) -> Result<Option<RawRows>> {
        // A projected page only: a whole stored document is no response, and a counted
        // page carries its count in a second column.
        if request.projection.is_none() || request.matched_up_to.is_some() {
            return Ok(None);
        }
        let statement = ProjectionStatement::of(request)?;
        self.execute_raw_text_with_session(
            &statement.sql,
            &statement.params(),
            session_vars,
            routing,
            request.raw_rows(),
        )
        .await
        .map(Some)
    }

    /// One statement, one snapshot, one connection held for the delivery (#958).
    ///
    /// The SQL is built by the same two builders the collecting read uses, so a
    /// streamed export and a buffered read of the same query cannot select
    /// different rows — the drift that made `execute_query_direct` disagree with
    /// `count_rows` about a tenant filter (#739).
    async fn execute_composed_with_session(
        &self,
        read: &ComposedLevel,
        session_vars: &[(&str, &str)],
        routing: ReadRouting,
    ) -> Result<Arc<Vec<JsonbValue>>> {
        let (sql, typed_params) = build_composed_select_sql(read)?;
        let param_refs = crate::types::as_sql_param_refs(&typed_params);
        self.execute_documents_with_session(
            &sql,
            &param_refs,
            session_vars,
            routing,
            read.decimals.as_deref().map(Decimals::Composed),
        )
        .await
        .map(Arc::new)
    }

    async fn stream_with_projection(
        &self,
        request: &ProjectionRequest<'_>,
        session_vars: &[(&str, &str)],
        routing: ReadRouting,
    ) -> Result<JsonbRowStream> {
        // A function root is not streamed; the executor refuses one first.
        crate::traits::refuse_function_source(self, request)?;
        let (sql, typed_params) = match request.projection {
            Some(projection) => build_projection_select_sql(
                projection,
                super::ReadFrom::relation(request.view),
                request.where_clause,
                request.limit,
                request.offset,
                request.order_by,
            )?,
            None => build_where_select_sql_ordered(
                super::ReadFrom::relation(request.view),
                request.where_clause,
                request.limit,
                request.offset,
                request.order_by,
            )?,
        };

        let decimals = request.decimals.cloned();
        super::streaming::stream_rows(
            self,
            sql,
            typed_params,
            session_vars,
            routing,
            move |row, sql| document_cell(row, 0, sql, Decimals::documents(decimals.as_ref())),
        )
        .await
        .map_err(|e| enrich_undefined_column_error(e, request.view, request.where_clause))
    }

    /// The column-shaped collecting read behind gRPC: the trait default would decode each
    /// row through `row_to_map`, which rounds a `NUMERIC` through an `f64`. On the primary,
    /// as `execute_raw_query`, which it replaces here, reads.
    async fn execute_row_query(
        &self,
        view_name: &str,
        columns: &[crate::types::ColumnSpec],
        where_sql: Option<&str>,
        order_by: Option<&str>,
        limit: Option<u32>,
        offset: Option<u32>,
    ) -> Result<Vec<Vec<crate::types::ColumnValue>>> {
        let sql = crate::traits::build_row_query_sql(view_name, where_sql, order_by, limit, offset);
        let client = self.acquire_connection_with_retry().await?;
        let rows: Vec<Row> = client.query(sql.as_str(), &[]).await.map_err(|e| {
            crate::postgres::database_error(
                format!("Row query execution failed: {}", pg_detail(&e)),
                &e,
            )
        })?;
        rows.iter().map(|row| row_to_columns(row, columns)).collect()
    }

    /// The column-shaped streaming read behind gRPC server-streaming (#958).
    ///
    /// Routed to the primary like its collecting sibling: `execute_row_query`
    /// reaches `execute_raw_query`, and a surface that executes composed SQL is
    /// never sent to a replica.
    async fn stream_row_query(
        &self,
        view_name: &str,
        columns: &[crate::types::ColumnSpec],
        where_sql: Option<&str>,
        order_by: Option<&str>,
        limit: Option<u32>,
        offset: Option<u32>,
    ) -> Result<ColumnRowStream> {
        let sql = crate::traits::build_row_query_sql(view_name, where_sql, order_by, limit, offset);
        let columns = columns.to_vec();

        super::streaming::stream_rows(
            self,
            sql,
            Vec::new(),
            &[],
            ReadRouting::Primary,
            move |row, _sql| row_to_columns(row, &columns),
        )
        .await
    }

    /// The session-pinned collecting row read (#1351).
    ///
    /// Same shape as [`count_where_query`](Self::count_where_query): with no session
    /// variables it is a plain read; with them it is one transaction, so `set_config`
    /// and the SELECT land on the same connection and an RLS policy backed by
    /// `current_setting()` sees them (#329).
    async fn execute_row_query_with_session(
        &self,
        view_name: &str,
        columns: &[crate::types::ColumnSpec],
        where_sql: Option<&str>,
        order_by: Option<&str>,
        limit: Option<u32>,
        offset: Option<u32>,
        session_vars: &[(&str, &str)],
    ) -> Result<Vec<Vec<crate::types::ColumnValue>>> {
        if session_vars.is_empty() {
            return self
                .execute_row_query(view_name, columns, where_sql, order_by, limit, offset)
                .await;
        }

        let sql = crate::traits::build_row_query_sql(view_name, where_sql, order_by, limit, offset);

        let mut client = self.acquire_connection_with_retry().await?;
        let txn = client.build_transaction().start().await.map_err(|e| {
            crate::postgres::database_error(
                format!("Failed to start row-read session-var transaction: {}", pg_detail(&e)),
                &e,
            )
        })?;
        apply_session_vars(&txn, session_vars).await?;
        let rows: Vec<Row> = txn.query(sql.as_str(), &[]).await.map_err(|e| {
            crate::postgres::database_error(
                format!("Row query execution failed: {}", pg_detail(&e)),
                &e,
            )
        })?;
        txn.commit().await.map_err(|e| {
            crate::postgres::database_error(
                format!("Failed to commit row-read session-var transaction: {}", pg_detail(&e)),
                &e,
            )
        })?;

        rows.iter().map(|row| row_to_columns(row, columns)).collect()
    }

    /// The session-pinned streaming row read (#1351).
    ///
    /// `stream_rows` already pins session variables for the life of the portal —
    /// the non-session arm above passes `&[]` to the very same call.
    async fn stream_row_query_with_session(
        &self,
        view_name: &str,
        columns: &[crate::types::ColumnSpec],
        where_sql: Option<&str>,
        order_by: Option<&str>,
        limit: Option<u32>,
        offset: Option<u32>,
        session_vars: &[(&str, &str)],
    ) -> Result<ColumnRowStream> {
        let sql = crate::traits::build_row_query_sql(view_name, where_sql, order_by, limit, offset);
        let columns = columns.to_vec();

        super::streaming::stream_rows(
            self,
            sql,
            Vec::new(),
            session_vars,
            ReadRouting::Primary,
            move |row, _sql| row_to_columns(row, &columns),
        )
        .await
    }

    async fn explain_query(
        &self,
        sql: &str,
        _params: &[serde_json::Value],
    ) -> Result<serde_json::Value> {
        // Defense-in-depth: reject multi-statement input even though this SQL is
        // compiler-generated. A semicolon would allow a second statement to be
        // appended to the EXPLAIN prefix.
        if sql.contains(';') {
            return Err(FraiseQLError::Validation {
                message: "EXPLAIN SQL must be a single statement".into(),
                path:    None,
            });
        }
        let explain_sql = format!("EXPLAIN (ANALYZE false, FORMAT JSON) {sql}");
        // Read-only (plan only, ANALYZE false): replica-eligible (#407).
        let client = self.acquire_read_connection_with_retry(ReadRouting::Any).await?;
        let rows: Vec<Row> = client.query(explain_sql.as_str(), &[]).await.map_err(|e| {
            crate::postgres::database_error(format!("EXPLAIN failed: {}", pg_detail(&e)), &e)
        })?;

        if let Some(row) = rows.first() {
            let plan: serde_json::Value = row.try_get(0).map_err(|e| FraiseQLError::Database {
                message:    format!("Failed to parse EXPLAIN output: {e}"),
                sql_state:  None,
                constraint: None,
            })?;
            Ok(plan)
        } else {
            Ok(serde_json::Value::Null)
        }
    }

    async fn query_stats(&self, limit: u32) -> Result<Vec<crate::types::QueryStatEntry>> {
        self.pg_query_stats(limit).await
    }

    async fn query_stats_by_id(&self, id: &str) -> Result<Option<crate::types::QueryStatEntry>> {
        self.pg_query_stats_by_id(id).await
    }

    async fn reset_query_stats(&self) -> Result<()> {
        self.pg_reset_query_stats().await
    }

    /// The operator SQL console (#962). The containment lives in the private
    /// `postgres::adapter::admin_sql` module; see [`AdminSqlRequest`] for what
    /// each bound is and how PostgreSQL — not a parse of the statement text —
    /// enforces it.
    ///
    /// [`AdminSqlRequest`]: crate::traits::AdminSqlRequest
    async fn execute_admin_sql(
        &self,
        request: &crate::traits::AdminSqlRequest,
    ) -> Result<crate::traits::AdminSqlOutcome> {
        super::admin_sql::execute_admin_sql_impl(self, request).await
    }
}

// async_trait: dyn-dispatch required; remove when RTN + Send is stable (RFC 3425)
#[async_trait]
impl Writer for PostgresAdapter {
    async fn execute_write(
        &self,
        request: &crate::traits::WriteRequest<'_>,
        gate: crate::traits::MutationRowGate<'_>,
    ) -> Result<Vec<std::collections::HashMap<String, serde_json::Value>>> {
        let mut rows =
            run_functions_in_txn(self, std::slice::from_ref(request), &|_, rows| gate(rows), false)
                .await?;
        Ok(rows.pop().unwrap_or_default())
    }

    async fn execute_writes(
        &self,
        requests: &[crate::traits::WriteRequest<'_>],
        gate: crate::traits::BatchRowGate<'_>,
    ) -> Result<Vec<Vec<std::collections::HashMap<String, serde_json::Value>>>> {
        run_functions_in_txn(self, requests, gate, true).await
    }

    fn dispatch_ledger(&self) -> Result<&dyn crate::traits::DispatchLedger> {
        Ok(self)
    }
}
