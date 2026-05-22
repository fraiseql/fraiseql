//! SQLite database adapter — **read-only** (queries only, no mutations).
//!
//! This adapter supports query execution (`execute_where_query`, `execute_raw_query`)
//! but does **not** implement [`SupportsMutations`](crate::SupportsMutations). Attempting
//! to compile a schema with mutations and run it against SQLite will produce a
//! **compile-time error** at the mutation executor call site.
//!
//! # When to use SQLite
//!
//! - Unit testing queries without a real database
//! - Schema exploration and local development (read-only)
//!
//! For mutation support, use PostgreSQL, MySQL, or SQL Server.

use std::fmt::Write;

use async_trait::async_trait;
use fraiseql_error::{FraiseQLError, Result};
use sqlx::{
    Column, Row,
    sqlite::{SqlitePool, SqlitePoolOptions, SqliteRow},
};

use super::where_generator::SqliteWhereGenerator;
use crate::{
    dialect::SqliteDialect,
    identifier::quote_sqlite_identifier,
    order_by::append_order_by,
    traits::{DatabaseAdapter, DirectMutationContext, MutationStrategy},
    types::{DatabaseType, JsonbValue, PoolMetrics, sql_hints::OrderByClause},
    where_clause::WhereClause,
};

/// SQLite database adapter with connection pooling.
///
/// Uses `sqlx` for connection pooling and async queries.
/// Ideal for local development and testing.
///
/// # Example
///
/// ```no_run
/// use fraiseql_db::sqlite::SqliteAdapter;
/// use fraiseql_db::{DatabaseAdapter, WhereClause, WhereOperator};
/// use serde_json::json;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// // Create adapter with file path
/// let adapter = SqliteAdapter::new("sqlite:./test.db").await?;
///
/// // Or use in-memory database
/// let adapter = SqliteAdapter::new("sqlite::memory:").await?;
///
/// // Execute query
/// let where_clause = WhereClause::Field {
///     path: vec!["email".to_string()],
///     operator: WhereOperator::Icontains,
///     value: json!("example.com"),
/// };
///
/// let results = adapter
///     .execute_where_query("v_user", Some(&where_clause), Some(10), None, None)
///     .await?;
///
/// println!("Found {} users", results.len());
/// # Ok(())
/// # }
/// ```
#[derive(Clone)]
pub struct SqliteAdapter {
    pub(super) pool: SqlitePool,
}

impl SqliteAdapter {
    /// Create new SQLite adapter with default pool configuration.
    ///
    /// # Arguments
    ///
    /// * `connection_string` - SQLite connection string (e.g., "sqlite:./mydb.db" or
    ///   "sqlite::memory:")
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::ConnectionPool` if pool creation fails.
    pub async fn new(connection_string: &str) -> Result<Self> {
        Self::with_pool_size(connection_string, 5).await
    }

    /// Create new SQLite adapter with custom pool configuration.
    ///
    /// # Arguments
    ///
    /// * `connection_string` - SQLite connection string
    /// * `min_size` - Minimum pool size
    /// * `max_size` - Maximum number of connections in pool
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::ConnectionPool` if pool creation fails.
    pub async fn with_pool_config(
        connection_string: &str,
        min_size: u32,
        max_size: u32,
    ) -> Result<Self> {
        let pool = SqlitePoolOptions::new()
            .min_connections(min_size)
            .max_connections(max_size)
            .connect(connection_string)
            .await
            .map_err(|e| FraiseQLError::ConnectionPool {
                message: format!("Failed to create SQLite connection pool: {e}"),
            })?;

        Ok(Self { pool })
    }

    /// Create new SQLite adapter with custom pool size.
    ///
    /// # Arguments
    ///
    /// * `connection_string` - SQLite connection string
    /// * `max_size` - Maximum number of connections in pool
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::ConnectionPool` if pool creation fails.
    pub async fn with_pool_size(connection_string: &str, max_size: u32) -> Result<Self> {
        let pool = SqlitePoolOptions::new()
            .max_connections(max_size)
            .connect(connection_string)
            .await
            .map_err(|e| FraiseQLError::ConnectionPool {
                message: format!("Failed to create SQLite connection pool: {e}"),
            })?;

        // Test connection
        sqlx::query("SELECT 1")
            .fetch_one(&pool)
            .await
            .map_err(|e| FraiseQLError::Database {
                message:   format!("Failed to connect to SQLite database: {e}"),
                sql_state: None,
            })?;

        Ok(Self { pool })
    }

    /// Create an in-memory SQLite adapter (useful for testing).
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::ConnectionPool` if pool creation fails.
    pub async fn in_memory() -> Result<Self> {
        Self::new("sqlite::memory:").await
    }

