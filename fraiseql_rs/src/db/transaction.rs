//! Transaction management for production pool.
//!
//! Provides ACID transaction support with savepoints and isolation levels.

use crate::db::{
    errors::{DatabaseError, DatabaseResult},
    pool_production::ProductionPool,
};
use tokio_postgres::IsolationLevel;

/// Transaction wrapper with ACID guarantees.
///
/// Supports:
/// - Commit/rollback operations
/// - Savepoints for nested transactions
/// - Isolation level control
/// - Query execution within transaction context
///
/// Note: This is a simplified implementation that uses SQL BEGIN/COMMIT
/// for compatibility with the pool architecture.
#[derive(Debug, Clone)]
pub struct Transaction {
    /// Connection pool reference
    pool: ProductionPool,
    /// Whether transaction is active
    active: bool,
    /// Savepoint stack for nested transactions
    savepoints: Vec<String>,
}

impl Transaction {
    /// Create a new transaction from a pool.
    ///
    /// Uses the default isolation level (READ COMMITTED).
    ///
    /// # Errors
    ///
    /// Returns `DatabaseError::QueryExecution` if transaction begin fails.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use fraiseql_rs::db::{DatabaseConfig, ProductionPool, Transaction};
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let config = DatabaseConfig::new("mydb");
    /// let pool = ProductionPool::new(config)?;
    ///
    /// let mut tx = Transaction::begin(&pool).await?;
    /// // Use transaction...
    /// tx.commit().await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn begin(pool: &ProductionPool) -> DatabaseResult<Self> {
        // Execute BEGIN
        pool.execute_query("BEGIN").await?;

        Ok(Self {
            pool: pool.clone(),
            active: true,
            savepoints: Vec::new(),
        })
    }

    /// Create transaction with specific isolation level.
    ///
    /// # Errors
    ///
    /// Returns `DatabaseError::QueryExecution` if transaction begin fails.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use fraiseql_rs::db::{DatabaseConfig, ProductionPool, Transaction};
    /// use tokio_postgres::IsolationLevel;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// # let pool = ProductionPool::new(DatabaseConfig::new("mydb"))?;
    /// let mut tx = Transaction::begin_with_isolation(
    ///     &pool,
    ///     IsolationLevel::Serializable
    /// ).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn begin_with_isolation(
        pool: &ProductionPool,
        isolation: IsolationLevel,
    ) -> DatabaseResult<Self> {
        // Build BEGIN statement with isolation level
        let sql = match isolation {
            IsolationLevel::ReadUncommitted => "BEGIN ISOLATION LEVEL READ UNCOMMITTED",
            IsolationLevel::ReadCommitted => "BEGIN ISOLATION LEVEL READ COMMITTED",
            IsolationLevel::RepeatableRead => "BEGIN ISOLATION LEVEL REPEATABLE READ",
            IsolationLevel::Serializable => "BEGIN ISOLATION LEVEL SERIALIZABLE",
            _ => "BEGIN",
        };

        pool.execute_query(sql).await?;

        Ok(Self {
            pool: pool.clone(),
            active: true,
            savepoints: Vec::new(),
        })
    }

    /// Execute a query within the transaction.
    ///
    /// # Errors
    ///
    /// Returns `DatabaseError::QueryExecution` if query fails.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use fraiseql_rs::db::{DatabaseConfig, ProductionPool, Transaction};
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// # let pool = ProductionPool::new(DatabaseConfig::new("mydb"))?;
    /// let mut tx = Transaction::begin(&pool).await?;
    /// let results = tx.query("SELECT 1 as num").await?;
    /// tx.commit().await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn query(&self, sql: &str) -> DatabaseResult<Vec<serde_json::Value>> {
        if !self.active {
            return Err(DatabaseError::QueryExecution(
                "Transaction is not active".to_string(),
            ));
        }
        self.pool.execute_query(sql).await
    }

    /// Execute a statement (INSERT, UPDATE, DELETE) within the transaction.
    ///
    /// Returns the result from the query.
    ///
    /// # Errors
    ///
    /// Returns `DatabaseError::QueryExecution` if execution fails.
    pub async fn execute(&self, sql: &str) -> DatabaseResult<Vec<serde_json::Value>> {
        if !self.active {
            return Err(DatabaseError::QueryExecution(
                "Transaction is not active".to_string(),
            ));
        }
        self.pool.execute_query(sql).await
    }

    /// Create a savepoint (nested transaction).
    ///
    /// # Errors
    ///
    /// Returns `DatabaseError::QueryExecution` if savepoint creation fails.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use fraiseql_rs::db::{DatabaseConfig, ProductionPool, Transaction};
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// # let pool = ProductionPool::new(DatabaseConfig::new("mydb"))?;
    /// let mut tx = Transaction::begin(&pool).await?;
    ///
    /// tx.savepoint("sp1").await?;
    /// // ... operations ...
    /// tx.rollback_to("sp1").await?; // Rollback to savepoint
    ///
    /// tx.commit().await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn savepoint(&mut self, name: &str) -> DatabaseResult<()> {
        if !self.active {
            return Err(DatabaseError::QueryExecution(
                "Transaction is not active".to_string(),
            ));
        }

        let sql = format!("SAVEPOINT {name}");
        self.pool.execute_query(&sql).await?;
        self.savepoints.push(name.to_string());
        Ok(())
    }

    /// Rollback to a savepoint.
    ///
    /// # Errors
    ///
    /// Returns `DatabaseError::QueryExecution` if rollback fails.
    pub async fn rollback_to(&mut self, name: &str) -> DatabaseResult<()> {
        if !self.active {
            return Err(DatabaseError::QueryExecution(
                "Transaction is not active".to_string(),
            ));
        }

        let sql = format!("ROLLBACK TO SAVEPOINT {name}");
        self.pool.execute_query(&sql).await?;

        // Remove savepoints after the rollback point
        if let Some(pos) = self.savepoints.iter().position(|s| s == name) {
            self.savepoints.truncate(pos);
        }

        Ok(())
    }

    /// Release a savepoint (mark it as successfully completed).
    ///
    /// # Errors
    ///
    /// Returns `DatabaseError::QueryExecution` if release fails.
    pub async fn release_savepoint(&mut self, name: &str) -> DatabaseResult<()> {
        if !self.active {
            return Err(DatabaseError::QueryExecution(
                "Transaction is not active".to_string(),
            ));
        }

        let sql = format!("RELEASE SAVEPOINT {name}");
        self.pool.execute_query(&sql).await?;

        // Remove the savepoint from stack
        self.savepoints.retain(|s| s != name);

        Ok(())
    }

    /// Commit the transaction.
    ///
    /// # Errors
    ///
    /// Returns `DatabaseError::QueryExecution` if commit fails.
    pub async fn commit(mut self) -> DatabaseResult<()> {
        if !self.active {
            return Err(DatabaseError::QueryExecution(
                "Transaction is not active".to_string(),
            ));
        }

        self.pool.execute_query("COMMIT").await?;
        self.active = false;
        Ok(())
    }

    /// Rollback the transaction.
    ///
    /// # Errors
    ///
    /// Returns `DatabaseError::QueryExecution` if rollback fails.
    pub async fn rollback(mut self) -> DatabaseResult<()> {
        if !self.active {
            return Ok(()); // Already rolled back
        }

        self.pool.execute_query("ROLLBACK").await?;
        self.active = false;
        Ok(())
    }

    /// Get the number of active savepoints.
    #[must_use]
    pub const fn savepoint_count(&self) -> usize {
        self.savepoints.len()
    }

    /// Check if transaction is active.
    #[must_use]
    pub const fn is_active(&self) -> bool {
        self.active
    }
}

