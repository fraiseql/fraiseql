//! SQL Server database adapter implementation.

use async_trait::async_trait;
use bb8::Pool;
use bb8_tiberius::ConnectionManager;
use tiberius::Config;

use super::where_generator::SqlServerWhereGenerator;
use crate::{
    db::{
        traits::DatabaseAdapter,
        types::{DatabaseType, JsonbValue, PoolMetrics},
        where_clause::WhereClause,
    },
    error::{FraiseQLError, Result},
};

/// SQL Server database adapter with connection pooling.
///
/// Uses `tiberius` for native TDS protocol support and `bb8` for connection pooling.
///
/// # Example
///
/// ```rust,ignore
/// use fraiseql_core::db::sqlserver::SqlServerAdapter;
/// use fraiseql_core::db::{DatabaseAdapter, WhereClause, WhereOperator};
/// use serde_json::json;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// // Create adapter with connection string
/// let adapter = SqlServerAdapter::new("server=localhost;database=mydb;user=sa;password=Password123").await?;
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
pub struct SqlServerAdapter {
    pool: Pool<ConnectionManager>,
}

impl SqlServerAdapter {
    /// Create new SQL Server adapter with default pool configuration.
    ///
    /// # Arguments
    ///
    /// * `connection_string` - SQL Server connection string (ADO.NET format)
    ///
    /// # Connection String Format
    ///
    /// ```text
    /// server=localhost;database=mydb;user=sa;password=Password123
    /// server=localhost,1433;database=mydb;integratedSecurity=true
    /// ```
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::ConnectionPool` if pool creation fails.
    pub async fn new(connection_string: &str) -> Result<Self> {
        Self::with_pool_size(connection_string, 10).await
    }

    /// Create new SQL Server adapter with custom pool configuration.
    ///
    /// # Arguments
    ///
    /// * `connection_string` - SQL Server connection string
    /// * `min_size` - Minimum pool size (bb8 doesn't support this; connections are created
    ///   on-demand)
    /// * `max_size` - Maximum number of connections in pool
    ///
    /// # Note on min_size
    ///
    /// The bb8 connection pool used by this adapter creates connections on-demand
    /// rather than pre-creating a minimum pool size. The `min_size` parameter is accepted
    /// for API compatibility with other database adapters but does not affect behavior.
    /// If you need a minimum number of pre-created connections, consider:
    /// 1. Calling `warmup_connections()` after creating the pool
    /// 2. Increasing `max_size` to ensure sufficient capacity
    /// 3. Using a different connection pool strategy for your use case
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::ConnectionPool` if pool creation fails.
    pub async fn with_pool_config(
        connection_string: &str,
        min_size: u32,
        max_size: u32,
    ) -> Result<Self> {
        if min_size > 0 {
            eprintln!(
                "Warning: SQL Server adapter does not support min_size parameter (min_size={}) - connections are created on-demand. Consider warmup_connections() if pre-allocation is needed.",
                min_size
            );
        }
        Self::with_pool_size(connection_string, max_size).await
    }