    /// Execute raw SQL query and return JSONB rows.
    async fn execute_raw(
        &self,
        sql: &str,
        params: Vec<serde_json::Value>,
    ) -> Result<Vec<JsonbValue>> {
        // Build the query with dynamic parameters
        let mut query = sqlx::query(sql);

        for param in &params {
            query = match param {
                serde_json::Value::String(s) => query.bind(s.clone()),
                serde_json::Value::Number(n) => {
                    if let Some(i) = n.as_i64() {
                        query.bind(i)
                    } else if let Some(f) = n.as_f64() {
                        query.bind(f)
                    } else {
                        query.bind(n.to_string())
                    }
                },
                serde_json::Value::Bool(b) => query.bind(*b),
                serde_json::Value::Null => query.bind(Option::<String>::None),
                serde_json::Value::Array(_) | serde_json::Value::Object(_) => {
                    query.bind(param.to_string())
                },
            };
        }

        let rows: Vec<SqliteRow> =
            query.fetch_all(&self.pool).await.map_err(|e| FraiseQLError::Database {
                message:   format!("SQLite query execution failed: {e}"),
                sql_state: None,
            })?;

        let results = rows
            .into_iter()
            .map(|row| {
                // SQLite stores JSON as TEXT, parse it
                let data_str: String = row.try_get("data").unwrap_or_default();
                let data: serde_json::Value =
                    serde_json::from_str(&data_str).unwrap_or(serde_json::Value::Null);
                JsonbValue::new(data)
            })
            .collect();

        Ok(results)
    }
}

// Reason: DatabaseAdapter is defined with #[async_trait]; all implementations must match
// its transformed method signatures to satisfy the trait contract
// async_trait: dyn-dispatch required; remove when RTN + Send is stable (RFC 3425)
#[async_trait]
impl DatabaseAdapter for SqliteAdapter {
    async fn execute_with_projection(
        &self,
        view: &str,
        projection: Option<&crate::types::SqlProjectionHint>,
        where_clause: Option<&WhereClause>,
        limit: Option<u32>,
        offset: Option<u32>,
        order_by: Option<&[OrderByClause]>,
    ) -> Result<Vec<JsonbValue>> {
        // If no projection provided, fall back to standard query
        if projection.is_none() {
            return self.execute_where_query(view, where_clause, limit, offset, order_by).await;
        }

        let Some(projection) = projection else {
            // Reason: unreachable — `is_none()` check above returns early
            unreachable!("projection is Some; None case returned above");
        };

        // Build SQL with SQLite-specific json_object projection
        let mut sql = format!(
            "SELECT {} FROM {}",
            projection.projection_template,
            quote_sqlite_identifier(view)
        );

        // Add WHERE clause if present
        let params: Vec<serde_json::Value> = if let Some(clause) = where_clause {
            let generator = super::where_generator::SqliteWhereGenerator::new(SqliteDialect);
            let (where_sql, where_params) = generator.generate(clause)?;
            sql.push_str(" WHERE ");
            sql.push_str(&where_sql);
            where_params
        } else {
            Vec::new()
        };

        // ORDER BY must come before LIMIT/OFFSET.
        append_order_by(&mut sql, order_by, DatabaseType::SQLite)?;

        // Add LIMIT/OFFSET — SQLite requires LIMIT before OFFSET.
        // Reason (expect below): fmt::Write for String is infallible.
        match (limit, offset) {
            (Some(lim), Some(off)) => {
                write!(sql, " LIMIT {lim} OFFSET {off}").expect("write to String");
            },
            (Some(lim), None) => {
                write!(sql, " LIMIT {lim}").expect("write to String");
            },
            (None, Some(off)) => {
                // SQLite requires LIMIT before OFFSET; use -1 as "unlimited"
                write!(sql, " LIMIT -1 OFFSET {off}").expect("write to String");
            },
            (None, None) => {},
        }

        // Execute the query
        self.execute_raw(&sql, params).await
    }

