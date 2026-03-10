//! SQLite database adapter — **read-only** (queries only, no mutations).
//!
//! This adapter supports query execution (`execute_where_query`, `execute_raw_query`)
//! but does **not** implement [`MutationCapable`](crate::MutationCapable). Attempting
//! to compile a schema with mutations and run it against SQLite will produce a
//! **compile-time error** at the mutation executor call site.
//!
//! # When to use SQLite
//!
//! - Unit testing queries without a real database
//! - Schema exploration and local development (read-only)
//!
//! For mutation support, use PostgreSQL, MySQL, or SQL Server.

use async_trait::async_trait;

use sqlx::{
    Column, Row,
    sqlite::{SqlitePool, SqlitePoolOptions, SqliteRow},
};

use super::where_generator::SqliteWhereGenerator;
use crate::dialect::SqliteDialect;
use fraiseql_error::{FraiseQLError, Result};

use crate::{
    identifier::quote_sqlite_identifier,
    traits::DatabaseAdapter,
    types::{DatabaseType, JsonbValue, PoolMetrics},
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
        false
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

        let rows: Vec<SqliteRow> = query.fetch_all(&self.pool).await.map_err(|e| {
            FraiseQLError::Database {
                message:   format!("SQLite parameterized aggregate query failed: {e}"),
                sql_state: None,
            }
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
                path: None,
            });
        }
        let explain_sql = format!("EXPLAIN QUERY PLAN {sql}");
        let rows: Vec<sqlx::sqlite::SqliteRow> =
            sqlx::query(&explain_sql).fetch_all(&self.pool).await.map_err(|e| {
                FraiseQLError::Database {
                    message:   format!("SQLite EXPLAIN failed: {e}"),
                    sql_state: None,
                }
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
#[allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
mod tests {
    use serde_json::json;
    use sqlx::Executor as _;

    use super::*;

    /// Create an in-memory adapter and seed a `v_user` table with N rows.
    async fn setup_user_table(n: usize) -> SqliteAdapter {
        let adapter = SqliteAdapter::in_memory().await.expect("Failed to create SQLite adapter");
        adapter
            .pool
            .execute("CREATE TABLE \"v_user\" (id INTEGER PRIMARY KEY, data TEXT)")
            .await
            .expect("Failed to create v_user");
        for i in 1..=n {
            let row = format!(
                r#"INSERT INTO "v_user" (data) VALUES ('{{"id":{i},"name":"user{i}","age":{age},"active":{active},"score":{score},"deleted_at":null}}')"#,
                age = 20 + i,
                active = if i % 2 == 0 { "true" } else { "false" },
                score = i * 10,
            );
            adapter.pool.execute(row.as_str()).await.expect("Failed to insert row");
        }
        adapter
    }

    #[tokio::test]
    async fn test_in_memory_adapter_creation() {
        let adapter = SqliteAdapter::in_memory().await.expect("Failed to create SQLite adapter");

        let metrics = adapter.pool_metrics();
        assert!(metrics.total_connections > 0);
        assert_eq!(adapter.database_type(), DatabaseType::SQLite);
    }

    #[tokio::test]
    async fn test_health_check() {
        let adapter = SqliteAdapter::in_memory().await.expect("Failed to create SQLite adapter");

        adapter.health_check().await.expect("Health check failed");
    }

    #[tokio::test]
    async fn test_raw_query() {
        let adapter = SqliteAdapter::in_memory().await.expect("Failed to create SQLite adapter");

        // Create a test table
        sqlx::query("CREATE TABLE test_table (id INTEGER PRIMARY KEY, data TEXT)")
            .execute(&adapter.pool)
            .await
            .expect("Failed to create table");

        // Insert test data
        sqlx::query("INSERT INTO test_table (data) VALUES ('{\"name\": \"test\"}')")
            .execute(&adapter.pool)
            .await
            .expect("Failed to insert data");

        // Query the data
        let results = adapter
            .execute_raw_query("SELECT * FROM test_table")
            .await
            .expect("Failed to execute query");

        assert_eq!(results.len(), 1);
        assert!(results[0].contains_key("id"));
        assert!(results[0].contains_key("data"));
    }

    #[tokio::test]
    async fn test_parameterized_limit_only() {
        let adapter = SqliteAdapter::in_memory().await.expect("Failed to create SQLite adapter");

        // Create test table
        sqlx::query("CREATE TABLE \"v_user\" (id INTEGER PRIMARY KEY, data TEXT)")
            .execute(&adapter.pool)
            .await
            .expect("Failed to create table");

        // Insert test data
        for i in 1..=5 {
            sqlx::query(&format!(
                "INSERT INTO \"v_user\" (data) VALUES ('{{\"id\": {}, \"name\": \"user{}\"}}') ",
                i, i
            ))
            .execute(&adapter.pool)
            .await
            .expect("Failed to insert data");
        }

        let results = adapter
            .execute_where_query("v_user", None, Some(2), None)
            .await
            .expect("Failed to execute query");

        assert_eq!(results.len(), 2);
    }

    #[tokio::test]
    async fn test_parameterized_offset_only() {
        let adapter = SqliteAdapter::in_memory().await.expect("Failed to create SQLite adapter");

        // Create test table
        sqlx::query("CREATE TABLE \"v_user\" (id INTEGER PRIMARY KEY, data TEXT)")
            .execute(&adapter.pool)
            .await
            .expect("Failed to create table");

        // Insert test data
        for i in 1..=5 {
            sqlx::query(&format!(
                "INSERT INTO \"v_user\" (data) VALUES ('{{\"id\": {}, \"name\": \"user{}\"}}') ",
                i, i
            ))
            .execute(&adapter.pool)
            .await
            .expect("Failed to insert data");
        }

        let results = adapter
            .execute_where_query("v_user", None, None, Some(2))
            .await
            .expect("Failed to execute query");

        assert_eq!(results.len(), 3);
    }

    #[tokio::test]
    async fn test_parameterized_limit_and_offset() {
        let adapter = SqliteAdapter::in_memory().await.expect("Failed to create SQLite adapter");

        // Create test table
        sqlx::query("CREATE TABLE \"v_user\" (id INTEGER PRIMARY KEY, data TEXT)")
            .execute(&adapter.pool)
            .await
            .expect("Failed to create table");

        // Insert test data
        for i in 1..=5 {
            sqlx::query(&format!(
                "INSERT INTO \"v_user\" (data) VALUES ('{{\"id\": {}, \"name\": \"user{}\"}}') ",
                i, i
            ))
            .execute(&adapter.pool)
            .await
            .expect("Failed to insert data");
        }

        let results = adapter
            .execute_where_query("v_user", None, Some(2), Some(1))
            .await
            .expect("Failed to execute query");

        assert_eq!(results.len(), 2);
    }

    #[tokio::test]
    async fn test_function_call_returns_unsupported_error() {
        // Primary enforcement is at compile time: `SqliteAdapter` does not implement
        // `MutationCapable`, so the mutation executor won't accept it as a type parameter.
        // This test verifies the runtime fallback for the rare case where
        // `execute_function_call` is called directly on the `DatabaseAdapter` trait object.
        let adapter = SqliteAdapter::in_memory().await.expect("Failed to create SQLite adapter");

        let err = adapter
            .execute_function_call("fn_create_user", &[json!("alice")])
            .await
            .expect_err("Expected Unsupported error");

        assert!(
            matches!(err, FraiseQLError::Unsupported { .. }),
            "Expected Unsupported error, got: {err:?}"
        );
        assert!(
            err.to_string().contains("fn_create_user"),
            "Error message should name the function"
        );
    }

    // ── WHERE operator matrix ─────────────────────────────────────────────────

    #[tokio::test]
    async fn test_where_eq_operator() {
        let adapter = setup_user_table(5).await;
        let clause = WhereClause::Field {
            path:     vec!["name".to_string()],
            operator: crate::where_clause::WhereOperator::Eq,
            value:    json!("user3"),
        };
        let results =
            adapter.execute_where_query("v_user", Some(&clause), None, None).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].as_value()["name"], "user3");
    }

    #[tokio::test]
    async fn test_where_neq_operator() {
        let adapter = setup_user_table(3).await;
        let clause = WhereClause::Field {
            path:     vec!["name".to_string()],
            operator: crate::where_clause::WhereOperator::Neq,
            value:    json!("user1"),
        };
        let results =
            adapter.execute_where_query("v_user", Some(&clause), None, None).await.unwrap();
        assert_eq!(results.len(), 2);
    }

    #[tokio::test]
    async fn test_where_gt_operator() {
        let adapter = setup_user_table(5).await;
        // age = 20+i, so age > 23 → users 4 and 5
        let clause = WhereClause::Field {
            path:     vec!["age".to_string()],
            operator: crate::where_clause::WhereOperator::Gt,
            value:    json!(23),
        };
        let results =
            adapter.execute_where_query("v_user", Some(&clause), None, None).await.unwrap();
        assert_eq!(results.len(), 2);
    }

    #[tokio::test]
    async fn test_where_gte_operator() {
        let adapter = setup_user_table(5).await;
        // age >= 23 → users 3, 4, 5
        let clause = WhereClause::Field {
            path:     vec!["age".to_string()],
            operator: crate::where_clause::WhereOperator::Gte,
            value:    json!(23),
        };
        let results =
            adapter.execute_where_query("v_user", Some(&clause), None, None).await.unwrap();
        assert_eq!(results.len(), 3);
    }

    #[tokio::test]
    async fn test_where_lt_operator() {
        let adapter = setup_user_table(5).await;
        // age < 23 → users 1 and 2
        let clause = WhereClause::Field {
            path:     vec!["age".to_string()],
            operator: crate::where_clause::WhereOperator::Lt,
            value:    json!(23),
        };
        let results =
            adapter.execute_where_query("v_user", Some(&clause), None, None).await.unwrap();
        assert_eq!(results.len(), 2);
    }

    #[tokio::test]
    async fn test_where_lte_operator() {
        let adapter = setup_user_table(5).await;
        // age <= 23 → users 1, 2, 3
        let clause = WhereClause::Field {
            path:     vec!["age".to_string()],
            operator: crate::where_clause::WhereOperator::Lte,
            value:    json!(23),
        };
        let results =
            adapter.execute_where_query("v_user", Some(&clause), None, None).await.unwrap();
        assert_eq!(results.len(), 3);
    }

    #[tokio::test]
    async fn test_where_in_operator() {
        let adapter = setup_user_table(5).await;
        let clause = WhereClause::Field {
            path:     vec!["name".to_string()],
            operator: crate::where_clause::WhereOperator::In,
            value:    json!(["user1", "user3", "user5"]),
        };
        let results =
            adapter.execute_where_query("v_user", Some(&clause), None, None).await.unwrap();
        assert_eq!(results.len(), 3);
    }

    #[tokio::test]
    async fn test_where_not_in_operator() {
        let adapter = setup_user_table(5).await;
        let clause = WhereClause::Field {
            path:     vec!["name".to_string()],
            operator: crate::where_clause::WhereOperator::Nin,
            value:    json!(["user1", "user2"]),
        };
        let results =
            adapter.execute_where_query("v_user", Some(&clause), None, None).await.unwrap();
        assert_eq!(results.len(), 3);
    }

    #[tokio::test]
    async fn test_where_like_operator() {
        let adapter = setup_user_table(5).await;
        // name LIKE 'user%' matches all 5
        let clause = WhereClause::Field {
            path:     vec!["name".to_string()],
            operator: crate::where_clause::WhereOperator::Like,
            value:    json!("user%"),
        };
        let results =
            adapter.execute_where_query("v_user", Some(&clause), None, None).await.unwrap();
        assert_eq!(results.len(), 5);
    }

    #[tokio::test]
    async fn test_where_is_null_operator() {
        let adapter = setup_user_table(3).await;
        // deleted_at is null for all rows (seeded as null)
        let clause = WhereClause::Field {
            path:     vec!["deleted_at".to_string()],
            operator: crate::where_clause::WhereOperator::IsNull,
            value:    json!(true),
        };
        let results =
            adapter.execute_where_query("v_user", Some(&clause), None, None).await.unwrap();
        assert_eq!(results.len(), 3);
    }

    #[tokio::test]
    async fn test_where_is_not_null_operator() {
        let adapter = setup_user_table(3).await;
        // deleted_at is null → IS NOT NULL returns 0 rows
        let clause = WhereClause::Field {
            path:     vec!["deleted_at".to_string()],
            operator: crate::where_clause::WhereOperator::IsNull,
            value:    json!(false),
        };
        let results =
            adapter.execute_where_query("v_user", Some(&clause), None, None).await.unwrap();
        assert_eq!(results.len(), 0);
    }

    #[tokio::test]
    async fn test_where_multiple_conditions_and() {
        let adapter = setup_user_table(5).await;
        // name = "user2" AND age = 22
        let clause = WhereClause::And(vec![
            WhereClause::Field {
                path:     vec!["name".to_string()],
                operator: crate::where_clause::WhereOperator::Eq,
                value:    json!("user2"),
            },
            WhereClause::Field {
                path:     vec!["age".to_string()],
                operator: crate::where_clause::WhereOperator::Eq,
                value:    json!(22),
            },
        ]);
        let results =
            adapter.execute_where_query("v_user", Some(&clause), None, None).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].as_value()["name"], "user2");
    }

    #[tokio::test]
    async fn test_where_multiple_conditions_or() {
        let adapter = setup_user_table(5).await;
        // name = "user1" OR name = "user5"
        let clause = WhereClause::Or(vec![
            WhereClause::Field {
                path:     vec!["name".to_string()],
                operator: crate::where_clause::WhereOperator::Eq,
                value:    json!("user1"),
            },
            WhereClause::Field {
                path:     vec!["name".to_string()],
                operator: crate::where_clause::WhereOperator::Eq,
                value:    json!("user5"),
            },
        ]);
        let results =
            adapter.execute_where_query("v_user", Some(&clause), None, None).await.unwrap();
        assert_eq!(results.len(), 2);
    }

    // ── Error paths ───────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_empty_result_set() {
        let adapter = setup_user_table(3).await;
        let clause = WhereClause::Field {
            path:     vec!["name".to_string()],
            operator: crate::where_clause::WhereOperator::Eq,
            value:    json!("nonexistent"),
        };
        let results =
            adapter.execute_where_query("v_user", Some(&clause), None, None).await.unwrap();
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn test_invalid_raw_query_returns_error() {
        let adapter = SqliteAdapter::in_memory().await.unwrap();
        let err = adapter
            .execute_raw_query("SELECT * FROM nonexistent_table_xyz")
            .await
            .expect_err("Expected database error");
        assert!(matches!(err, FraiseQLError::Database { .. }));
    }

    // ── Pool metrics ──────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_pool_metrics_when_idle() {
        let adapter = SqliteAdapter::in_memory().await.unwrap();
        let metrics = adapter.pool_metrics();
        // Idle connections should be ≤ total
        assert!(metrics.idle_connections <= metrics.total_connections);
        assert_eq!(metrics.waiting_requests, 0);
    }

    // ── explain_query ─────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_explain_query_returns_plan() {
        let adapter = setup_user_table(3).await;
        let result = adapter
            .explain_query("SELECT data FROM \"v_user\"", &[])
            .await
            .expect("explain_query should succeed");
        // EXPLAIN QUERY PLAN returns at least one step
        assert!(result.as_array().is_some_and(|a| !a.is_empty()));
    }

    // ── Projection ────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_projection_filters_fields() {
        use crate::types::SqlProjectionHint;

        let adapter = setup_user_table(3).await;
        let projection = SqlProjectionHint {
            database:                    "sqlite".to_string(),
            projection_template:         "json_object('name', json_extract(data, '$.name')) AS data"
                .to_string(),
            estimated_reduction_percent: 50,
        };
        let results = adapter
            .execute_with_projection("v_user", Some(&projection), None, None)
            .await
            .expect("execute_with_projection should succeed");
        assert_eq!(results.len(), 3);
        // Only 'name' key is present; 'age' should be absent
        for row in &results {
            assert!(row.as_value().get("name").is_some());
            assert!(row.as_value().get("age").is_none());
        }
    }
}
