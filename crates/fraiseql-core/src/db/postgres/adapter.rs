//! PostgreSQL database adapter implementation.

use async_trait::async_trait;
use deadpool_postgres::{Config, ManagerConfig, Pool, RecyclingMethod, Runtime};
use tokio_postgres::{NoTls, Row};

use super::where_generator::PostgresWhereGenerator;
use crate::{
    db::{
        traits::DatabaseAdapter,
        types::{DatabaseType, JsonbValue, PoolMetrics, QueryParam},
        where_clause::WhereClause,
    },
    error::{FraiseQLError, Result},
    schema::SqlProjectionHint,
};

/// PostgreSQL database adapter with connection pooling.
///
/// Uses `deadpool-postgres` for connection pooling and `tokio-postgres` for async queries.
///
/// # Example
///
/// ```rust,no_run
/// use fraiseql_core::db::postgres::PostgresAdapter;
/// use fraiseql_core::db::{DatabaseAdapter, WhereClause, WhereOperator};
/// use serde_json::json;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// // Create adapter with connection string
/// let adapter = PostgresAdapter::new("postgresql://localhost/mydb").await?;
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
pub struct PostgresAdapter {
    pool: Pool,
}

impl PostgresAdapter {
    /// Create new PostgreSQL adapter with default pool configuration.
    ///
    /// # Arguments
    ///
    /// * `connection_string` - PostgreSQL connection string (e.g., "postgresql://localhost/mydb")
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::ConnectionPool` if pool creation fails.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use fraiseql_core::db::postgres::PostgresAdapter;
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let adapter = PostgresAdapter::new("postgresql://localhost/mydb").await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn new(connection_string: &str) -> Result<Self> {
        Self::with_pool_size(connection_string, 10).await
    }

    /// Create new PostgreSQL adapter with custom pool configuration.
    ///
    /// # Arguments
    ///
    /// * `connection_string` - PostgreSQL connection string
    /// * `min_size` - Minimum size hint (not enforced by deadpool-postgres)
    /// * `max_size` - Maximum number of connections in pool
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::ConnectionPool` if pool creation fails.
    ///
    /// # Note
    ///
    /// `min_size` is accepted for API compatibility but deadpool-postgres uses
    /// lazy initialization with dynamic pool sizing up to `max_size`.
    pub async fn with_pool_config(
        connection_string: &str,
        _min_size: usize,
        max_size: usize,
    ) -> Result<Self> {
        Self::with_pool_size(connection_string, max_size).await
    }

    /// Create new PostgreSQL adapter with custom pool size.
    ///
    /// # Arguments
    ///
    /// * `connection_string` - PostgreSQL connection string
    /// * `max_size` - Maximum number of connections in pool
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::ConnectionPool` if pool creation fails.
    pub async fn with_pool_size(connection_string: &str, max_size: usize) -> Result<Self> {
        let mut cfg = Config::new();
        cfg.url = Some(connection_string.to_string());
        cfg.manager = Some(ManagerConfig {
            recycling_method: RecyclingMethod::Fast,
        });
        cfg.pool = Some(deadpool_postgres::PoolConfig::new(max_size));

        let pool = cfg.create_pool(Some(Runtime::Tokio1), NoTls).map_err(|e| {
            FraiseQLError::ConnectionPool {
                message: format!("Failed to create connection pool: {e}"),
            }
        })?;

        // Test connection
        let client = pool.get().await.map_err(|e| FraiseQLError::ConnectionPool {
            message: format!("Failed to acquire connection: {e}"),
        })?;

        client.query("SELECT 1", &[]).await.map_err(|e| FraiseQLError::Database {
            message:   format!("Failed to connect to database: {e}"),
            sql_state: e.code().map(|c| c.code().to_string()),
        })?;

        Ok(Self { pool })
    }

    /// Get a reference to the internal connection pool.
    ///
    /// This allows sharing the pool with other components like `PostgresIntrospector`.
    #[must_use]
    pub fn pool(&self) -> &Pool {
        &self.pool
    }

    /// Execute raw SQL query and return JSONB rows.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Database` on query execution failure.
    async fn execute_raw(
        &self,
        sql: &str,
        params: &[&(dyn tokio_postgres::types::ToSql + Sync)],
    ) -> Result<Vec<JsonbValue>> {
        let client = self.pool.get().await.map_err(|e| FraiseQLError::ConnectionPool {
            message: format!("Failed to acquire connection: {e}"),
        })?;

        let rows: Vec<Row> =
            client.query(sql, params).await.map_err(|e| FraiseQLError::Database {
                message:   format!("Query execution failed: {e}"),
                sql_state: e.code().map(|c| c.code().to_string()),
            })?;

        let results = rows
            .into_iter()
            .map(|row| {
                let data: serde_json::Value = row.get(0);
                JsonbValue::new(data)
            })
            .collect();

        Ok(results)
    }