    async fn execute_where_query(
        &self,
        view: &str,
        where_clause: Option<&WhereClause>,
        limit: Option<u32>,
        offset: Option<u32>,
        order_by: Option<&[OrderByClause]>,
    ) -> Result<Vec<JsonbValue>> {
        // Build base query - SQLite uses double quotes for identifiers
        let mut sql = format!("SELECT data FROM {}", quote_sqlite_identifier(view));

        // Add WHERE clause if present
        let mut params: Vec<serde_json::Value> = if let Some(clause) = where_clause {
            let generator = SqliteWhereGenerator::new(SqliteDialect);
            let (where_sql, where_params) = generator.generate(clause)?;
            sql.push_str(" WHERE ");
            sql.push_str(&where_sql);
            where_params
        } else {
            Vec::new()
        };

        // ORDER BY must come before LIMIT/OFFSET.
        append_order_by(&mut sql, order_by, DatabaseType::SQLite)?;

        // Add LIMIT and OFFSET
        // Note: SQLite requires LIMIT when using OFFSET, so we use LIMIT -1 for "unlimited"
        match (limit, offset) {
            (Some(lim), Some(off)) => {
                sql.push_str(" LIMIT ? OFFSET ?");
                params.push(serde_json::Value::Number(lim.into()));
                params.push(serde_json::Value::Number(off.into()));
            },
            (Some(lim), None) => {
                sql.push_str(" LIMIT ?");
                params.push(serde_json::Value::Number(lim.into()));
            },
            (None, Some(off)) => {
                // SQLite requires LIMIT with OFFSET; use -1 for unlimited
                sql.push_str(" LIMIT -1 OFFSET ?");
                params.push(serde_json::Value::Number(off.into()));
            },
            (None, None) => {},
        }

        self.execute_raw(&sql, params).await
    }

    fn database_type(&self) -> DatabaseType {
        DatabaseType::SQLite
    }

    fn supports_mutations(&self) -> bool {
        true
    }

    fn mutation_strategy(&self) -> MutationStrategy {
        MutationStrategy::DirectSql
    }

    async fn execute_direct_mutation(
        &self,
        ctx: &DirectMutationContext<'_>,
    ) -> Result<Vec<serde_json::Value>> {
        let (sql, bind_values) = super::helpers::build_direct_mutation_sql(ctx)?;

        let mut query = sqlx::query(&sql);
        for val in &bind_values {
            query = match val {
                serde_json::Value::String(s) => query.bind(s.clone()),
                serde_json::Value::Number(n) => {
                    if let Some(i) = n.as_i64() {
                        query.bind(i)
                    } else if let Some(f) = n.as_f64() {
                        query.bind(f)
                    } else {
                        query.bind(n.to_string())
                    }
                },
                serde_json::Value::Bool(b) => query.bind(*b),
                serde_json::Value::Null => query.bind(Option::<String>::None),
                serde_json::Value::Array(_) | serde_json::Value::Object(_) => {
                    query.bind(val.to_string())
                },
            };
        }

        let rows: Vec<SqliteRow> =
            query.fetch_all(&self.pool).await.map_err(|e| FraiseQLError::Database {
                message:   format!("SQLite direct mutation failed: {e}"),
                sql_state: None,
            })?;

        if rows.is_empty() {
            return Err(FraiseQLError::Validation {
                message: format!(
                    "Direct mutation on '{}' affected no rows — \
                     the target row may not exist or RLS filters rejected it",
                    ctx.table
                ),
                path:    None,
            });
        }

        let status = match ctx.operation {
            crate::traits::DirectMutationOp::Insert => "new",
            crate::traits::DirectMutationOp::Update => "updated",
            crate::traits::DirectMutationOp::Delete => "deleted",
        };

        let mut results = Vec::with_capacity(rows.len());
        for row in &rows {
            let entity = super::helpers::sqlite_row_to_json(row);

            // For INSERT, entity_id is null (new entity). For UPDATE/DELETE,
            // extract the primary key value from the first client column.
            let entity_id = match ctx.operation {
                crate::traits::DirectMutationOp::Insert => None,
                crate::traits::DirectMutationOp::Update
                | crate::traits::DirectMutationOp::Delete => {
                    ctx.values.first().map(|v| v.to_string().trim_matches('"').to_string())
                },
            };

            results.push(serde_json::json!({
                "status": status,
                "message": null,
                "entity_id": entity_id,
                "entity_type": ctx.return_type,
                "entity": entity,
                "updated_fields": null,
                "cascade": null,
                "metadata": null,
            }));
        }

        Ok(results)
    }

    async fn health_check(&self) -> Result<()> {
        sqlx::query("SELECT 1").fetch_one(&self.pool).await.map_err(|e| {
            FraiseQLError::Database {
                message:   format!("SQLite health check failed: {e}"),
                sql_state: None,
            }
        })?;

        Ok(())
    }

