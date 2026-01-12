///! PostgreSQL database introspection for fact tables.
use async_trait::async_trait;
use deadpool_postgres::Pool;
use tokio_postgres::Row;

use crate::compiler::fact_table::{DatabaseIntrospector, DatabaseType};
use crate::error::{FraiseQLError, Result};

/// PostgreSQL introspector for fact table metadata.
pub struct PostgresIntrospector {
    pool: Pool,
}

impl PostgresIntrospector {
    /// Create new PostgreSQL introspector from connection pool.
    #[must_use]
    pub const fn new(pool: Pool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl DatabaseIntrospector for PostgresIntrospector {
    async fn get_columns(&self, table_name: &str) -> Result<Vec<(String, String, bool)>> {
        let client = self.pool.get().await.map_err(|e| {
            FraiseQLError::ConnectionPool {
                message: format!("Failed to acquire connection: {e}"),
            }
        })?;

        // Query information_schema for column information
        let query = r"
            SELECT
                column_name,
                data_type,
                is_nullable = 'YES' as is_nullable
            FROM information_schema.columns
            WHERE table_name = $1
            AND table_schema = 'public'
            ORDER BY ordinal_position
        ";

        let rows: Vec<Row> = client.query(query, &[&table_name]).await.map_err(|e| {
            FraiseQLError::Database {
                message: format!("Failed to query column information: {e}"),
                sql_state: e.code().map(|c| c.code().to_string()),
            }
        })?;

        let columns = rows
            .into_iter()
            .map(|row| {
                let name: String = row.get(0);
                let data_type: String = row.get(1);
                let is_nullable: bool = row.get(2);
                (name, data_type, is_nullable)
            })
            .collect();

        Ok(columns)
    }

    async fn get_indexed_columns(&self, table_name: &str) -> Result<Vec<String>> {
        let client = self.pool.get().await.map_err(|e| {
            FraiseQLError::ConnectionPool {
                message: format!("Failed to acquire connection: {e}"),
            }
        })?;

        // Query pg_indexes for indexed columns
        let query = r"
            SELECT DISTINCT
                a.attname as column_name
            FROM
                pg_index i
                JOIN pg_attribute a ON a.attrelid = i.indrelid AND a.attnum = ANY(i.indkey)
                JOIN pg_class t ON t.oid = i.indrelid
                JOIN pg_namespace n ON n.oid = t.relnamespace
            WHERE
                t.relname = $1
                AND n.nspname = 'public'
                AND a.attnum > 0
            ORDER BY column_name
        ";

        let rows: Vec<Row> = client.query(query, &[&table_name]).await.map_err(|e| {
            FraiseQLError::Database {
                message: format!("Failed to query index information: {e}"),
                sql_state: e.code().map(|c| c.code().to_string()),
            }
        })?;

        let indexed_columns = rows
            .into_iter()
            .map(|row| {
                let name: String = row.get(0);
                name
            })
            .collect();

        Ok(indexed_columns)
    }

    fn database_type(&self) -> DatabaseType {
        DatabaseType::PostgreSQL
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::postgres::PostgresAdapter;

    const TEST_DB_URL: &str = "postgresql://fraiseql_test:fraiseql_test_password@localhost:5433/test_fraiseql";

    // Helper to create test introspector
    async fn create_test_introspector() -> PostgresIntrospector {
        let adapter = PostgresAdapter::new(TEST_DB_URL)
            .await
            .expect("Failed to create test adapter");

        // Extract pool from adapter (we need a way to get the pool)
        // For now, create a new pool directly
        use deadpool_postgres::{Config, ManagerConfig, RecyclingMethod, Runtime};
        use tokio_postgres::NoTls;

        let mut cfg = Config::new();
        cfg.url = Some(TEST_DB_URL.to_string());
        cfg.manager = Some(ManagerConfig {
            recycling_method: RecyclingMethod::Fast,
        });
        cfg.pool = Some(deadpool_postgres::PoolConfig::new(10));

        let pool = cfg
            .create_pool(Some(Runtime::Tokio1), NoTls)
            .expect("Failed to create pool");

        PostgresIntrospector::new(pool)
    }

    #[tokio::test]
    #[ignore]
    async fn test_get_columns_tf_sales() {
        let introspector = create_test_introspector().await;

        let columns = introspector
            .get_columns("tf_sales")
            .await
            .expect("Failed to get columns");

        // Should have: id, revenue, quantity, cost, discount, data, customer_id, product_id, occurred_at, created_at
        assert!(columns.len() >= 10, "Expected at least 10 columns, got {}", columns.len());

        // Check for key columns
        let column_names: Vec<String> = columns.iter().map(|(name, _, _)| name.clone()).collect();
        assert!(column_names.contains(&"revenue".to_string()));
        assert!(column_names.contains(&"quantity".to_string()));
        assert!(column_names.contains(&"data".to_string()));
        assert!(column_names.contains(&"customer_id".to_string()));
    }

    #[tokio::test]
    #[ignore]
    async fn test_get_indexed_columns_tf_sales() {
        let introspector = create_test_introspector().await;

        let indexed = introspector
            .get_indexed_columns("tf_sales")
            .await
            .expect("Failed to get indexed columns");

        // Should have indexes on: id (PK), customer_id, product_id, occurred_at, data (GIN)
        assert!(indexed.len() >= 4, "Expected at least 4 indexed columns, got {}", indexed.len());

        assert!(indexed.contains(&"customer_id".to_string()));
        assert!(indexed.contains(&"product_id".to_string()));
        assert!(indexed.contains(&"occurred_at".to_string()));
    }

    #[tokio::test]
    #[ignore]
    async fn test_database_type() {
        let introspector = create_test_introspector().await;
        assert_eq!(introspector.database_type(), DatabaseType::PostgreSQL);
    }
}
