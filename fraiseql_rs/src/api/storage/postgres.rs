//! PostgreSQL storage backend implementation
//!
//! Provides StorageBackend implementation for PostgreSQL database.
//! Uses a pluggable PoolBackend abstraction for connection pooling.
//! FraiseQL is PostgreSQL-only to leverage native JSONB support.
//!
//! # Architecture
//! - Depends on PoolBackend trait (abstraction)
//! - Not tied to specific pool implementation (deadpool, sqlx, etc.)
//! - Designed for dependency injection (pool provided at initialization)
//!
//! Phase 3: Real database backend with connection pooling

use super::traits::{ExecuteResult, QueryResult, StorageBackend, Transaction};
use super::StorageError;
use crate::db::pool::PoolBackend;
use std::sync::Arc;

/// PostgreSQL storage backend using pluggable pool abstraction
pub struct PostgresBackend {
    /// Pluggable connection pool (abstraction)
    pool: Arc<dyn PoolBackend>,
}

impl PostgresBackend {
    /// Create a new PostgreSQL backend with an existing pool.
    ///
    /// # Arguments
    /// * `pool` - A pluggable PoolBackend implementation (e.g., ProductionPool)
    ///
    /// # Returns
    /// * `Ok(PostgresBackend)` - Backend ready to use
    /// * `Err(StorageError)` - If pool is invalid
    ///
    /// # Example
    /// ```ignore
    /// use fraiseql_rs::db::ProductionPool;
    /// use fraiseql_rs::api::storage::PostgresBackend;
    ///
    /// let pool = ProductionPool::new(config)?;
    /// let pool_arc: Arc<dyn PoolBackend> = Arc::new(pool);
    /// let backend = PostgresBackend::with_pool(pool_arc)?;
    /// ```
    pub fn with_pool(pool: Arc<dyn PoolBackend>) -> Result<Self, StorageError> {
        Ok(PostgresBackend { pool })
    }
}

#[async_trait::async_trait]
impl StorageBackend for PostgresBackend {
    async fn query(
        &self,
        sql: &str,
        _params: &[serde_json::Value],
    ) -> Result<QueryResult, StorageError> {
        // Execute SELECT query using pool abstraction
        // FraiseQL pattern: JSONB data already in column 0 (no conversion needed)
        let start = std::time::Instant::now();

        let rows =
            self.pool.query(sql).await.map_err(|e| {
                StorageError::DatabaseError(format!("Query execution failed: {}", e))
            })?;

        let execution_time_ms = start.elapsed().as_millis() as u64;
        let row_count = rows.len();

        Ok(QueryResult {
            rows,
            row_count,
            execution_time_ms,
        })
    }

    async fn execute(
        &self,
        sql: &str,
        _params: &[serde_json::Value],
    ) -> Result<ExecuteResult, StorageError> {
        // Execute INSERT/UPDATE/DELETE statement using pool abstraction
        let start = std::time::Instant::now();

        let rows_affected = self.pool.execute(sql).await.map_err(|e| {
            StorageError::DatabaseError(format!("Statement execution failed: {}", e))
        })?;

        let execution_time_ms = start.elapsed().as_millis() as u64;

        Ok(ExecuteResult {
            rows_affected,
            last_insert_id: None, // Phase 3.2: Extract from result
            execution_time_ms,
        })
    }

    async fn begin_transaction(&self) -> Result<Box<dyn Transaction>, StorageError> {
        // Phase 3.2+: Implement transaction support
        Err(StorageError::TransactionError(
            "Transactions not yet implemented".to_string(),
        ))
    }

    async fn health_check(&self) -> Result<(), StorageError> {
        // Execute simple health check query using pool abstraction
        self.pool
            .query("SELECT 1")
            .await
            .map_err(|e| StorageError::ConnectionError(format!("Health check failed: {}", e)))?;

        Ok(())
    }

    fn backend_name(&self) -> &str {
        self.pool.backend_name()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // Mock PoolBackend for testing
    struct MockPool;

    #[async_trait::async_trait]
    impl PoolBackend for MockPool {
        async fn query(
            &self,
            _sql: &str,
        ) -> Result<Vec<serde_json::Value>, crate::db::pool::PoolError> {
            Ok(vec![json!({"id": 1, "name": "test"})])
        }

        async fn execute(&self, _sql: &str) -> Result<u64, crate::db::pool::PoolError> {
            Ok(1)
        }

        fn pool_info(&self) -> serde_json::Value {
            json!({"backend": "mock"})
        }

        fn backend_name(&self) -> &str {
            "mock"
        }
    }

    #[test]
    fn test_postgres_backend_creation() {
        let pool: Arc<dyn PoolBackend> = Arc::new(MockPool);
        let backend = PostgresBackend::with_pool(pool);
        assert!(backend.is_ok());
    }

    #[tokio::test]
    async fn test_postgres_backend_name() {
        let pool: Arc<dyn PoolBackend> = Arc::new(MockPool);
        let backend = PostgresBackend::with_pool(pool).expect("Failed to create backend");
        assert_eq!(backend.backend_name(), "mock");
    }

    #[tokio::test]
    async fn test_postgres_query() {
        let pool: Arc<dyn PoolBackend> = Arc::new(MockPool);
        let backend = PostgresBackend::with_pool(pool).expect("Failed to create backend");

        let result = backend.query("SELECT * FROM test", &[]).await;
        assert!(result.is_ok());

        let query_result = result.unwrap();
        assert_eq!(query_result.row_count, 1);
    }

    #[tokio::test]
    async fn test_postgres_execute() {
        let pool: Arc<dyn PoolBackend> = Arc::new(MockPool);
        let backend = PostgresBackend::with_pool(pool).expect("Failed to create backend");

        let result = backend.execute("INSERT INTO test VALUES (1)", &[]).await;
        assert!(result.is_ok());

        let exec_result = result.unwrap();
        assert_eq!(exec_result.rows_affected, 1);
    }
}