    #[allow(clippy::cast_possible_truncation)] // Reason: value is bounded; truncation cannot occur in practice
    fn pool_metrics(&self) -> PoolMetrics {
        let size = self.pool.size();
        let idle = self.pool.num_idle();

        PoolMetrics {
            total_connections:  size,
            idle_connections:   idle as u32,
            active_connections: size - idle as u32,
            waiting_requests:   0, // sqlx doesn't expose waiting count
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
        let rows: Vec<SqliteRow> =
            sqlx::query(sql)
                .fetch_all(&self.pool)
                .await
                .map_err(|e| FraiseQLError::Database {
                    message:   format!("SQLite query execution failed: {e}"),
                    sql_state: None,
                })?;

        // Convert each row to HashMap<String, Value>
        let results: Vec<std::collections::HashMap<String, serde_json::Value>> = rows
            .into_iter()
            .map(|row| {
                let mut map = std::collections::HashMap::new();

                // Iterate over all columns in the row
                for column in row.columns() {
                    let column_name = column.name().to_string();

                    // Try to extract value based on SQLite type
                    let value: serde_json::Value =
                        if let Ok(v) = row.try_get::<i32, _>(column_name.as_str()) {
                            serde_json::json!(v)
                        } else if let Ok(v) = row.try_get::<i64, _>(column_name.as_str()) {
                            serde_json::json!(v)
                        } else if let Ok(v) = row.try_get::<f64, _>(column_name.as_str()) {
                            serde_json::json!(v)
                        } else if let Ok(v) = row.try_get::<String, _>(column_name.as_str()) {
                            // Try to parse as JSON first
                            if let Ok(json_val) = serde_json::from_str::<serde_json::Value>(&v) {
                                json_val
                            } else {
                                serde_json::json!(v)
                            }
                        } else if let Ok(v) = row.try_get::<bool, _>(column_name.as_str()) {
                            serde_json::json!(v)
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

    async fn execute_parameterized_aggregate(
        &self,
        sql: &str,
        params: &[serde_json::Value],
    ) -> Result<Vec<std::collections::HashMap<String, serde_json::Value>>> {
        let mut query = sqlx::query(sql);
        for param in params {
            query = match param {
                serde_json::Value::String(s) => query.bind(s.clone()),
                serde_json::Value::Number(n) => {
                    if let Some(i) = n.as_i64() {
                        query.bind(i)
                    } else if let Some(f) = n.as_f64() {
                        query.bind(f)
                    } else {
                        query.bind(n.to_string())
                    }
                },
                serde_json::Value::Bool(b) => query.bind(*b),
                serde_json::Value::Null => query.bind(Option::<String>::None),
                serde_json::Value::Array(_) | serde_json::Value::Object(_) => {
                    query.bind(param.to_string())
                },
            };
        }

        let rows: Vec<SqliteRow> =
            query.fetch_all(&self.pool).await.map_err(|e| FraiseQLError::Database {
                message:   format!("SQLite parameterized aggregate query failed: {e}"),
                sql_state: None,
            })?;

        let results = rows
            .into_iter()
            .map(|row| {
                let mut map = std::collections::HashMap::new();
                for column in row.columns() {
                    let column_name = column.name().to_string();
                    let value: serde_json::Value =
                        if let Ok(v) = row.try_get::<i32, _>(column_name.as_str()) {
                            serde_json::json!(v)
                        } else if let Ok(v) = row.try_get::<i64, _>(column_name.as_str()) {
                            serde_json::json!(v)
                        } else if let Ok(v) = row.try_get::<f64, _>(column_name.as_str()) {
                            serde_json::json!(v)
                        } else if let Ok(v) = row.try_get::<String, _>(column_name.as_str()) {
                            if let Ok(json_val) = serde_json::from_str::<serde_json::Value>(&v) {
                                json_val
                            } else {
                                serde_json::json!(v)
                            }
                        } else if let Ok(v) = row.try_get::<bool, _>(column_name.as_str()) {
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
        use sqlx::Row as _;

        // Defense-in-depth: compiler-generated SQL should never contain a
        // semicolon, but guard against it to prevent statement injection.
        if sql.contains(';') {
            return Err(FraiseQLError::Validation {
                message: "EXPLAIN SQL must be a single statement".into(),
                path:    None,
            });
        }
        let explain_sql = format!("EXPLAIN QUERY PLAN {sql}");
        let rows: Vec<sqlx::sqlite::SqliteRow> = sqlx::query(&explain_sql)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| FraiseQLError::Database {
                message:   format!("SQLite EXPLAIN failed: {e}"),
                sql_state: None,
            })?;

        let steps: Vec<serde_json::Value> = rows
            .iter()
            .map(|row| {
                let detail: String = row.try_get("detail").unwrap_or_default();
                serde_json::json!({ "detail": detail })
            })
            .collect();

        Ok(serde_json::json!(steps))
    }
}

#[cfg(test)]
mod tests;
