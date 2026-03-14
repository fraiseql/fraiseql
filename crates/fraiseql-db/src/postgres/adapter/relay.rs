//! `RelayDatabaseAdapter` implementation for `PostgresAdapter`.

use async_trait::async_trait;
use fraiseql_error::{FraiseQLError, Result};

use super::{PostgresAdapter, escape_jsonb_key};
use crate::{
    identifier::quote_postgres_identifier,
    postgres::where_generator::PostgresWhereGenerator,
    traits::{CursorValue, RelayDatabaseAdapter, RelayPageResult},
    types::{
        QueryParam,
        sql_hints::{OrderByClause, OrderDirection},
    },
    where_clause::WhereClause,
};

#[async_trait]
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
        after: Option<CursorValue>,
        before: Option<CursorValue>,
        limit: u32,
        forward: bool,
        where_clause: Option<&WhereClause>,
        order_by: Option<&[OrderByClause]>,
        include_total_count: bool,
    ) -> Result<RelayPageResult> {
        let quoted_view = quote_postgres_identifier(view);
        let quoted_col = quote_postgres_identifier(cursor_column);

        // ── Cursor condition (page query only, NOT the count query) ────────────
        //
        // Per the Relay spec, totalCount ignores cursor position. The cursor
        // condition is therefore excluded from the count query.
        //
        // The cursor occupies at most one parameter slot ($1) at the front of the
        // page query's parameter list.
        //
        // UUID cursors use `$1::uuid` cast; BIGINT cursors use plain `$1`.
        let cursor_param: Option<QueryParam>;
        let cursor_where_part: Option<String>;
        let active_cursor = if forward { after } else { before };
        match active_cursor {
            None => {
                cursor_param = None;
                cursor_where_part = None;
            },
            Some(CursorValue::Int64(pk)) => {
                let op = if forward { ">" } else { "<" };
                cursor_param = Some(QueryParam::BigInt(pk));
                cursor_where_part = Some(format!("{quoted_col} {op} $1"));
            },
            Some(CursorValue::Uuid(uuid)) => {
                let op = if forward { ">" } else { "<" };
                cursor_param = Some(QueryParam::Text(uuid));
                cursor_where_part = Some(format!("{quoted_col} {op} $1::uuid"));
            },
        }
        let cursor_param_count: usize = if cursor_param.is_some() { 1 } else { 0 };

        // ── User WHERE clause ──────────────────────────────────────────────────
        //
        // Used in BOTH the count query (offset 0) and the page query (offset by
        // cursor_param_count so parameter indices don't collide).
        let mut user_where_json_params: Vec<serde_json::Value> = Vec::new();
        let page_user_where_sql: Option<String> = if let Some(clause) = where_clause {
            let generator = PostgresWhereGenerator::new();
            let (sql, params) = generator.generate_with_param_offset(clause, cursor_param_count)?;
            user_where_json_params = params;
            Some(sql)
        } else {
            None
        };
        let user_param_count = user_where_json_params.len();

        // ── ORDER BY clause ────────────────────────────────────────────────────
        //
        // Custom sort columns first, then cursor column as tiebreaker for stable
        // keyset pagination.
        let order_sql = if let Some(clauses) = order_by {
            let mut parts: Vec<String> = clauses
                .iter()
                .map(|c| {
                    let dir = match c.direction {
                        OrderDirection::Asc => "ASC",
                        OrderDirection::Desc => "DESC",
                    };
                    // escape_jsonb_key is defense-in-depth: field names are already
                    // validated as GraphQL identifiers (which cannot contain `'`).
                    format!("data->>'{field}' {dir}", field = escape_jsonb_key(&c.field))
                })
                .collect();
            let primary_dir = if forward { "ASC" } else { "DESC" };
            parts.push(format!("{quoted_col} {primary_dir}"));
            format!(" ORDER BY {}", parts.join(", "))
        } else {
            let dir = if forward { "ASC" } else { "DESC" };
            format!(" ORDER BY {quoted_col} {dir}")
        };

        // ── Page WHERE SQL ─────────────────────────────────────────────────────
        //
        // Combines cursor condition AND user filter with offset parameter indices.
        let cursor_part = cursor_where_part.as_deref().unwrap_or("");
        let user_part =
            page_user_where_sql.as_deref().map(|s| format!("({s})")).unwrap_or_default();
        let page_where_sql = if cursor_part.is_empty() && user_part.is_empty() {
            String::new()
        } else if cursor_part.is_empty() {
            format!(" WHERE {user_part}")
        } else if user_part.is_empty() {
            format!(" WHERE {cursor_part}")
        } else {
            format!(" WHERE {cursor_part} AND {user_part}")
        };

        // ── LIMIT parameter index ──────────────────────────────────────────────
        let limit_idx = cursor_param_count + user_param_count + 1;

        // ── Page SQL ───────────────────────────────────────────────────────────
        //
        // Backward pagination wraps the inner query in a subquery to re-sort
        // the descending page back to ascending order.
        let page_sql = if forward {
            format!("SELECT data FROM {quoted_view}{page_where_sql}{order_sql} LIMIT ${limit_idx}")
        } else {
            let inner = format!(
                "SELECT data, {quoted_col} AS _relay_cursor \
                 FROM {quoted_view}{page_where_sql}{order_sql} LIMIT ${limit_idx}"
            );
            format!("SELECT data FROM ({inner}) _relay_page ORDER BY _relay_cursor ASC")
        };

        // ── Page params: [cursor?, user_where_params..., limit] ────────────────
        let mut page_typed_params: Vec<QueryParam> = Vec::new();
        if let Some(cp) = cursor_param {
            page_typed_params.push(cp);
        }
        for v in &user_where_json_params {
            page_typed_params.push(QueryParam::from(v.clone()));
        }
        page_typed_params.push(QueryParam::BigInt(i64::from(limit)));

        let client = self.acquire_connection_with_retry().await?;

        // ── Execute page query ─────────────────────────────────────────────────
        let page_param_refs: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> = page_typed_params
            .iter()
            .map(|p| p as &(dyn tokio_postgres::types::ToSql + Sync))
            .collect();

        let page_rows = client.query(&page_sql, &page_param_refs).await.map_err(|e| {
            FraiseQLError::Database {
                message:   e.to_string(),
                sql_state: e.code().map(|c| c.code().to_string()),
            }
        })?;

        let rows: Vec<crate::types::JsonbValue> = page_rows
            .iter()
            .map(|row| {
                let data: serde_json::Value = row.get("data");
                crate::types::JsonbValue::new(data)
            })
            .collect();

        // ── Count query (Relay spec: totalCount ignores cursor position) ────────
        //
        // The WHERE clause is regenerated with offset 0 (no cursor parameter prefix)
        // because this is a standalone query. Using the same connection avoids an
        // extra pool acquisition.
        let total_count = if include_total_count {
            let (count_sql, count_typed_params) = if let Some(clause) = where_clause {
                let generator = PostgresWhereGenerator::new();
                let (where_sql, params) = generator.generate_with_param_offset(clause, 0)?;
                let sql = format!("SELECT COUNT(*) FROM {quoted_view} WHERE ({where_sql})");
                let typed: Vec<QueryParam> = params.into_iter().map(QueryParam::from).collect();
                (sql, typed)
            } else {
                (format!("SELECT COUNT(*) FROM {quoted_view}"), Vec::<QueryParam>::new())
            };

            let count_param_refs: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> =
                count_typed_params
                    .iter()
                    .map(|p| p as &(dyn tokio_postgres::types::ToSql + Sync))
                    .collect();

            let count_row = client.query_one(&count_sql, &count_param_refs).await.map_err(|e| {
                FraiseQLError::Database {
                    message:   e.to_string(),
                    sql_state: e.code().map(|c| c.code().to_string()),
                }
            })?;

            let total: i64 = count_row.get(0);
            Some(total as u64)
        } else {
            None
        };

        Ok(RelayPageResult { rows, total_count })
    }
}
