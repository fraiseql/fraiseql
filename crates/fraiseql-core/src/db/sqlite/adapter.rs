//! SQLite database adapter implementation.

use async_trait::async_trait;
use sqlx::{
    Column, Row,
    sqlite::{SqlitePool, SqlitePoolOptions, SqliteRow},
};

use super::where_generator::SqliteWhereGenerator;
use crate::{
    db::{
        traits::DatabaseAdapter,
        types::{DatabaseType, JsonbValue, PoolMetrics},
        where_clause::WhereClause,
    },
    error::{FraiseQLError, Result},
};

/// SQLite database adapter with connection pooling.
///
/// Uses `sqlx` for connection pooling and async queries.
/// Ideal for local development and testing.
///
/// # Example
///
/// ```rust,ignore
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

#[async_trait]
impl DatabaseAdapter for SqliteAdapter {
    async fn execute_with_projection(
        &self,
        view: &str,
        projection: Option<&crate::schema::SqlProjectionHint>,
        where_clause: Option<&WhereClause>,
        limit: Option<u32>,
    ) -> Result<Vec<JsonbValue>> {
        // For now, fall back to standard query until SQLite projection is optimized
        // TODO: Implement SQLite-specific json_object projection
        self.execute_where_query(view, where_clause, limit, None).await
    }

    async fn execute_where_query(
        &self,
        view: &str,
        where_clause: Option<&WhereClause>,
        limit: Option<u32>,
        offset: Option<u32>,
    ) -> Result<Vec<JsonbValue>> {
        // Build base query - SQLite uses double quotes for identifiers
        let mut sql = format!("SELECT data FROM \"{view}\"");

        // Collect WHERE clause params (if any)
        let mut params: Vec<serde_json::Value> = Vec::new();

        // Add WHERE clause if present
        if let Some(clause) = where_clause {
            let generator = SqliteWhereGenerator::new();
            let (where_sql, where_params) = generator.generate(clause)?;
            sql.push_str(" WHERE ");
            sql.push_str(&where_sql);
            params = where_params;
        }

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
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
