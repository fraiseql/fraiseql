//! PostgreSQL backend for APQ
//!
//! Provides persistent, distributed APQ storage using PostgreSQL.
//! Suitable for multi-instance deployments or when persistence is required.

use async_trait::async_trait;
use deadpool_postgres::Pool;
use std::sync::Arc;

use crate::apq::storage::{ApqError, ApqStats, ApqStorage};

/// PostgreSQL APQ storage backend
///
/// Provides persistent query storage with multi-instance support.
/// Requires a PostgreSQL database with the APQ table initialized.
#[derive(Debug)]
pub struct PostgresApqStorage {
    /// Database connection pool
    pool: Arc<Pool>,

    /// Table name for APQ queries
    table_name: String,
}

impl PostgresApqStorage {
    /// Create new PostgreSQL storage
    ///
    /// # Arguments
    ///
    /// * `pool` - Database connection pool
    /// * `table_name` - Optional table name (default: "fraiseql_apq_queries")
    ///
    /// # Example
    ///
    /// ```ignore
    /// let pool = Arc::new(create_pool().await?);
    /// let storage = PostgresApqStorage::new(pool, None);
    /// ```
    #[must_use]
    pub fn new(pool: Arc<Pool>, table_name: Option<String>) -> Self {
        Self {
            pool,
            table_name: table_name.unwrap_or_else(|| "fraiseql_apq_queries".to_string()),
        }
    }

    /// Initialize the database table
    ///
    /// Creates the APQ table if it doesn't exist.
    /// Must be called once before using the storage.
    ///
    /// # Errors
    ///
    /// Returns error if database initialization fails
    pub async fn init(&self) -> Result<(), ApqError> {
        let client = self
            .pool
            .get()
            .await
            .map_err(|e| ApqError::DatabaseError(e.to_string()))?;

        let sql = format!(
            "CREATE TABLE IF NOT EXISTS {} (
                hash TEXT PRIMARY KEY,
                query TEXT NOT NULL,
                created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                last_accessed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                access_count BIGINT NOT NULL DEFAULT 1
            )",
            self.table_name
        );

        client
            .execute(&sql, &[])
            .await
            .map_err(|e| ApqError::DatabaseError(e.to_string()))?;

        // Create index on last_accessed_at for cleanup queries
        let index_sql = format!(
            "CREATE INDEX IF NOT EXISTS idx_{}_last_accessed
             ON {}(last_accessed_at)",
            self.table_name, self.table_name
        );

        client
            .execute(&index_sql, &[])
            .await
            .map_err(|e| ApqError::DatabaseError(e.to_string()))?;

        Ok(())
    }
}

#[async_trait]
impl ApqStorage for PostgresApqStorage {
    async fn get(&self, hash: &str) -> Result<Option<String>, ApqError> {
        let client = self
            .pool
            .get()
            .await
            .map_err(|e| ApqError::DatabaseError(e.to_string()))?;

        let sql = format!(
            "UPDATE {}
             SET last_accessed_at = NOW(), access_count = access_count + 1
             WHERE hash = $1
             RETURNING query",
            self.table_name
        );

        let row = client
            .query_opt(&sql, &[&hash])
            .await
            .map_err(|e| ApqError::DatabaseError(e.to_string()))?;

        Ok(row.map(|r| r.get(0)))
    }

    async fn set(&self, hash: String, query: String) -> Result<(), ApqError> {
        let client = self
            .pool
            .get()
            .await
            .map_err(|e| ApqError::DatabaseError(e.to_string()))?;

        let sql = format!(
            "INSERT INTO {} (hash, query)
             VALUES ($1, $2)
             ON CONFLICT (hash) DO UPDATE
             SET last_accessed_at = NOW(), access_count = {}.access_count + 1",
            self.table_name, self.table_name
        );

        client
            .execute(&sql, &[&hash, &query])
            .await
            .map_err(|e| ApqError::DatabaseError(e.to_string()))?;

        Ok(())
    }

    async fn exists(&self, hash: &str) -> Result<bool, ApqError> {
        let client = self
            .pool
            .get()
            .await
            .map_err(|e| ApqError::DatabaseError(e.to_string()))?;

        let sql = format!("SELECT 1 FROM {} WHERE hash = $1 LIMIT 1", self.table_name);

        let row = client
            .query_opt(&sql, &[&hash])
            .await
            .map_err(|e| ApqError::DatabaseError(e.to_string()))?;

        Ok(row.is_some())
    }

    async fn remove(&self, hash: &str) -> Result<(), ApqError> {
        let client = self
            .pool
            .get()
            .await
            .map_err(|e| ApqError::DatabaseError(e.to_string()))?;

        let sql = format!("DELETE FROM {} WHERE hash = $1", self.table_name);

        client
            .execute(&sql, &[&hash])
            .await
            .map_err(|e| ApqError::DatabaseError(e.to_string()))?;

        Ok(())
    }

    async fn stats(&self) -> Result<ApqStats, ApqError> {
        let client = self
            .pool
            .get()
            .await
            .map_err(|e| ApqError::DatabaseError(e.to_string()))?;

        let sql = format!(
            "SELECT
                COUNT(*) as total,
                SUM(access_count) as total_accesses,
                AVG(access_count) as avg_accesses
             FROM {}",
            self.table_name
        );

        let row = client
            .query_one(&sql, &[])
            .await
            .map_err(|e| ApqError::DatabaseError(e.to_string()))?;

        let total: i64 = row.get(0);
        let total_accesses: Option<i64> = row.get(1);
        let avg_accesses: Option<f64> = row.get(2);

        Ok(ApqStats::with_extra(
            total as usize,
            "postgresql".to_string(),
            serde_json::json!({
                "total_accesses": total_accesses.unwrap_or(0),
                "avg_accesses": avg_accesses.unwrap_or(0.0),
            }),
        ))
    }

    async fn clear(&self) -> Result<(), ApqError> {
        let client = self
            .pool
            .get()
            .await
            .map_err(|e| ApqError::DatabaseError(e.to_string()))?;

        let sql = format!("TRUNCATE TABLE {}", self.table_name);

        client
            .execute(&sql, &[])
            .await
            .map_err(|e| ApqError::DatabaseError(e.to_string()))?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_postgres_storage_creation() {
        // This is a unit test that doesn't need a real database
        // The actual integration tests would be in tests/ directory
        // Just verify the struct can be created
        let table_name = Some("test_apq".to_string());
        // We can't actually create a PostgresApqStorage without a real pool
        // This is just to verify the API
        let _ = table_name;
    }
}