    /// Execute query with SQL field projection optimization.
    ///
    /// Uses the provided `SqlProjectionHint` to generate optimized SQL that projects
    /// only the requested fields from the JSONB column, reducing network payload and
    /// JSON deserialization overhead.
    ///
    /// # Arguments
    ///
    /// * `view` - View/table name to query
    /// * `projection` - Optional SQL projection hint with field list
    /// * `where_clause` - Optional WHERE clause for filtering
    /// * `limit` - Optional row limit
    ///
    /// # Returns
    ///
    /// Vector of projected JSONB rows with only the requested fields
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Database` on query execution failure.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let projection = SqlProjectionHint {
    ///     database: "postgresql".to_string(),
    ///     projection_template: "...".to_string(),
    ///     estimated_reduction_percent: 75,
    /// };
    ///
    /// let results = adapter
    ///     .execute_with_projection("v_user", Some(&projection), None, Some(10))
    ///     .await?;
    /// ```
    pub async fn execute_with_projection(
        &self,
        view: &str,
        projection: Option<&SqlProjectionHint>,
        where_clause: Option<&WhereClause>,
        limit: Option<u32>,
    ) -> Result<Vec<JsonbValue>> {
        // If no projection, fall back to standard query
        if projection.is_none() {
            return self.execute_where_query(view, where_clause, limit, None).await;
        }

        let projection = projection.unwrap();

        // Build SQL with projection
        // The projection_template is expected to be the SELECT clause with projection SQL
        // e.g., "jsonb_build_object('id', data->>'id', 'email', data->>'email')"
        let mut sql = format!("SELECT {} FROM \"{}\"", projection.projection_template, view);

        // Add WHERE clause if present
        if let Some(clause) = where_clause {
            let generator = PostgresWhereGenerator::new();
            let (where_sql, where_params) = generator.generate(clause)?;
            sql.push_str(" WHERE ");
            sql.push_str(&where_sql);

            // Add parameterized LIMIT
            let mut params = where_params;
            let mut param_count = params.len();

            if let Some(lim) = limit {
                param_count += 1;
                sql.push_str(&format!(" LIMIT ${param_count}"));
                params.push(serde_json::Value::Number(lim.into()));
            }

            // Convert JSON values to QueryParam (preserves types)
            let typed_params: Vec<QueryParam> =
                params.into_iter().map(QueryParam::from).collect();

            eprintln!("DEBUG: SQL with projection = {}", sql);
            eprintln!("DEBUG: typed_params = {:?}", typed_params);

            // Create references to QueryParam for ToSql
            let param_refs: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> = typed_params
                .iter()
                .map(|p| p as &(dyn tokio_postgres::types::ToSql + Sync))
                .collect();

            self.execute_raw(&sql, &param_refs).await
        } else {
            // No WHERE clause
            let mut params: Vec<serde_json::Value> = vec![];
            let mut param_count = 0;

            if let Some(lim) = limit {
                param_count += 1;
                sql.push_str(&format!(" LIMIT ${param_count}"));
                params.push(serde_json::Value::Number(lim.into()));
            }

            // Convert JSON values to QueryParam (preserves types)
            let typed_params: Vec<QueryParam> =
                params.into_iter().map(QueryParam::from).collect();

            // Create references to QueryParam for ToSql
            let param_refs: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> = typed_params
                .iter()
                .map(|p| p as &(dyn tokio_postgres::types::ToSql + Sync))
                .collect();

            self.execute_raw(&sql, &param_refs).await
        }
    }
}