impl Drop for Transaction {
    fn drop(&mut self) {
        // Note: We can't run async code in Drop, so we just mark as inactive
        // In production, users should always explicitly commit or rollback
        if self.active {
            eprintln!("Warning: Transaction dropped without explicit commit or rollback");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{DatabaseConfig, SslMode};

    #[tokio::test]
    #[ignore = "Requires PostgreSQL database connection"]
    async fn test_transaction_commit() {
        let config = DatabaseConfig::new("postgres").with_ssl_mode(SslMode::Disable);
        let pool = ProductionPool::new(config).unwrap();

        let tx = Transaction::begin(&pool).await.unwrap();
        assert!(tx.is_active());

        let result = tx.query("SELECT 1 as num").await.unwrap();
        assert_eq!(result.len(), 1);

        tx.commit().await.unwrap();
    }

    #[tokio::test]
    #[ignore = "Requires PostgreSQL database connection"]
    async fn test_transaction_rollback() {
        let config = DatabaseConfig::new("postgres").with_ssl_mode(SslMode::Disable);
        let pool = ProductionPool::new(config).unwrap();

        let tx = Transaction::begin(&pool).await.unwrap();
        let _ = tx.query("SELECT 1").await;

        tx.rollback().await.unwrap();
    }

    #[tokio::test]
    #[ignore = "Requires PostgreSQL database connection"]
    async fn test_savepoint() {
        let config = DatabaseConfig::new("postgres").with_ssl_mode(SslMode::Disable);
        let pool = ProductionPool::new(config).unwrap();

        let mut tx = Transaction::begin(&pool).await.unwrap();

        tx.savepoint("sp1").await.unwrap();
        assert_eq!(tx.savepoint_count(), 1);

        tx.rollback_to("sp1").await.unwrap();
        assert_eq!(tx.savepoint_count(), 0);

        tx.commit().await.unwrap();
    }

    #[tokio::test]
    #[ignore = "Requires PostgreSQL database connection"]
    async fn test_nested_savepoints() {
        let config = DatabaseConfig::new("postgres").with_ssl_mode(SslMode::Disable);
        let pool = ProductionPool::new(config).unwrap();

        let mut tx = Transaction::begin(&pool).await.unwrap();

        tx.savepoint("sp1").await.unwrap();
        tx.savepoint("sp2").await.unwrap();
        tx.savepoint("sp3").await.unwrap();
        assert_eq!(tx.savepoint_count(), 3);

        tx.rollback_to("sp2").await.unwrap();
        assert_eq!(tx.savepoint_count(), 1); // sp1 remains

        tx.commit().await.unwrap();
    }

    #[tokio::test]
    #[ignore = "Requires PostgreSQL database connection"]
    async fn test_isolation_level() {
        let config = DatabaseConfig::new("postgres").with_ssl_mode(SslMode::Disable);
        let pool = ProductionPool::new(config).unwrap();

        let tx = Transaction::begin_with_isolation(&pool, IsolationLevel::Serializable)
            .await
            .unwrap();

        assert!(tx.is_active());
        tx.commit().await.unwrap();
    }

    #[tokio::test]
    #[ignore = "Requires PostgreSQL database connection"]
    async fn test_release_savepoint() {
        let config = DatabaseConfig::new("postgres").with_ssl_mode(SslMode::Disable);
        let pool = ProductionPool::new(config).unwrap();

        let mut tx = Transaction::begin(&pool).await.unwrap();

        tx.savepoint("sp1").await.unwrap();
        tx.savepoint("sp2").await.unwrap();
        assert_eq!(tx.savepoint_count(), 2);

        tx.release_savepoint("sp1").await.unwrap();
        assert_eq!(tx.savepoint_count(), 1); // sp2 remains

        tx.commit().await.unwrap();
    }
}
