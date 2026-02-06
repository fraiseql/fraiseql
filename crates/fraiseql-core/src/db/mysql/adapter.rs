//! MySQL database adapter implementation.

use async_trait::async_trait;
use sqlx::{
    Column, Row,
    mysql::{MySqlPool, MySqlPoolOptions, MySqlRow},
};

use super::where_generator::MySqlWhereGenerator;
use crate::{
    db::{
        identifier::quote_mysql_identifier,
        traits::DatabaseAdapter,
        types::{DatabaseType, JsonbValue, PoolMetrics},
        where_clause::WhereClause,
    },
    error::{FraiseQLError, Result},
};

/// MySQL database adapter with connection pooling.
///
/// Uses `sqlx` for connection pooling and async queries.
///
/// # Example
///
/// ```rust,ignore
/// use fraiseql_core::db::mysql::MySqlAdapter;
/// use fraiseql_core::db::{DatabaseAdapter, WhereClause, WhereOperator};
/// use serde_json::json;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// // Create adapter with connection string
/// let adapter = MySqlAdapter::new("mysql://user:password@localhost/mydb").await?;
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
pub struct MySqlAdapter {
    pool: MySqlPool,
}

impl MySqlAdapter {
    /// Create new MySQL adapter with default pool configuration.
    ///
    /// # Arguments
    ///
    /// * `connection_string` - MySQL connection string (e.g., "mysql://user:pass@localhost/mydb")
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::ConnectionPool` if pool creation fails.
    pub async fn new(connection_string: &str) -> Result<Self> {
        Self::with_pool_size(connection_string, 10).await
    }

    /// Create new MySQL adapter with custom pool configuration.
    ///
    /// # Arguments
    ///
    /// * `connection_string` - MySQL connection string
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
        let pool = MySqlPoolOptions::new()
            .min_connections(min_size)
            .max_connections(max_size)
            .connect(connection_string)
            .await
            .map_err(|e| FraiseQLError::ConnectionPool {
                message: format!("Failed to create MySQL connection pool: {e}"),
            })?;

        Ok(Self { pool })
    }

    /// Create new MySQL adapter with custom pool size.
    ///
    /// # Arguments
    ///
    /// * `connection_string` - MySQL connection string
    /// * `max_size` - Maximum number of connections in pool
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::ConnectionPool` if pool creation fails.
    pub async fn with_pool_size(connection_string: &str, max_size: u32) -> Result<Self> {
        let pool = MySqlPoolOptions::new()
            .max_connections(max_size)
            .connect(connection_string)
            .await
            .map_err(|e| FraiseQLError::ConnectionPool {
                message: format!("Failed to create MySQL connection pool: {e}"),
            })?;

        // Test connection
        sqlx::query("SELECT 1")
            .fetch_one(&pool)
            .await
            .map_err(|e| FraiseQLError::Database {
                message:   format!("Failed to connect to MySQL database: {e}"),
                sql_state: None,
            })?;

        Ok(Self { pool })
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

        let rows: Vec<MySqlRow> =
            query.fetch_all(&self.pool).await.map_err(|e| FraiseQLError::Database {
                message:   format!("MySQL query execution failed: {e}"),
                sql_state: None,
            })?;

        let results = rows
            .into_iter()
            .map(|row| {
                // MySQL stores JSON in a column, get it directly
                let data: serde_json::Value =
                    row.try_get("data").unwrap_or(serde_json::Value::Null);
                JsonbValue::new(data)
            })
            .collect();

        Ok(results)
    }
}

