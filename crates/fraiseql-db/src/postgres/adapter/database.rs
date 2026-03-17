//! `DatabaseAdapter` and `SupportsMutations` implementations for `PostgresAdapter`.

use async_trait::async_trait;
use fraiseql_error::{FraiseQLError, Result};
use tokio_postgres::Row;

use super::{PostgresAdapter, build_where_select_sql};
use crate::{
    traits::{DatabaseAdapter, SupportsMutations},
    types::{DatabaseType, JsonbValue, PoolMetrics, QueryParam, sql_hints::SqlProjectionHint},
    where_clause::WhereClause,
};

/// Convert a single `tokio_postgres::Row` into a `HashMap<String, serde_json::Value>`.
///
/// Tries each PostgreSQL type in priority order; falls back to `Null` for
/// types that cannot be represented as JSON.
fn row_to_map(row: &Row) -> std::collections::HashMap<String, serde_json::Value> {
    let mut map = std::collections::HashMap::new();
    for (idx, column) in row.columns().iter().enumerate() {
        let column_name = column.name().to_string();
        let value: serde_json::Value = if let Ok(v) = row.try_get::<_, i32>(idx) {
            serde_json::json!(v)
        } else if let Ok(v) = row.try_get::<_, i64>(idx) {
            serde_json::json!(v)
        } else if let Ok(v) = row.try_get::<_, f64>(idx) {
            serde_json::json!(v)
        } else if let Ok(v) = row.try_get::<_, String>(idx) {
            serde_json::json!(v)
        } else if let Ok(v) = row.try_get::<_, bool>(idx) {
            serde_json::json!(v)
        } else if let Ok(v) = row.try_get::<_, serde_json::Value>(idx) {
            v
        } else {
            serde_json::Value::Null
        };
        map.insert(column_name, value);
    }
    map
}

// Reason: DatabaseAdapter is defined with #[async_trait]; all implementations must match
// its transformed method signatures to satisfy the trait contract
// async_trait: dyn-dispatch required; remove when RTN + Send is stable (RFC 3425)
#[async_trait]
impl DatabaseAdapter for PostgresAdapter {
    async fn execute_with_projection(
        &self,
        view: &str,
        projection: Option<&SqlProjectionHint>,
        where_clause: Option<&WhereClause>,
        limit: Option<u32>,
    ) -> Result<Vec<JsonbValue>> {
        self.execute_with_projection(view, projection, where_clause, limit).await
    }

    async fn execute_where_query(
        &self,
        view: &str,
        where_clause: Option<&WhereClause>,
        limit: Option<u32>,
        offset: Option<u32>,
    ) -> Result<Vec<JsonbValue>> {
        let (sql, typed_params) = build_where_select_sql(view, where_clause, limit, offset)?;

        let param_refs: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> = typed_params
            .iter()
            .map(|p| p as &(dyn tokio_postgres::types::ToSql + Sync))
            .collect();

        self.execute_raw(&sql, &param_refs).await
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

        let param_refs: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> = typed_params
            .iter()
            .map(|p| p as &(dyn tokio_postgres::types::ToSql + Sync))
            .collect();

        let client = self.acquire_connection_with_retry().await?;
        let rows = client.query(explain_sql.as_str(), &param_refs).await.map_err(|e| {
            FraiseQLError::Database {
                message:   format!("EXPLAIN ANALYZE failed: {e}"),
                sql_state: e.code().map(|c| c.code().to_string()),
            }
        })?;

        if let Some(row) = rows.first() {
            let plan: serde_json::Value = row.try_get(0).map_err(|e| FraiseQLError::Database {
                message:   format!("Failed to parse EXPLAIN output: {e}"),
                sql_state: None,
            })?;
            Ok(plan)
        } else {
            Ok(serde_json::Value::Null)
        }
    }

    fn database_type(&self) -> DatabaseType {
        DatabaseType::PostgreSQL
    }

    async fn health_check(&self) -> Result<()> {
        // Use retry logic for health check to avoid false negatives during pool exhaustion
        let client = self.acquire_connection_with_retry().await?;

        client.query("SELECT 1", &[]).await.map_err(|e| FraiseQLError::Database {
            message:   format!("Health check failed: {e}"),
            sql_state: e.code().map(|c| c.code().to_string()),
        })?;

        Ok(())
    }

    #[allow(clippy::cast_possible_truncation)]  // Reason: value is bounded; truncation cannot occur in practice
    // Reason: Connection pool counts are bounded by `max_pool_size` which defaults to 50 and
    // is configured via `fraiseql.toml` (typically ≤1000). Truncation from usize to u32 cannot
    // occur in practice; u32::MAX (4 billion) far exceeds any realistic pool size.
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

        let rows: Vec<Row> = client.query(sql, &[]).await.map_err(|e| FraiseQLError::Database {
            message:   format!("Query execution failed: {e}"),
            sql_state: e.code().map(|c| c.code().to_string()),
        })?;

