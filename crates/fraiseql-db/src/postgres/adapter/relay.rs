//! `RelayDatabaseAdapter` implementation for `PostgresAdapter`.

use fraiseql_error::{FraiseQLError, Result};

use super::PostgresAdapter;
use crate::{
    dialect::PostgresDialect,
    identifier::quote_postgres_identifier,
    postgres::{pg_detail, where_generator::PostgresWhereGenerator},
    traits::{RelayCursor, RelayDatabaseAdapter, RelayPageResult},
    types::{QueryParam, ReadRouting, sql_hints::OrderByClause},
    where_clause::WhereClause,
};

impl RelayDatabaseAdapter for PostgresAdapter {
    /// Execute keyset (cursor-based) pagination against a JSONB view.
    ///
    /// # `totalCount` semantics
    ///
    /// When `include_total_count` is `true`, **two queries** are issued on the same
    /// connection:
    ///
    /// 1. A count query — `SELECT COUNT(*) FROM {view} WHERE {user_filter}` — that reflects the
    ///    **full connection** size, ignoring cursor position. This is required by the Relay Cursor
    ///    Connections spec, which defines `totalCount` as the count of all objects in the
    ///    connection, regardless of `after`/`before`.
    ///
    /// 2. A page query — the cursor-filtered, limited result set.
    ///
    /// The two-query approach fixes a previous bug where `COUNT(*) OVER()` ran
    /// inside the cursor-filtered subquery, causing `totalCount` to shrink as the
    /// cursor advanced.  It also handles the edge case where the current page is
    /// empty but the total count is non-zero (e.g., cursor past the last row).
    ///
    /// When `include_total_count` is `false`, only the page query is issued.
    ///
    /// # Performance note
    ///
    /// The count query scans all rows matching the user filter without LIMIT. On
    /// large unfiltered tables this may be slow. Mitigations:
    /// - Only enable `totalCount` when the client explicitly requests it (enforced by the executor
    ///   via `include_total_count`).
    /// - Add a `statement_timeout` on the connection for relay queries on very large datasets.
    /// - Maintain a denormalised count table or materialised view for hot paths.
    async fn execute_relay_page(
        &self,
        view: &str,
        cursor_column: &str,
        after: Option<RelayCursor>,
        before: Option<RelayCursor>,
        limit: u32,
        forward: bool,
        where_clause: Option<&WhereClause>,
        order_by: Option<&[OrderByClause]>,
        include_total_count: bool,
    ) -> Result<RelayPageResult> {
        // Relay pagination is a compiled SELECT pair: replica-eligible (#407).
        let client = self.acquire_read_connection_with_retry(ReadRouting::Any).await?;
        let active = if forward {
            after.clone()
        } else {
            before.clone()
        };
        let page = self
            .run_relay_page(
                &**client,
                view,
                cursor_column,
                after,
                before,
                limit,
                forward,
                where_clause,
                order_by,
                include_total_count,
            )
            .await;
        drop(client);
        self.refuse_a_cursor_the_page_could_not_read(page, active.as_ref(), order_by)
            .await
    }

    #[allow(clippy::too_many_arguments)] // Reason: relay pagination requires all cursor/filter/sort/count arguments plus session vars; no natural grouping
    async fn execute_relay_page_with_session(
        &self,
        view: &str,
        cursor_column: &str,
        after: Option<RelayCursor>,
        before: Option<RelayCursor>,
        limit: u32,
        forward: bool,
        where_clause: Option<&WhereClause>,
        order_by: Option<&[OrderByClause]>,
        include_total_count: bool,
        session_vars: &[(&str, &str)],
        routing: ReadRouting,
    ) -> Result<RelayPageResult> {
        // Fast path: no session vars => identical to execute_relay_page.
        if session_vars.is_empty() {
            return self
                .execute_relay_page(
                    view,
                    cursor_column,
                    after,
                    before,
                    limit,
                    forward,
                    where_clause,
                    order_by,
                    include_total_count,
                )
                .await;
        }

        // Apply set_config and run BOTH the page and count queries inside one
        // transaction on one connection so RLS sees the session variables.
        // Read-only and standby-safe: replica-eligible (#407).
        let mut client = self.acquire_read_connection_with_retry(routing).await?;
        let txn =
            client.build_transaction().start().await.map_err(|e| FraiseQLError::Database {
                message:   format!(
                    "Failed to start relay session-var transaction: {}",
                    pg_detail(&e)
                ),
                sql_state: e.code().map(|c| c.code().to_string()),
            })?;
        super::database::apply_session_vars(&txn, session_vars).await?;
        let active = if forward {
            after.clone()
        } else {
            before.clone()
        };
        let page = self
            .run_relay_page(
                &*txn,
                view,
                cursor_column,
                after,
                before,
                limit,
                forward,
                where_clause,
                order_by,
                include_total_count,
            )
            .await;
        if page.is_err() {
            // The transaction is aborted; the probe runs on a connection of its own.
            drop(txn);
            drop(client);
            return self
                .refuse_a_cursor_the_page_could_not_read(page, active.as_ref(), order_by)
                .await;
        }
        let result = page?;
        txn.commit().await.map_err(|e| FraiseQLError::Database {
            message:   format!("Failed to commit relay session-var transaction: {}", pg_detail(&e)),
            sql_state: e.code().map(|c| c.code().to_string()),
        })?;
        Ok(result)
    }
}