#[async_trait]
impl DatabaseAdapter for PostgresAdapter {
    async fn execute_where_query(
        &self,
        view: &str,
        where_clause: Option<&WhereClause>,
        limit: Option<u32>,
        offset: Option<u32>,
    ) -> Result<Vec<JsonbValue>> {
        // Build base query
        let mut sql = format!("SELECT data FROM {view}");

        // Add WHERE clause if present
        if let Some(clause) = where_clause {
            let generator = PostgresWhereGenerator::new();
            let (where_sql, where_params) = generator.generate(clause)?;
            sql.push_str(" WHERE ");
            sql.push_str(&where_sql);

            // Add parameterized LIMIT and OFFSET
            let mut params = where_params;
            let mut param_count = params.len();

            if let Some(lim) = limit {
                param_count += 1;
                sql.push_str(&format!(" LIMIT ${param_count}"));
                params.push(serde_json::Value::Number(lim.into()));
            }

            if let Some(off) = offset {
                param_count += 1;
                sql.push_str(&format!(" OFFSET ${param_count}"));
                params.push(serde_json::Value::Number(off.into()));
            }

            // Convert JSON values to QueryParam (preserves types)
            let typed_params: Vec<QueryParam> =
                params.into_iter().map(QueryParam::from).collect();

            eprintln!("DEBUG: SQL = {}", sql);
            eprintln!("DEBUG: typed_params = {:?}", typed_params);

            // Create references to QueryParam for ToSql
            let param_refs: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> = typed_params
                .iter()
                .map(|p| p as &(dyn tokio_postgres::types::ToSql + Sync))
                .collect();

            self.execute_raw(&sql, &param_refs).await
        } else {
            // No WHERE clause - execute simple query
            let mut params: Vec<serde_json::Value> = vec![];
            let mut param_count = 0;

            if let Some(lim) = limit {
                param_count += 1;
                sql.push_str(&format!(" LIMIT ${param_count}"));
                params.push(serde_json::Value::Number(lim.into()));
            }

            if let Some(off) = offset {
                param_count += 1;
                sql.push_str(&format!(" OFFSET ${param_count}"));
                params.push(serde_json::Value::Number(off.into()));
            }

            // Convert JSON values to QueryParam (preserves types)
            let typed_params: Vec<QueryParam> =
                params.into_iter().map(QueryParam::from).collect();

            // Create references to QueryParam for ToSql
            let param_refs: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> = typed_params
                .iter()
                .map(|p| p as &(dyn tokio_postgres::types::ToSql + Sync))
                .collect();

            self.execute_raw(&sql, &param_refs).await
        }
    }

    fn database_type(&self) -> DatabaseType {
        DatabaseType::PostgreSQL
    }

    async fn health_check(&self) -> Result<()> {
        let client = self.pool.get().await.map_err(|e| FraiseQLError::ConnectionPool {
            message: format!("Failed to acquire connection: {e}"),
        })?;

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
        let client = self.pool.get().await.map_err(|e| FraiseQLError::ConnectionPool {
            message: format!("Failed to acquire connection: {e}"),
        })?;

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
}

/// PostgreSQL integration tests.
///
/// These tests require a running PostgreSQL database with test data.
///
/// ## Running the tests
///
/// ```bash
/// # Start test database
/// docker compose -f docker-compose.test.yml up -d postgres-test
///
/// # Run tests with the test-postgres feature
/// cargo test -p fraiseql-core --features test-postgres db::postgres::adapter::tests
///
/// # Or run all tests including ignored ones (legacy method)
/// cargo test -p fraiseql-core -- --ignored
///
/// # Stop test database
/// docker compose -f docker-compose.test.yml down
/// ```
#[cfg(all(test, feature = "test-postgres"))]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::db::{WhereClause, WhereOperator};