#[async_trait]
impl DatabaseAdapter for MySqlAdapter {
    async fn execute_with_projection(
        &self,
        view: &str,
        projection: Option<&crate::schema::SqlProjectionHint>,
        where_clause: Option<&WhereClause>,
        limit: Option<u32>,
    ) -> Result<Vec<JsonbValue>> {
        // If no projection provided, fall back to standard query
        if projection.is_none() {
            return self.execute_where_query(view, where_clause, limit, None).await;
        }

        let projection = projection.unwrap();

        // Build SQL with MySQL-specific JSON_OBJECT projection
        // The projection_template contains the SELECT clause with JSON_OBJECT calls
        // e.g., "JSON_OBJECT('id', data->'$.id', 'email', data->'$.email')"
        let mut sql = format!(
            "SELECT {} FROM {}",
            projection.projection_template,
            quote_mysql_identifier(view)
        );

        // Collect WHERE clause params (if any)
        let mut params: Vec<serde_json::Value> = Vec::new();

        // Add WHERE clause if present
        if let Some(clause) = where_clause {
            let generator = super::where_generator::MySqlWhereGenerator::new();
            let (where_sql, where_params) = generator.generate(clause)?;
            sql.push_str(" WHERE ");
            sql.push_str(&where_sql);
            params = where_params;
        }

        // Add LIMIT if present
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
        // Build base query
        let mut sql = format!("SELECT data FROM {}", quote_mysql_identifier(view));

        // Collect WHERE clause params (if any)
        let mut params: Vec<serde_json::Value> = Vec::new();

        // Add WHERE clause if present
        if let Some(clause) = where_clause {
            let generator = MySqlWhereGenerator::new();
            let (where_sql, where_params) = generator.generate(clause)?;
            sql.push_str(" WHERE ");
            sql.push_str(&where_sql);
            params = where_params;
        }

        // Add LIMIT and OFFSET
        // Note: MySQL requires LIMIT when using OFFSET, so we use a large number for "unlimited"
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
                // MySQL requires LIMIT with OFFSET; use large number for "unlimited"
                // MySQL's max is 18446744073709551615, but we use a practical large value
                sql.push_str(" LIMIT 18446744073709551615 OFFSET ?");
                params.push(serde_json::Value::Number(off.into()));
            },
            (None, None) => {},
        }

        self.execute_raw(&sql, params).await
    }

    fn database_type(&self) -> DatabaseType {
        DatabaseType::MySQL
    }

    async fn health_check(&self) -> Result<()> {
        sqlx::query("SELECT 1").fetch_one(&self.pool).await.map_err(|e| {
            FraiseQLError::Database {
                message:   format!("MySQL health check failed: {e}"),
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
        let rows: Vec<MySqlRow> =
            sqlx::query(sql)
                .fetch_all(&self.pool)
                .await
                .map_err(|e| FraiseQLError::Database {
                    message:   format!("MySQL query execution failed: {e}"),
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

                    // Try to extract value based on MySQL type
                    let value: serde_json::Value = if let Ok(v) =
                        row.try_get::<i32, _>(column_name.as_str())
                    {
                        serde_json::json!(v)
                    } else if let Ok(v) = row.try_get::<i64, _>(column_name.as_str()) {
                        serde_json::json!(v)
                    } else if let Ok(v) = row.try_get::<f64, _>(column_name.as_str()) {
                        serde_json::json!(v)
                    } else if let Ok(v) = row.try_get::<String, _>(column_name.as_str()) {
                        serde_json::json!(v)
                    } else if let Ok(v) = row.try_get::<bool, _>(column_name.as_str()) {
                        serde_json::json!(v)
                    } else if let Ok(v) = row.try_get::<serde_json::Value, _>(column_name.as_str())
                    {
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
}

#[cfg(all(test, feature = "test-mysql"))]
mod tests {
    use super::*;

    // Note: These tests require a running MySQL instance with test data.
    // Run with: cargo test --features test-mysql -p fraiseql-core db::mysql::adapter

    const TEST_DB_URL: &str =
        "mysql://fraiseql_test:fraiseql_test_password@localhost:3307/test_fraiseql";

    #[tokio::test]
    async fn test_adapter_creation() {
        let adapter = MySqlAdapter::new(TEST_DB_URL).await.expect("Failed to create MySQL adapter");

        let metrics = adapter.pool_metrics();
        assert!(metrics.total_connections > 0);
        assert_eq!(adapter.database_type(), DatabaseType::MySQL);
    }

    #[tokio::test]
    async fn test_health_check() {
        let adapter = MySqlAdapter::new(TEST_DB_URL).await.expect("Failed to create MySQL adapter");

        adapter.health_check().await.expect("Health check failed");
    }

    #[tokio::test]
    async fn test_parameterized_limit_only() {
        let adapter = MySqlAdapter::new(TEST_DB_URL).await.expect("Failed to create MySQL adapter");

        let results = adapter
            .execute_where_query("v_user", None, Some(2), None)
            .await
            .expect("Failed to execute query");

        assert!(results.len() <= 2);
    }

    #[tokio::test]
    async fn test_parameterized_offset_only() {
        let adapter = MySqlAdapter::new(TEST_DB_URL).await.expect("Failed to create MySQL adapter");

        let results = adapter
            .execute_where_query("v_user", None, None, Some(1))
            .await
            .expect("Failed to execute query");

        assert!(!results.is_empty());
    }

    #[tokio::test]
    async fn test_parameterized_limit_and_offset() {
        let adapter = MySqlAdapter::new(TEST_DB_URL).await.expect("Failed to create MySQL adapter");

        let results = adapter
            .execute_where_query("v_user", None, Some(2), Some(1))
            .await
            .expect("Failed to execute query");

        assert!(results.len() <= 2);
    }
}