impl PostgresAdapter {
    /// `page`, unless it failed on a data exception that the cursor it resumed from caused
    /// (#1521): then a refusal of the cursor, saying how to recover.
    ///
    /// A cursor is client data, and a forged value reaches PostgreSQL as a bound parameter
    /// cast to its key's type, where it fails as `22P02` and the like. Only on that failure
    /// path, PostgreSQL's own input parser is asked about each of the cursor's typed values
    /// ([`cursor_values_probe`](crate::keyset::cursor_values_probe)), on a fresh connection.
    /// When every value is valid the data raised the exception, and it is returned as it
    /// was; so is the original error if the probe itself cannot run.
    async fn refuse_a_cursor_the_page_could_not_read(
        &self,
        page: Result<RelayPageResult>,
        cursor: Option<&RelayCursor>,
        order_by: Option<&[OrderByClause]>,
    ) -> Result<RelayPageResult> {
        let error = match page {
            Err(error @ FraiseQLError::Database { .. }) => error,
            other => return other,
        };
        let data_exception = matches!(
            &error,
            FraiseQLError::Database { sql_state: Some(state), .. } if state.starts_with("22")
        );
        let (Some(cursor), true) = (cursor, data_exception) else {
            return Err(error);
        };
        let Ok(keys) = crate::keyset::keyset_keys(order_by) else {
            return Err(error);
        };
        let uuid_position = match &cursor.position {
            crate::traits::CursorValue::Uuid(uuid) => Some(uuid.as_str()),
            crate::traits::CursorValue::Int64(_) => None,
        };
        let Some((sql, params)) =
            crate::keyset::cursor_values_probe(&keys, &cursor.sort_keys, uuid_position)
        else {
            return Err(error);
        };
        let Ok(client) = self.acquire_read_connection_with_retry(ReadRouting::Any).await else {
            return Err(error);
        };
        let params: Vec<QueryParam> = params.into_iter().map(QueryParam::Text).collect();
        let refs = crate::types::as_sql_param_refs(&params);
        let Ok(row) = client.query_one(&sql, &refs).await else {
            return Err(error);
        };
        let valid: Vec<bool> = row.get("valid");
        if valid.iter().all(|v| *v) {
            return Err(error);
        }
        Err(FraiseQLError::validation(
            "the `after`/`before` cursor is not one of this connection: a value it carries is \
             not of its key's type; request the first page again, without `after`/`before`",
        ))
    }

