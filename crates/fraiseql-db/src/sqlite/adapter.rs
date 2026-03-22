//! SQLite database adapter with query and mutation support.
//!
//! This adapter supports query execution (`execute_where_query`, `execute_raw_query`)
//! and **direct-SQL mutations** (INSERT/UPDATE/DELETE with RETURNING).
//!
//! Unlike PostgreSQL/MySQL/SQL Server, SQLite has no stored procedures. Mutations
//! are executed as direct SQL statements rather than function calls. The adapter
//! implements [`MutationCapable`](crate::MutationCapable) and uses
//! [`MutationStrategy::DirectSql`](crate::MutationStrategy::DirectSql).
//!
//! # Requirements
//!
//! - SQLite 3.35.0+ (2021-03-12) for `RETURNING` clause support.
//!
//! # Limitations
//!
//! - `MutationOperation::Custom` mutations are not supported (no stored procedures).
//! - No server-side validation logic beyond constraint checking.
//! - Each mutation is a single atomic statement (no multi-statement transactions).
//!
//! # When to use SQLite
//!
//! - Local development with full read/write cycles
//! - Unit testing queries and mutations without a real database
//! - Schema exploration

use async_trait::async_trait;
use fraiseql_error::{FraiseQLError, Result};
use sqlx::{
    Column, Row,
    sqlite::{SqlitePool, SqlitePoolOptions, SqliteRow},
};

use super::helpers::{build_direct_mutation_sql, sqlite_row_to_json};
use super::where_generator::SqliteWhereGenerator;
use crate::{
    dialect::SqliteDialect,
    identifier::quote_sqlite_identifier,
    traits::{
        DatabaseAdapter, DirectMutationContext, DirectMutationOp, MutationCapable, MutationStrategy,
    },
    types::{DatabaseType, JsonbValue, PoolMetrics},
    where_clause::WhereClause,
};

/// Hard upper bound on pool size to prevent resource exhaustion.
const MAX_POOL_SIZE: u32 = 200;

/// SQLite database adapter with connection pooling.
///
/// Uses `sqlx` for connection pooling and async queries.
/// Ideal for local development and testing.
///
/// # Example
///
/// ```no_run
/// use fraiseql_core::db::sqlite::SqliteAdapter;
/// use fraiseql_core::db::{DatabaseAdapter, WhereClause, WhereOperator};
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
///     .execute_where_query("v_user", Some(&where_clause), Some(10), None)
///     .await?;
///
/// println!("Found {} users", results.len());
/// # Ok(())
/// # }
/// ```
#[derive(Clone)]
pub struct SqliteAdapter {
    pool: SqlitePool,
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
        if max_size == 0 || max_size > MAX_POOL_SIZE {
            return Err(FraiseQLError::Validation {
                message: format!(
                    "Pool size must be between 1 and {MAX_POOL_SIZE}, got {max_size}"
                ),
                path:    None,
            });
        }

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
    ) -> Result<Vec<JsonbValue>> {
        // If no projection provided, fall back to standard query
        if projection.is_none() {
            return self.execute_where_query(view, where_clause, limit, None).await;
        }

        let projection = projection.expect("projection is Some; None was returned above");

        // Build SQL with SQLite-specific json_object projection
        // The projection_template contains the SELECT clause with json_object() calls
        // SQLite uses double quotes for identifiers, not backticks
        // e.g., "json_object('id', data->'$.id', 'email', data->'$.email')"
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

        // Add LIMIT if present (SQLite uses LIMIT before OFFSET)
        if let Some(lim) = limit {
            sql.push_str(&format!(" LIMIT {lim}"));
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
    ) -> Result<Vec<JsonbValue>> {
        // Build base query - SQLite uses double quotes for identifiers
        // SAFETY: view is schema-derived (from CompiledSchema, validated at compile time),
        // not user input. Additionally passed through quote_sqlite_identifier().
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
    ) -> Result<Vec<std::collections::HashMap<String, serde_json::Value>>> {
        let (sql, bind_values) = build_direct_mutation_sql(ctx)?;

        // Bind parameters and execute
        let mut query = sqlx::query(&sql);
        for value in bind_values {
            query = match value {
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
                    query.bind(value.to_string())
                },
            };
        }

        let row_opt = query
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| FraiseQLError::Database {
                message:   format!("SQLite direct mutation failed: {e}"),
                sql_state: None,
            })?;

        let Some(row) = row_opt else {
            // DELETE/UPDATE of non-existent row
            return Err(FraiseQLError::Validation {
                message: format!(
                    "Mutation on table '{}' affected no rows (entity not found)",
                    ctx.table
                ),
                path: None,
            });
        };

        // Convert the RETURNING * row into a JSON object for the `entity` field
        let entity = sqlite_row_to_json(&row);

        // Determine status and entity_id based on operation
        let status = match ctx.operation {
            DirectMutationOp::Insert => "new",
            DirectMutationOp::Update => "updated",
            DirectMutationOp::Delete => "deleted",
        };

        // For UPDATE/DELETE, extract the PK value as entity_id
        let entity_id: serde_json::Value = match ctx.operation {
            DirectMutationOp::Update | DirectMutationOp::Delete => {
                // First value is the PK
                match &ctx.values[0] {
                    serde_json::Value::Number(n) => serde_json::json!(n.to_string()),
                    v => serde_json::json!(v.to_string().trim_matches('"')),
                }
            },
            DirectMutationOp::Insert => serde_json::Value::Null,
        };

        let mut result = std::collections::HashMap::new();
        result.insert("status".into(), serde_json::json!(status));
        result.insert("message".into(), serde_json::Value::Null);
        result.insert("entity".into(), entity);
        result.insert("entity_type".into(), serde_json::json!(ctx.return_type));
        result.insert("entity_id".into(), entity_id);
        result.insert("cascade".into(), serde_json::Value::Null);
        result.insert("metadata".into(), serde_json::Value::Null);
        Ok(vec![result])
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

    #[allow(clippy::cast_possible_truncation)] // Reason: pool sizes are always far below u32::MAX in practice
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
        // SAFETY: sql is compiler-generated from schema-derived sources, not user input.
        // Defense-in-depth: semicolons are rejected above.
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

impl MutationCapable for SqliteAdapter {}

#[cfg(test)]
#[allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#[path = "adapter_tests.rs"]
mod tests;
