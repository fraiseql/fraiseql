//! TestDatabase helper for managing test PostgreSQL instances
//!
//! Phase 0.2: Basic infrastructure. Full testcontainers implementation in Phase 1.

use std::sync::Arc;

/// Manages a test PostgreSQL database instance
#[derive(Clone)]
pub struct TestDatabase {
    _inner: Arc<TestDatabaseInner>,
}

struct TestDatabaseInner {
    // Placeholder for future container management
}

impl TestDatabase {
    /// Create a new test database with default settings
    pub async fn new() -> Result<Self, Box<dyn std::error::Error>> {
        // Phase 0.2: Return a mock instance
        // Full implementation with testcontainers in Phase 1
        Ok(TestDatabase {
            _inner: Arc::new(TestDatabaseInner {}),
        })
    }

    /// Create a test database with custom configuration
    pub async fn with_config(
        _config: TestDatabaseConfig,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        // Phase 0.2: Return a mock instance
        Self::new().await
    }

    /// Get connection string for this test database
    pub fn connection_string(&self) -> String {
        // Phase 0.2: Return mock connection string
        // Real implementation will use actual container port
        "postgresql://postgres:postgres@localhost:5432/test_db".to_string()
    }

    /// Get a PostgreSQL client for queries (placeholder)
    pub async fn client(&self) -> Result<tokio_postgres::Client, Box<dyn std::error::Error>> {
        // Phase 0.2: This will panic - real implementation in Phase 1
        // For now, tests can use the connection_string() method
        Err("TestDatabase client() not implemented in Phase 0.2".into())
    }

    /// Execute a query and return results (placeholder)
    pub async fn query(
        &self,
        _sql: &str,
        _params: &[&(dyn tokio_postgres::types::ToSql + Sync)],
    ) -> Result<Vec<tokio_postgres::Row>, Box<dyn std::error::Error>> {
        Err("TestDatabase query() not implemented in Phase 0.2".into())
    }

    /// Execute a statement without returning rows (placeholder)
    pub async fn execute(
        &self,
        _sql: &str,
        _params: &[&(dyn tokio_postgres::types::ToSql + Sync)],
    ) -> Result<u64, Box<dyn std::error::Error>> {
        Err("TestDatabase execute() not implemented in Phase 0.2".into())
    }

    /// Run migrations on test database (placeholder)
    pub async fn migrate(&self, _migrations: &[&str]) -> Result<(), Box<dyn std::error::Error>> {
        Err("TestDatabase migrate() not implemented in Phase 0.2".into())
    }
}

/// Configuration for test database
#[derive(Clone, Debug)]
pub struct TestDatabaseConfig {
    pub db_name: String,
    pub user: String,
    pub password: String,
}

impl Default for TestDatabaseConfig {
    fn default() -> Self {
        TestDatabaseConfig {
            db_name: "test_db".to_string(),
            user: "postgres".to_string(),
            password: "postgres".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_database_connection() {
        // Phase 0.2: Basic API test - full functionality in Phase 1
        let db = TestDatabase::new().await.expect("Failed to create test database");
        assert!(!db.connection_string().is_empty());
    }

    #[tokio::test]
    async fn test_database_query() {
        // Phase 0.2: API placeholder - real tests in Phase 1
        let db = TestDatabase::new().await.expect("Failed to create test database");
        let result = db.query("SELECT 1", &[]).await;
        assert!(result.is_err()); // Should fail in Phase 0.2
    }
}