    const TEST_DB_URL: &str =
        "postgresql://fraiseql_test:fraiseql_test_password@localhost:5433/test_fraiseql";

    // Helper to create test adapter
    async fn create_test_adapter() -> PostgresAdapter {
        PostgresAdapter::new(TEST_DB_URL)
            .await
            .expect("Failed to create test adapter - is PostgreSQL running? Use: docker compose -f docker-compose.test.yml up -d postgres-test")
    }

    // ========================================================================
    // Connection & Adapter Tests
    // ========================================================================

    #[tokio::test]
    async fn test_adapter_creation() {
        let adapter = create_test_adapter().await;
        let metrics = adapter.pool_metrics();
        assert!(metrics.total_connections > 0);
        assert_eq!(adapter.database_type(), DatabaseType::PostgreSQL);
    }

    #[tokio::test]
    async fn test_adapter_with_custom_pool_size() {
        let adapter = PostgresAdapter::with_pool_size(TEST_DB_URL, 5)
            .await
            .expect("Failed to create adapter");

        // Pool starts with 1 connection and grows on demand up to max_size
        let metrics = adapter.pool_metrics();
        assert!(metrics.total_connections >= 1, "Pool should have at least 1 connection");
        assert!(metrics.total_connections <= 5, "Pool should not exceed max_size of 5");
    }

    #[tokio::test]
    async fn test_health_check() {
        let adapter = create_test_adapter().await;
        adapter.health_check().await.expect("Health check failed");
    }

    #[tokio::test]
    async fn test_pool_metrics() {
        let adapter = create_test_adapter().await;
        let metrics = adapter.pool_metrics();

        assert!(metrics.total_connections > 0);
        assert!(metrics.idle_connections <= metrics.total_connections);
        assert_eq!(
            metrics.active_connections,
            metrics.total_connections - metrics.idle_connections
        );
    }

    // ========================================================================
    // Simple Query Tests (No WHERE Clause)
    // ========================================================================

    #[tokio::test]
    async fn test_query_all_users() {
        let adapter = create_test_adapter().await;

        let results = adapter
            .execute_where_query("v_user", None, None, None)
            .await
            .expect("Failed to query users");

        assert_eq!(results.len(), 5, "Should have 5 test users");

        // Verify JSONB structure
        let first_user = results[0].as_value();
        assert!(first_user.get("id").is_some());
        assert!(first_user.get("email").is_some());
        assert!(first_user.get("name").is_some());
    }

    #[tokio::test]
    async fn test_query_all_posts() {
        let adapter = create_test_adapter().await;

        let results = adapter
            .execute_where_query("v_post", None, None, None)
            .await
            .expect("Failed to query posts");

        assert_eq!(results.len(), 4, "Should have 4 test posts");

        // Verify nested author object
        let first_post = results[0].as_value();
        assert!(first_post.get("author").is_some());
        assert!(first_post["author"].get("name").is_some());
    }

    // ========================================================================
    // WHERE Clause Tests - Comparison Operators
    // ========================================================================

