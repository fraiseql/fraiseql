//! PostgreSQL database adapter implementation.

use async_trait::async_trait;
use deadpool_postgres::{Config, ManagerConfig, Pool, RecyclingMethod, Runtime};
use tokio_postgres::{NoTls, Row};

use crate::error::{FraiseQLError, Result};
use crate::db::traits::DatabaseAdapter;
use crate::db::types::{DatabaseType, JsonbValue, PoolMetrics};
use crate::db::where_clause::WhereClause;
use super::where_generator::PostgresWhereGenerator;

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

        let pool = cfg
            .create_pool(Some(Runtime::Tokio1), NoTls)
            .map_err(|e| {
                FraiseQLError::ConnectionPool {
                    message: format!("Failed to create connection pool: {e}"),
                }
            })?;

        // Test connection
        let client = pool.get().await.map_err(|e| {
            FraiseQLError::ConnectionPool {
                message: format!("Failed to acquire connection: {e}"),
            }
        })?;

        client.query("SELECT 1", &[]).await.map_err(|e| {
            FraiseQLError::Database {
                message: format!("Failed to connect to database: {e}"),
                sql_state: e.code().map(|c| c.code().to_string()),
            }
        })?;

        Ok(Self { pool })
    }

    /// Execute raw SQL query and return JSONB rows.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Database` on query execution failure.
    async fn execute_raw(&self, sql: &str, params: &[&(dyn tokio_postgres::types::ToSql + Sync)]) -> Result<Vec<JsonbValue>> {
        let client = self.pool.get().await.map_err(|e| {
            FraiseQLError::ConnectionPool {
                message: format!("Failed to acquire connection: {e}"),
            }
        })?;

        let rows: Vec<Row> = client.query(sql, params).await.map_err(|e| {
            FraiseQLError::Database {
                message: format!("Query execution failed: {e}"),
                sql_state: e.code().map(|c| c.code().to_string()),
            }
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
        let mut params: Vec<serde_json::Value> = Vec::new();

        // Add WHERE clause if present
        if let Some(clause) = where_clause {
            let generator = PostgresWhereGenerator::new();
            let (where_sql, where_params) = generator.generate(clause)?;
            sql.push_str(" WHERE ");
            sql.push_str(&where_sql);
            params.extend(where_params);
        }

        // Add LIMIT
        if let Some(lim) = limit {
            sql.push_str(&format!(" LIMIT {lim}"));
        }

        // Add OFFSET
        if let Some(off) = offset {
            sql.push_str(&format!(" OFFSET {off}"));
        }

        // Convert params to references for execute_raw
        // serde_json::Value implements ToSql, so we can pass references directly
        let param_refs: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> = params
            .iter()
            .map(|p| p as &(dyn tokio_postgres::types::ToSql + Sync))
            .collect();

        self.execute_raw(&sql, &param_refs).await
    }

    fn database_type(&self) -> DatabaseType {
        DatabaseType::PostgreSQL
    }

    async fn health_check(&self) -> Result<()> {
        let client = self.pool.get().await.map_err(|e| {
            FraiseQLError::ConnectionPool {
                message: format!("Failed to acquire connection: {e}"),
            }
        })?;

        client.query("SELECT 1", &[]).await.map_err(|e| {
            FraiseQLError::Database {
                message: format!("Health check failed: {e}"),
                sql_state: e.code().map(|c| c.code().to_string()),
            }
        })?;

        Ok(())
    }

    fn pool_metrics(&self) -> PoolMetrics {
        let status = self.pool.status();

        PoolMetrics {
            total_connections: status.size as u32,
            idle_connections: status.available as u32,
            active_connections: (status.size - status.available) as u32,
            waiting_requests: status.waiting as u32,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Note: These tests require a running PostgreSQL instance.
    // They are marked as ignored by default. Run with `cargo test -- --ignored`

    #[tokio::test]
    #[ignore]
    async fn test_postgres_adapter_creation() {
        let adapter = PostgresAdapter::new("postgresql://localhost/test_fraiseql")
            .await
            .expect("Failed to create adapter");

        let metrics = adapter.pool_metrics();
        assert!(metrics.total_connections > 0);
    }

    #[tokio::test]
    #[ignore]
    async fn test_health_check() {
        let adapter = PostgresAdapter::new("postgresql://localhost/test_fraiseql")
            .await
            .expect("Failed to create adapter");

        adapter.health_check().await.expect("Health check failed");
    }

    #[tokio::test]
    #[ignore]
    async fn test_pool_metrics() {
        let adapter = PostgresAdapter::with_pool_size("postgresql://localhost/test_fraiseql", 5)
            .await
            .expect("Failed to create adapter");

        let metrics = adapter.pool_metrics();
        assert_eq!(metrics.total_connections, 5);
        assert!(metrics.idle_connections <= 5);
    }
}