    /// Create new SQL Server adapter with custom pool size.
    ///
    /// # Arguments
    ///
    /// * `connection_string` - SQL Server connection string
    /// * `max_size` - Maximum number of connections in pool
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::ConnectionPool` if pool creation fails.
    pub async fn with_pool_size(connection_string: &str, max_size: u32) -> Result<Self> {
        // Parse connection string into tiberius Config
        let config = Config::from_ado_string(connection_string).map_err(|e| {
            FraiseQLError::ConnectionPool {
                message: format!("Invalid SQL Server connection string: {e}"),
            }
        })?;

        let manager = ConnectionManager::new(config);

        let pool = Pool::builder().max_size(max_size).build(manager).await.map_err(|e| {
            FraiseQLError::ConnectionPool {
                message: format!("Failed to create SQL Server connection pool: {e}"),
            }
        })?;

        // Test connection
        {
            let mut conn = pool.get().await.map_err(|e| FraiseQLError::ConnectionPool {
                message: format!("Failed to acquire connection: {e}"),
            })?;

            conn.simple_query("SELECT 1").await.map_err(|e| FraiseQLError::Database {
                message:   format!("Failed to connect to SQL Server database: {e}"),
                sql_state: None,
            })?;
        }

        Ok(Self { pool })
    }

    /// Execute raw SQL query and return JSONB rows.
    async fn execute_raw(
        &self,
        sql: &str,
        params: Vec<serde_json::Value>,
    ) -> Result<Vec<JsonbValue>> {
        let mut conn = self.pool.get().await.map_err(|e| FraiseQLError::ConnectionPool {
            message: format!("Failed to acquire connection: {e}"),
        })?;

        // Build and execute query
        // Note: tiberius doesn't support dynamic parameter binding like sqlx
        // We need to use simple_query for dynamic SQL or build the query differently
        let rows = if params.is_empty() {
            let result = conn.simple_query(sql).await.map_err(|e| FraiseQLError::Database {
                message:   format!("SQL Server query execution failed: {e}"),
                sql_state: None,
            })?;
            result.into_first_result().await.map_err(|e| FraiseQLError::Database {
                message:   format!("Failed to get result set: {e}"),
                sql_state: None,
            })?
        } else {
            // For parameterized queries, we need to use Query with bind
            // This is simplified - in production you'd want proper parameter binding
            let mut query = tiberius::Query::new(sql);

            // We need to collect string representations for non-primitive types
            // to ensure they live long enough
            let mut string_params: Vec<String> = Vec::new();
            for param in &params {
                if !matches!(
                    param,
                    serde_json::Value::String(_)
                        | serde_json::Value::Number(_)
                        | serde_json::Value::Bool(_)
                        | serde_json::Value::Null
                ) {
                    string_params.push(param.to_string());
                }
            }

            let mut string_idx = 0;
            for param in &params {
                match param {
                    serde_json::Value::String(s) => query.bind(s.as_str()),
                    serde_json::Value::Number(n) => {
                        if let Some(i) = n.as_i64() {
                            query.bind(i);
                        } else if let Some(f) = n.as_f64() {
                            query.bind(f);
                        }
                    },
                    serde_json::Value::Bool(b) => query.bind(*b),
                    serde_json::Value::Null => query.bind(Option::<String>::None),
                    _ => {
                        query.bind(string_params[string_idx].as_str());
                        string_idx += 1;
                    },
                }
            }

            let result = query.query(&mut *conn).await.map_err(|e| FraiseQLError::Database {
                message:   format!("SQL Server query execution failed: {e}"),
                sql_state: None,
            })?;
            result.into_first_result().await.map_err(|e| FraiseQLError::Database {
                message:   format!("Failed to get result set: {e}"),
                sql_state: None,
            })?
        };

        // Process result set
        let mut results = Vec::new();

        for row in rows {
            // Try to get 'data' column as string and parse as JSON
            if let Some(data_str) = row.try_get::<&str, _>("data").ok().flatten() {
                let data: serde_json::Value =
                    serde_json::from_str(data_str).unwrap_or(serde_json::Value::Null);
                results.push(JsonbValue::new(data));
            } else {
                results.push(JsonbValue::new(serde_json::Value::Null));
            }
        }

        Ok(results)
    }
}

#[async_trait]
impl DatabaseAdapter for SqlServerAdapter {
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

        // Build SQL with SQL Server-specific JSON projection
        // The projection_template contains the SELECT clause with JSON functions
        // SQL Server uses square brackets for identifiers and TOP for LIMIT
        // e.g., "JSON_QUERY((SELECT data FOR JSON PATH, WITHOUT_ARRAY_WRAPPER))"
        let mut sql = if let Some(lim) = limit {
            format!(
                "SELECT TOP {} {} FROM [{}]",
                lim, projection.projection_template, view
            )
        } else {
            format!("SELECT {} FROM [{}]", projection.projection_template, view)
        };

        // Add WHERE clause if present
        if let Some(clause) = where_clause {
            let generator = super::where_generator::SqlServerWhereGenerator::new();
            let where_sql = generator.generate(clause)?;
            sql.push_str(" WHERE ");
            sql.push_str(&where_sql);
        }

        // Execute the query
        self.execute_raw(&sql).await
    }

    async fn execute_where_query(
        &self,
        view: &str,
        where_clause: Option<&WhereClause>,
        limit: Option<u32>,
        offset: Option<u32>,
    ) -> Result<Vec<JsonbValue>> {
        // Build base query - SQL Server uses square brackets for identifiers
        // SQL Server uses TOP instead of LIMIT, and OFFSET...FETCH for pagination
        let mut sql = if let Some(lim) = limit {
            if offset.is_some() {
                // With OFFSET, we need ORDER BY for pagination
                format!("SELECT data FROM [{view}]")
            } else {
                format!("SELECT TOP {lim} data FROM [{view}]")
            }
        } else {
            format!("SELECT data FROM [{view}]")
        };

        // Collect WHERE clause params (if any)
        let mut params: Vec<serde_json::Value> = Vec::new();
        let mut param_count = 0;

        // Add WHERE clause if present
        if let Some(clause) = where_clause {
            let generator = SqlServerWhereGenerator::new();
            let (where_sql, where_params) = generator.generate(clause)?;
            sql.push_str(" WHERE ");
            sql.push_str(&where_sql);
            param_count = where_params.len();
            params = where_params;
        }

        // Handle pagination with OFFSET...FETCH (requires ORDER BY)
        // SQL Server uses @p1, @p2, ... for parameters
        if let Some(off) = offset {
            sql.push_str(" ORDER BY (SELECT NULL)"); // Arbitrary ordering for pagination
            param_count += 1;
            sql.push_str(&format!(" OFFSET @p{param_count} ROWS"));
            params.push(serde_json::Value::Number(off.into()));
            if let Some(lim) = limit {
                param_count += 1;
                sql.push_str(&format!(" FETCH NEXT @p{param_count} ROWS ONLY"));
                params.push(serde_json::Value::Number(lim.into()));
            }
        }

        self.execute_raw(&sql, params).await
    }

    fn database_type(&self) -> DatabaseType {
        DatabaseType::SQLServer
    }

    async fn health_check(&self) -> Result<()> {
        let mut conn = self.pool.get().await.map_err(|e| FraiseQLError::ConnectionPool {
            message: format!("Failed to acquire connection: {e}"),
        })?;

        conn.simple_query("SELECT 1").await.map_err(|e| FraiseQLError::Database {
            message:   format!("SQL Server health check failed: {e}"),
            sql_state: None,
        })?;

        Ok(())
    }

    fn pool_metrics(&self) -> PoolMetrics {
        let state = self.pool.state();

        PoolMetrics {
            total_connections:  state.connections,
            idle_connections:   state.idle_connections,
            active_connections: state.connections - state.idle_connections,
            waiting_requests:   0, // bb8 doesn't expose waiting count directly
        }
    }

    async fn execute_raw_query(
        &self,
        sql: &str,
    ) -> Result<Vec<std::collections::HashMap<String, serde_json::Value>>> {
        let mut conn = self.pool.get().await.map_err(|e| FraiseQLError::ConnectionPool {
            message: format!("Failed to acquire connection: {e}"),
        })?;

        let result = conn.simple_query(sql).await.map_err(|e| FraiseQLError::Database {
            message:   format!("SQL Server query execution failed: {e}"),
            sql_state: None,
        })?;

        let rows = result.into_first_result().await.map_err(|e| FraiseQLError::Database {
            message:   format!("Failed to get result set: {e}"),
            sql_state: None,
        })?;

        // Convert each row to HashMap<String, Value>
        let results: Vec<std::collections::HashMap<String, serde_json::Value>> = rows
            .into_iter()
            .map(|row| {
                let mut map = std::collections::HashMap::new();

                // Iterate over all columns in the row
                for (idx, column) in row.columns().iter().enumerate() {
                    let column_name = column.name().to_string();

                    // Try to extract value based on SQL Server type
                    let value: serde_json::Value =
                        if let Some(v) = row.try_get::<i32, _>(idx).ok().flatten() {
                            serde_json::json!(v)
                        } else if let Some(v) = row.try_get::<i64, _>(idx).ok().flatten() {
                            serde_json::json!(v)
                        } else if let Some(v) = row.try_get::<f64, _>(idx).ok().flatten() {
                            serde_json::json!(v)
                        } else if let Some(v) = row.try_get::<&str, _>(idx).ok().flatten() {
                            // Try to parse as JSON first
                            if let Ok(json_val) = serde_json::from_str::<serde_json::Value>(v) {
                                json_val
                            } else {
                                serde_json::json!(v)
                            }
                        } else if let Some(v) = row.try_get::<bool, _>(idx).ok().flatten() {
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

#[cfg(all(test, feature = "test-sqlserver"))]
mod tests {
    use super::*;

    // Note: These tests require a running SQL Server instance with test data.
    // Run with: cargo test --features test-sqlserver -p fraiseql-core db::sqlserver::adapter

    const TEST_DB_URL: &str = "server=localhost,1434;database=fraiseql_test;user=sa;password=FraiseQL_Test1234;TrustServerCertificate=true";

    #[tokio::test]
    async fn test_adapter_creation() {
        let adapter = SqlServerAdapter::new(TEST_DB_URL)
            .await
            .expect("Failed to create SQL Server adapter");

        let metrics = adapter.pool_metrics();
        assert!(metrics.total_connections > 0);
        assert_eq!(adapter.database_type(), DatabaseType::SQLServer);
    }

    #[tokio::test]
    async fn test_health_check() {
        let adapter = SqlServerAdapter::new(TEST_DB_URL)
            .await
            .expect("Failed to create SQL Server adapter");

        adapter.health_check().await.expect("Health check failed");
    }

    #[tokio::test]
    async fn test_parameterized_limit_and_offset() {
        let adapter = SqlServerAdapter::new(TEST_DB_URL)
            .await
            .expect("Failed to create SQL Server adapter");

        // SQL Server requires ORDER BY for OFFSET...FETCH
        // This test just ensures parameterization works
        let results = adapter
            .execute_where_query("v_user", None, Some(2), Some(1))
            .await
            .expect("Failed to execute query");

        assert!(results.len() <= 2);
    }
}
