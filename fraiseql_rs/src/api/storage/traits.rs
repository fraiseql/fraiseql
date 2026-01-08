//! Storage backend traits and interfaces
//!
//! Defines the contract that all storage backends must implement.

use super::StorageError;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Result of executing a database query
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryResult {
    /// Rows returned by the query (each row is a JSON object)
    pub rows: Vec<serde_json::Value>,

    /// Total number of rows returned
    pub row_count: usize,

    /// Query execution time in milliseconds
    pub execution_time_ms: u64,
}

/// Result of executing an INSERT/UPDATE/DELETE statement
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecuteResult {
    /// Number of rows affected
    pub rows_affected: u64,

    /// Last inserted row ID (for INSERT operations)
    pub last_insert_id: Option<u64>,

    /// Query execution time in milliseconds
    pub execution_time_ms: u64,
}

/// Represents a database transaction
#[async_trait]
pub trait Transaction: Send + Sync {
    /// Execute a SELECT query within the transaction
    ///
    /// # Arguments
    /// * `sql` - SQL query with placeholders
    /// * `params` - Query parameters to bind
    ///
    /// # Returns
    /// * `Ok(QueryResult)` - Query results
    /// * `Err(StorageError)` - If query fails
    async fn query(
        &self,
        sql: &str,
        params: &[serde_json::Value],
    ) -> Result<QueryResult, StorageError>;

    /// Execute an INSERT/UPDATE/DELETE within the transaction
    ///
    /// # Arguments
    /// * `sql` - SQL statement with placeholders
    /// * `params` - Parameters to bind
    ///
    /// # Returns
    /// * `Ok(ExecuteResult)` - Rows affected
    /// * `Err(StorageError)` - If execution fails
    async fn execute(
        &self,
        sql: &str,
        params: &[serde_json::Value],
    ) -> Result<ExecuteResult, StorageError>;

    /// Commit the transaction
    ///
    /// # Returns
    /// * `Ok(())` - Transaction committed
    /// * `Err(StorageError)` - If commit fails
    async fn commit(self) -> Result<(), StorageError>;

    /// Rollback the transaction
    ///
    /// # Returns
    /// * `Ok(())` - Transaction rolled back
    /// * `Err(StorageError)` - If rollback fails
    async fn rollback(self) -> Result<(), StorageError>;
}

/// Main storage backend trait - all databases implement this
#[async_trait]
pub trait StorageBackend: Send + Sync {
    /// Execute a SELECT query against the database
    ///
    /// # Arguments
    /// * `sql` - SQL query with ? placeholders
    /// * `params` - Query parameters in order
    ///
    /// # Returns
    /// * `Ok(QueryResult)` - Rows from the database
    /// * `Err(StorageError)` - If query fails
    ///
    /// # Example
    /// ```rust,ignore
    /// let result = backend.query(
    ///     "SELECT id, name FROM users WHERE id = ?",
    ///     &[serde_json::json!("123")],
    /// ).await?;
    /// ```
    async fn query(
        &self,
        sql: &str,
        params: &[serde_json::Value],
    ) -> Result<QueryResult, StorageError>;

    /// Execute an INSERT/UPDATE/DELETE statement
    ///
    /// # Arguments
    /// * `sql` - SQL statement with ? placeholders
    /// * `params` - Parameters in order
    ///
    /// # Returns
    /// * `Ok(ExecuteResult)` - Rows affected and metadata
    /// * `Err(StorageError)` - If execution fails
    async fn execute(
        &self,
        sql: &str,
        params: &[serde_json::Value],
    ) -> Result<ExecuteResult, StorageError>;

    /// Begin a new transaction
    ///
    /// # Returns
    /// * `Ok(Transaction)` - Active transaction handle
    /// * `Err(StorageError)` - If transaction start fails
    ///
    /// # Note
    /// Transaction must be committed or rolled back.
    /// Dropping without explicit commit/rollback will rollback.
    async fn begin_transaction(&self) -> Result<Box<dyn Transaction>, StorageError>;

    /// Check if the storage backend is healthy and connected
    ///
    /// # Returns
    /// * `Ok(())` - Backend is healthy
    /// * `Err(StorageError)` - If health check fails
    async fn health_check(&self) -> Result<(), StorageError>;

    /// Get backend name (for logging/debugging)
    fn backend_name(&self) -> &str;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_query_result_serialization() {
        let result = QueryResult {
            rows: vec![serde_json::json!({"id": 1, "name": "test"})],
            row_count: 1,
            execution_time_ms: 10,
        };

        let json = serde_json::to_string(&result).unwrap();
        let deserialized: QueryResult = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.row_count, 1);
        assert_eq!(deserialized.execution_time_ms, 10);
    }

    #[test]
    fn test_execute_result_serialization() {
        let result = ExecuteResult {
            rows_affected: 5,
            last_insert_id: Some(42),
            execution_time_ms: 5,
        };

        let json = serde_json::to_string(&result).unwrap();
        let deserialized: ExecuteResult = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.rows_affected, 5);
        assert_eq!(deserialized.last_insert_id, Some(42));
    }

    #[test]
    fn test_storage_error_display() {
        let err = StorageError::ConnectionError("Connection refused".to_string());
        assert!(err.to_string().contains("Connection error"));

        let err = StorageError::TimeoutError;
        assert!(err.to_string().contains("timeout"));

        let err = StorageError::NotFound;
        assert!(err.to_string().contains("not found"));
    }
}