        // Convert each row to HashMap<String, Value>
        let results: Vec<std::collections::HashMap<String, serde_json::Value>> =
            rows.iter().map(row_to_map).collect();

        Ok(results)
    }

    async fn execute_parameterized_aggregate(
        &self,
        sql: &str,
        params: &[serde_json::Value],
    ) -> Result<Vec<std::collections::HashMap<String, serde_json::Value>>> {
        // Convert serde_json::Value params to QueryParam so that strings are bound
        // as TEXT (not JSONB), which is required for correct WHERE comparisons against
        // data->>'field' expressions that return TEXT.
        let typed: Vec<QueryParam> = params.iter().cloned().map(QueryParam::from).collect();
        let param_refs: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> =
            typed.iter().map(|p| p as &(dyn tokio_postgres::types::ToSql + Sync)).collect();

        let client = self.acquire_connection_with_retry().await?;
        let rows: Vec<Row> =
            client.query(sql, &param_refs).await.map_err(|e| FraiseQLError::Database {
                message:   format!("Parameterized aggregate query failed: {e}"),
                sql_state: e.code().map(|c| c.code().to_string()),
            })?;

        let results: Vec<std::collections::HashMap<String, serde_json::Value>> =
            rows.iter().map(row_to_map).collect();

        Ok(results)
    }

    async fn execute_function_call(
        &self,
        function_name: &str,
        args: &[serde_json::Value],
    ) -> Result<Vec<std::collections::HashMap<String, serde_json::Value>>> {
        // Build: SELECT * FROM "fn_name"($1, $2, ...)
        // The function name is double-quoted so that reserved words, mixed-case
        // names, and names with special characters are handled correctly.
        // Any embedded double quotes are escaped by doubling them ("").
        let quoted_fn = format!("\"{}\"", function_name.replace('"', "\"\""));
        let placeholders: Vec<String> = (1..=args.len()).map(|i| format!("${i}")).collect();
        let sql = format!("SELECT * FROM {quoted_fn}({})", placeholders.join(", "));

        let mut client = self.acquire_connection_with_retry().await?;

        // Bind each JSON argument as a text parameter (PostgreSQL can cast text→jsonb)
        let params: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> =
            args.iter().map(|v| v as &(dyn tokio_postgres::types::ToSql + Sync)).collect();

        if self.mutation_timing_enabled {
            // Wrap in a transaction so SET LOCAL scopes the variable to this call only.
            // `set_config(name, value, is_local)` with is_local=true is equivalent to
            // SET LOCAL and is parameterized to avoid SQL injection.
            let txn = client
                .build_transaction()
                .start()
                .await
                .map_err(|e| FraiseQLError::Database {
                    message:   format!("Failed to start mutation timing transaction: {e}"),
                    sql_state: e.code().map(|c| c.code().to_string()),
                })?;

            txn.execute(
                "SELECT set_config($1, clock_timestamp()::text, true)",
                &[&self.timing_variable_name],
            )
            .await
            .map_err(|e| FraiseQLError::Database {
                message:   format!("Failed to set mutation timing variable: {e}"),
                sql_state: e.code().map(|c| c.code().to_string()),
            })?;

            let rows: Vec<Row> =
                txn.query(sql.as_str(), params.as_slice()).await.map_err(|e| {
                    FraiseQLError::Database {
                        message:   format!("Function call {function_name} failed: {e}"),
                        sql_state: e.code().map(|c| c.code().to_string()),
                    }
                })?;

            txn.commit().await.map_err(|e| FraiseQLError::Database {
                message:   format!("Failed to commit mutation timing transaction: {e}"),
                sql_state: e.code().map(|c| c.code().to_string()),
            })?;

            let results: Vec<std::collections::HashMap<String, serde_json::Value>> =
                rows.iter().map(row_to_map).collect();

            Ok(results)
        } else {
            let rows: Vec<Row> =
                client.query(sql.as_str(), params.as_slice()).await.map_err(|e| {
                    FraiseQLError::Database {
                        message:   format!("Function call {function_name} failed: {e}"),
                        sql_state: e.code().map(|c| c.code().to_string()),
                    }
                })?;

            let results: Vec<std::collections::HashMap<String, serde_json::Value>> =
                rows.iter().map(row_to_map).collect();

            Ok(results)
        }
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
        let client = self.acquire_connection_with_retry().await?;
        let rows: Vec<Row> =
            client
                .query(explain_sql.as_str(), &[])
                .await
                .map_err(|e| FraiseQLError::Database {
                    message:   format!("EXPLAIN failed: {e}"),
                    sql_state: e.code().map(|c| c.code().to_string()),
                })?;

        if let Some(row) = rows.first() {
            let plan: serde_json::Value = row.try_get(0).map_err(|e| FraiseQLError::Database {
                message:   format!("Failed to parse EXPLAIN output: {e}"),
                sql_state: None,
            })?;
            Ok(plan)
        } else {
            Ok(serde_json::Value::Null)
        }
    }
}

impl SupportsMutations for PostgresAdapter {}