    #[tokio::test]
    async fn test_where_eq() {
        let adapter = create_test_adapter().await;

        let where_clause = WhereClause::Field {
            path:     vec!["email".to_string()],
            operator: WhereOperator::Eq,
            value:    json!("alice@example.com"),
        };

        let results = adapter
            .execute_where_query("v_user", Some(&where_clause), None, None)
            .await
            .expect("Failed to execute query");

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].as_value()["email"], "alice@example.com");
    }

    #[tokio::test]
    async fn test_where_neq() {
        let adapter = create_test_adapter().await;

        let where_clause = WhereClause::Field {
            path:     vec!["role".to_string()],
            operator: WhereOperator::Neq,
            value:    json!("user"),
        };

        let results = adapter
            .execute_where_query("v_user", Some(&where_clause), None, None)
            .await
            .expect("Failed to execute query");

        // Should return admin and moderator (not regular users)
        assert!(results.len() >= 2);
        for result in &results {
            assert_ne!(result.as_value()["role"], "user");
        }
    }

    #[tokio::test]
    async fn test_where_gt() {
        let adapter = create_test_adapter().await;

        let where_clause = WhereClause::Field {
            path:     vec!["age".to_string()],
            operator: WhereOperator::Gt,
            value:    json!(30),
        };

        let results = adapter
            .execute_where_query("v_user", Some(&where_clause), None, None)
            .await
            .expect("Failed to execute query");

        assert!(!results.is_empty(), "Should return at least one result");
        assert_eq!(results.len(), 1, "Should return exactly 1 user (Charlie with age 35)");

        for result in &results {
            let age = result.as_value()["age"].as_i64().unwrap();
            assert!(age > 30, "Age should be > 30, but got {}", age);
        }
    }

    #[tokio::test]
    async fn test_where_gte() {
        let adapter = create_test_adapter().await;

        let where_clause = WhereClause::Field {
            path:     vec!["age".to_string()],
            operator: WhereOperator::Gte,
            value:    json!(30),
        };

        let results = adapter
            .execute_where_query("v_user", Some(&where_clause), None, None)
            .await
            .expect("Failed to execute query");

        for result in &results {
            let age = result.as_value()["age"].as_i64().unwrap();
            assert!(age >= 30);
        }
    }

    // ========================================================================
    // WHERE Clause Tests - String Operators
    // ========================================================================

    #[tokio::test]
    async fn test_where_icontains() {
        let adapter = create_test_adapter().await;

        let where_clause = WhereClause::Field {
            path:     vec!["email".to_string()],
            operator: WhereOperator::Icontains,
            value:    json!("example.com"),
        };

        let results = adapter
            .execute_where_query("v_user", Some(&where_clause), None, None)
            .await
            .expect("Failed to execute query");

        assert!(results.len() >= 3);
        for result in &results {
            let email = result.as_value()["email"].as_str().unwrap();
            assert!(email.to_lowercase().contains("example.com"));
        }
    }

    #[tokio::test]
    async fn test_where_startswith() {
        let adapter = create_test_adapter().await;

        let where_clause = WhereClause::Field {
            path:     vec!["name".to_string()],
            operator: WhereOperator::Startswith,
            value:    json!("Alice"),
        };

        let results = adapter
            .execute_where_query("v_user", Some(&where_clause), None, None)
            .await
            .expect("Failed to execute query");

        assert_eq!(results.len(), 1);
        assert!(results[0].as_value()["name"].as_str().unwrap().starts_with("Alice"));
    }

    // ========================================================================
    // WHERE Clause Tests - Logical Operators
    // ========================================================================

    #[tokio::test]
    async fn test_where_and() {
        let adapter = create_test_adapter().await;

        let where_clause = WhereClause::And(vec![
            WhereClause::Field {
                path:     vec!["active".to_string()],
                operator: WhereOperator::Eq,
                value:    json!(true),
            },
            WhereClause::Field {
                path:     vec!["age".to_string()],
                operator: WhereOperator::Gte,
                value:    json!(25),
            },
        ]);

        let results = adapter
            .execute_where_query("v_user", Some(&where_clause), None, None)
            .await
            .expect("Failed to execute query");

        for result in &results {
            assert_eq!(result.as_value()["active"], true);
            let age = result.as_value()["age"].as_i64().unwrap();
            assert!(age >= 25);
        }
    }

    #[tokio::test]
    async fn test_where_or() {
        let adapter = create_test_adapter().await;

        let where_clause = WhereClause::Or(vec![
            WhereClause::Field {
                path:     vec!["role".to_string()],
                operator: WhereOperator::Eq,
                value:    json!("admin"),
            },
            WhereClause::Field {
                path:     vec!["role".to_string()],
                operator: WhereOperator::Eq,
                value:    json!("moderator"),
            },
        ]);

        let results = adapter
            .execute_where_query("v_user", Some(&where_clause), None, None)
            .await
            .expect("Failed to execute query");

        assert!(results.len() >= 2);
        for result in &results {
            let role = result.as_value()["role"].as_str().unwrap();
            assert!(role == "admin" || role == "moderator");
        }
    }

    #[tokio::test]
    async fn test_where_not() {
        let adapter = create_test_adapter().await;

        let where_clause = WhereClause::Not(Box::new(WhereClause::Field {
            path:     vec!["active".to_string()],
            operator: WhereOperator::Eq,
            value:    json!(true),
        }));

        let results = adapter
            .execute_where_query("v_user", Some(&where_clause), None, None)
            .await
            .expect("Failed to execute query");

        for result in &results {
            assert_ne!(result.as_value()["active"], json!(true));
        }
    }

    // ========================================================================
    // WHERE Clause Tests - Array Operators
    // ========================================================================

    #[tokio::test]
    async fn test_where_in() {
        let adapter = create_test_adapter().await;

        let where_clause = WhereClause::Field {
            path:     vec!["role".to_string()],
            operator: WhereOperator::In,
            value:    json!(["admin", "moderator"]),
        };

        let results = adapter
            .execute_where_query("v_user", Some(&where_clause), None, None)
            .await
            .expect("Failed to execute query");

        assert!(results.len() >= 2);
        for result in &results {
            let role = result.as_value()["role"].as_str().unwrap();
            assert!(role == "admin" || role == "moderator");
        }
    }

    // ========================================================================
    // Pagination Tests
    // ========================================================================

    #[tokio::test]
    async fn test_limit() {
        let adapter = create_test_adapter().await;

        let results = adapter
            .execute_where_query("v_user", None, Some(2), None)
            .await
            .expect("Failed to execute query");

        assert_eq!(results.len(), 2);
    }

    #[tokio::test]
    async fn test_offset() {
        let adapter = create_test_adapter().await;

        let results_all = adapter
            .execute_where_query("v_user", None, None, None)
            .await
            .expect("Failed to execute query");

        let results_offset = adapter
            .execute_where_query("v_user", None, None, Some(2))
            .await
            .expect("Failed to execute query");

        assert_eq!(results_offset.len(), results_all.len() - 2);
    }

    #[tokio::test]
    async fn test_limit_and_offset() {
        let adapter = create_test_adapter().await;

        let results = adapter
            .execute_where_query("v_user", None, Some(2), Some(1))
            .await
            .expect("Failed to execute query");

        assert_eq!(results.len(), 2);
    }

    // ========================================================================
    // Nested Object Tests
    // ========================================================================

    #[tokio::test]
    async fn test_nested_object_query() {
        let adapter = create_test_adapter().await;

        let where_clause = WhereClause::Field {
            path:     vec!["metadata".to_string(), "city".to_string()],
            operator: WhereOperator::Eq,
            value:    json!("Paris"),
        };

        let results = adapter
            .execute_where_query("v_user", Some(&where_clause), None, None)
            .await
            .expect("Failed to execute query");

        assert!(!results.is_empty());
        for result in &results {
            assert_eq!(result.as_value()["metadata"]["city"], "Paris");
        }
    }

    // ========================================================================
    // Complex Query Tests
    // ========================================================================

    #[tokio::test]
    async fn test_complex_nested_where() {
        let adapter = create_test_adapter().await;

        // (active = true) AND ((role = 'admin') OR (age >= 30))
        let where_clause = WhereClause::And(vec![
            WhereClause::Field {
                path:     vec!["active".to_string()],
                operator: WhereOperator::Eq,
                value:    json!(true),
            },
            WhereClause::Or(vec![
                WhereClause::Field {
                    path:     vec!["role".to_string()],
                    operator: WhereOperator::Eq,
                    value:    json!("admin"),
                },
                WhereClause::Field {
                    path:     vec!["age".to_string()],
                    operator: WhereOperator::Gte,
                    value:    json!(30),
                },
            ]),
        ]);

        let results = adapter
            .execute_where_query("v_user", Some(&where_clause), None, None)
            .await
            .expect("Failed to execute query");

        for result in &results {
            assert_eq!(result.as_value()["active"], true);
            let role = result.as_value()["role"].as_str().unwrap();
            let age = result.as_value()["age"].as_i64().unwrap();
            assert!(role == "admin" || age >= 30);
        }
    }

    // ========================================================================
    // Error Handling Tests
    // ========================================================================

    #[tokio::test]
    async fn test_invalid_view_name() {
        let adapter = create_test_adapter().await;

        let result = adapter.execute_where_query("v_nonexistent", None, None, None).await;

        assert!(result.is_err());
        match result {
            Err(FraiseQLError::Database { .. }) => (),
            _ => panic!("Expected Database error"),
        }
    }

    #[tokio::test]
    async fn test_invalid_connection_string() {
        let result =
            PostgresAdapter::new("postgresql://invalid:invalid@localhost:9999/nonexistent").await;

        assert!(result.is_err());
        match result {
            Err(FraiseQLError::ConnectionPool { .. }) => (),
            _ => panic!("Expected ConnectionPool error"),
        }
    }

    // ========================================================================
    // Parameterized Query Tests (LIMIT/OFFSET with parameters)
    // ========================================================================

    #[tokio::test]
    async fn test_parameterized_limit_only() {
        let adapter = create_test_adapter().await;

        // Test that LIMIT is parameterized (not interpolated)
        let results = adapter
            .execute_where_query("v_user", None, Some(3), None)
            .await
            .expect("Failed to execute query");

        assert_eq!(results.len(), 3, "Should return exactly 3 results with parameterized LIMIT");
    }

    #[tokio::test]
    async fn test_parameterized_offset_only() {
        let adapter = create_test_adapter().await;

        let results_all = adapter
            .execute_where_query("v_user", None, None, None)
            .await
            .expect("Failed to execute query");

        let offset_val = 1;
        let results_offset = adapter
            .execute_where_query("v_user", None, None, Some(offset_val))
            .await
            .expect("Failed to execute query");

        assert_eq!(results_offset.len(), results_all.len() - offset_val as usize);
    }

    #[tokio::test]
    async fn test_parameterized_limit_and_offset() {
        let adapter = create_test_adapter().await;

        // Query with both LIMIT and OFFSET parameterized
        let limit_val = 2;
        let offset_val = 1;
        let results = adapter
            .execute_where_query("v_user", None, Some(limit_val), Some(offset_val))
            .await
            .expect("Failed to execute query");

        assert_eq!(
            results.len(),
            limit_val as usize,
            "Should return exactly {} results",
            limit_val
        );
    }

    #[tokio::test]
    async fn test_parameterized_limit_with_where_clause() {
        let adapter = create_test_adapter().await;

        let where_clause = WhereClause::Field {
            path:     vec!["active".to_string()],
            operator: WhereOperator::Eq,
            value:    json!(true),
        };

        // Parameterized LIMIT with WHERE clause
        let results = adapter
            .execute_where_query("v_user", Some(&where_clause), Some(2), None)
            .await
            .expect("Failed to execute query");

        assert!(results.len() <= 2);
        for result in &results {
            assert_eq!(result.as_value()["active"], true);
        }
    }

    #[tokio::test]
    async fn test_parameterized_limit_and_offset_with_where_clause() {
        let adapter = create_test_adapter().await;

        let where_clause = WhereClause::Field {
            path:     vec!["active".to_string()],
            operator: WhereOperator::Eq,
            value:    json!(true),
        };

        // Parameterized LIMIT and OFFSET with WHERE clause
        let results = adapter
            .execute_where_query("v_user", Some(&where_clause), Some(2), Some(1))
            .await
            .expect("Failed to execute query");

        assert!(results.len() <= 2);
        for result in &results {
            assert_eq!(result.as_value()["active"], true);
        }
    }
}
