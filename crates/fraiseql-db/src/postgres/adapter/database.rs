//! `DatabaseAdapter` and `MutationCapable` implementations for `PostgresAdapter`.

use async_trait::async_trait;
use tokio_postgres::Row;

use fraiseql_error::{FraiseQLError, Result};

use crate::{
    traits::{DatabaseAdapter, MutationCapable},
    types::{DatabaseType, JsonbValue, PoolMetrics},
    types::sql_hints::SqlProjectionHint,
    where_clause::WhereClause,
};

use super::{build_where_select_sql, PostgresAdapter};

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
        let (select_sql, typed_params) =
            build_where_select_sql(view, where_clause, limit, offset)?;
        let explain_sql = format!("EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON) {select_sql}");

        let param_refs: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> = typed_params
            .iter()
            .map(|p| p as &(dyn tokio_postgres::types::ToSql + Sync))
            .collect();

        let client = self.acquire_connection_with_retry().await?;
        let rows = client
            .query(explain_sql.as_str(), &param_refs)
            .await
            .map_err(|e| FraiseQLError::Database {
                message:   format!("EXPLAIN ANALYZE failed: {e}"),
                sql_state: e.code().map(|c| c.code().to_string()),
            })?;

        if let Some(row) = rows.first() {
            let plan: serde_json::Value =
                row.try_get(0).map_err(|e| FraiseQLError::Database {
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

    fn pool_metrics(&self) -> PoolMetrics {
        let status = self.pool.status();

        PoolMetrics {
            total_connections:  status.size as u32,
            idle_connections:   status.available as u32,
            active_connections: (status.size - status.available) as u32,
            waiting_requests:   status.waiting as u32,
        }
    }

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
        let results: Vec<std::collections::HashMap<String, serde_json::Value>> = rows
            .into_iter()
            .map(|row| {
                let mut map = std::collections::HashMap::new();

                // Iterate over all columns in the row
                for (idx, column) in row.columns().iter().enumerate() {
                    let column_name = column.name().to_string();

                    // Try to extract value based on PostgreSQL type
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
                        // Fallback: NULL
                        serde_json::Value::Null
                    };

                    map.insert(column_name, value);
                }

                map
            })
            .collect();

        Ok(results)
    }

    async fn execute_function_call(
        &self,
        function_name: &str,
        args: &[serde_json::Value],
    ) -> Result<Vec<std::collections::HashMap<String, serde_json::Value>>> {
        // Build: SELECT * FROM fn_name($1, $2, ...)
        let placeholders: Vec<String> =
            (1..=args.len()).map(|i| format!("${i}")).collect();
        let sql = format!(
            "SELECT * FROM {function_name}({})",
            placeholders.join(", ")
        );

        let client = self.acquire_connection_with_retry().await?;

        // Bind each JSON argument as a text parameter (PostgreSQL can cast text→jsonb)
        let params: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> = args
            .iter()
            .map(|v| v as &(dyn tokio_postgres::types::ToSql + Sync))
            .collect();

        let rows: Vec<Row> =
            client.query(sql.as_str(), params.as_slice()).await.map_err(|e| {
                FraiseQLError::Database {
                    message:   format!("Function call {function_name} failed: {e}"),
                    sql_state: e.code().map(|c| c.code().to_string()),
                }
            })?;

        let results = rows
            .into_iter()
            .map(|row| {
                let mut map = std::collections::HashMap::new();
                for (idx, column) in row.columns().iter().enumerate() {
                    let column_name = column.name().to_string();
                    let value: serde_json::Value =
                        if let Ok(v) = row.try_get::<_, i32>(idx) {
                            serde_json::json!(v)
                        } else if let Ok(v) = row.try_get::<_, i64>(idx) {
                            serde_json::json!(v)
                        } else if let Ok(v) = row.try_get::<_, f64>(idx) {
                            serde_json::json!(v)
                        } else if let Ok(v) = row.try_get::<_, bool>(idx) {
                            serde_json::json!(v)
                        } else if let Ok(v) = row.try_get::<_, serde_json::Value>(idx) {
                            v
                        } else if let Ok(v) = row.try_get::<_, String>(idx) {
                            serde_json::json!(v)
                        } else {
                            serde_json::Value::Null
                        };
                    map.insert(column_name, value);
                }
                map
            })
            .collect();

        Ok(results)
    }

    async fn explain_query(
        &self,
        sql: &str,
        _params: &[serde_json::Value],
    ) -> Result<serde_json::Value> {
        let explain_sql = format!("EXPLAIN (ANALYZE false, FORMAT JSON) {sql}");
        let client = self.acquire_connection_with_retry().await?;
        let rows: Vec<Row> = client.query(explain_sql.as_str(), &[]).await.map_err(|e| {
            FraiseQLError::Database {
                message:   format!("EXPLAIN failed: {e}"),
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
}

impl MutationCapable for PostgresAdapter {}