    /// Build and run the relay page (and optional total-count) queries against
    /// an arbitrary client — a pooled connection for the plain path, or a
    /// transaction for the connection-affine `*_with_session` path.
    #[allow(clippy::too_many_arguments)] // Reason: relay pagination requires all cursor/filter/sort/count arguments; no natural grouping
    async fn run_relay_page<C>(
        &self,
        client: &C,
        view: &str,
        cursor_column: &str,
        after: Option<RelayCursor>,
        before: Option<RelayCursor>,
        limit: u32,
        forward: bool,
        where_clause: Option<&WhereClause>,
        order_by: Option<&[OrderByClause]>,
        include_total_count: bool,
    ) -> Result<RelayPageResult>
    where
        C: tokio_postgres::GenericClient + Sync,
    {
        let quoted_view = quote_postgres_identifier(view);
        let quoted_col = quote_postgres_identifier(cursor_column);

        // #1284: a relevance-ranked read cannot be paged by cursor (refused by
        // `keyset_keys`). Each key is rendered by the same rules the offset path's
        // `ORDER BY` uses (#832): storage key, declared type's cast, native column,
        // collation, localized label.
        let keys = crate::keyset::keyset_keys(order_by)?;

        // ── Keyset condition (page query only, NOT the count query) ────────────
        //
        // Per the Relay spec, totalCount ignores cursor position, so the count query
        // leaves this out. The page resumes past the cursor row's sort-key values,
        // then past its position (#1521): its parameters lead the page query's list,
        // the key values (as text) then the position.
        let mut page_typed_params: Vec<QueryParam> = Vec::new();
        let active_cursor = if forward { after } else { before };
        let keyset_where = match active_cursor {
            None => None,
            Some(cursor) => {
                let (position, placeholder) = crate::keyset::position_param(&cursor.position);
                let (predicate, key_params, _) = crate::keyset::keyset_predicate(
                    &keys,
                    &cursor.sort_keys,
                    &quoted_col,
                    placeholder,
                    forward,
                    1,
                )?;
                page_typed_params.extend(key_params.into_iter().map(QueryParam::Text));
                page_typed_params.push(position);
                Some(predicate)
            },
        };
        let keyset_param_count = page_typed_params.len();

        // ── User WHERE clause ──────────────────────────────────────────────────
        //
        // Used in BOTH the count query (offset 0) and the page query (offset by
        // the keyset's parameters so indices don't collide).
        let page_user_where_sql: Option<String> = if let Some(clause) = where_clause {
            let generator = PostgresWhereGenerator::new(PostgresDialect);
            let (sql, params) = generator.generate_with_param_offset(clause, keyset_param_count)?;
            page_typed_params.extend(params.into_iter().map(QueryParam::from));
            Some(sql)
        } else {
            None
        };

        // ── Page WHERE SQL ─────────────────────────────────────────────────────
        let conditions: Vec<String> = keyset_where
            .into_iter()
            .chain(page_user_where_sql.map(|s| format!("({s})")))
            .collect();
        let page_where_sql = if conditions.is_empty() {
            String::new()
        } else {
            format!(" WHERE {}", conditions.join(" AND "))
        };

        // ── LIMIT parameter ────────────────────────────────────────────────────
        page_typed_params.push(QueryParam::BigInt(i64::from(limit)));
        let limit_idx = page_typed_params.len();

        // ── Page SQL ───────────────────────────────────────────────────────────
        //
        // The ordering ends with the cursor column, which makes it total, and nothing
        // after it: the next page resumes from exactly these terms (#1287). Each row's
        // document carries its sort-key values, which the next page's cursor is built
        // from. A backward page reads the ordering reversed from the cursor, then the
        // outer query restores the requested order.
        let document = crate::keyset::with_sort_keys("data", &keys);
        let order_sql = crate::keyset::keyset_order(&keys, &quoted_col, forward);
        let page_sql = if forward {
            format!(
                "SELECT {document} AS data FROM {quoted_view}{page_where_sql} \
                 ORDER BY {order_sql} LIMIT ${limit_idx}"
            )
        } else {
            let (key_columns, outer_keys) =
                crate::keyset::carried_keys(&keys, "_relay_k", "_relay_page");
            let inner = format!(
                "SELECT {document} AS data, {quoted_col} AS _relay_cursor{key_columns} \
                 FROM {quoted_view}{page_where_sql} ORDER BY {order_sql} LIMIT ${limit_idx}"
            );
            let outer_order =
                crate::keyset::keyset_order(&outer_keys, "_relay_page._relay_cursor", true);
            format!("SELECT data FROM ({inner}) _relay_page ORDER BY {outer_order}")
        };

        // ── Execute page query (on the caller-provided client / transaction) ────
        let page_param_refs = crate::types::as_sql_param_refs(&page_typed_params);

        let page_rows = client.query(&page_sql, &page_param_refs).await.map_err(|e| {
            FraiseQLError::Database {
                message:   pg_detail(&e),
                sql_state: e.code().map(|c| c.code().to_string()),
            }
        })?;

        let rows: Vec<crate::types::JsonbValue> = page_rows
            .iter()
            .map(|row| super::jsonb_cell(row, "data", &page_sql))
            .collect::<Result<Vec<_>>>()?;

        // ── Count query (Relay spec: totalCount ignores cursor position) ────────
        //
        // The WHERE clause is regenerated with offset 0 (no cursor parameter prefix)
        // because this is a standalone query. Using the same connection avoids an
        // extra pool acquisition.
        let total_count = if include_total_count {
            let (count_sql, count_typed_params) = if let Some(clause) = where_clause {
                let generator = PostgresWhereGenerator::new(PostgresDialect);
                let (where_sql, params) = generator.generate_with_param_offset(clause, 0)?;
                let sql = format!("SELECT COUNT(*) FROM {quoted_view} WHERE ({where_sql})");
                let typed: Vec<QueryParam> = params.into_iter().map(QueryParam::from).collect();
                (sql, typed)
            } else {
                (format!("SELECT COUNT(*) FROM {quoted_view}"), Vec::<QueryParam>::new())
            };

            let count_param_refs = crate::types::as_sql_param_refs(&count_typed_params);

            let count_row = client.query_one(&count_sql, &count_param_refs).await.map_err(|e| {
                FraiseQLError::Database {
                    message:   pg_detail(&e),
                    sql_state: e.code().map(|c| c.code().to_string()),
                }
            })?;

            let total: i64 = count_row.get(0);
            // cast_unsigned() is the clippy-recommended alternative to `as u64` for i64;
            // it has the same bit-pattern semantics but makes the sign-loss intent explicit.
            // Row counts from COUNT(*) are always non-negative so sign loss is impossible.
            Some(total.cast_unsigned())
        } else {
            None
        };

        Ok(RelayPageResult::new(rows, total_count))
    }
}
