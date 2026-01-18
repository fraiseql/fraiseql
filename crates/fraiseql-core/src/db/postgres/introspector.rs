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
    async fn list_fact_tables(&self) -> Result<Vec<String>> {
        let client = self.pool.get().await.map_err(|e| {
            FraiseQLError::ConnectionPool {
                message: format!("Failed to acquire connection: {e}"),
            }
        })?;

        // Query information_schema for tables matching tf_* pattern
        let query = r"
            SELECT table_name
            FROM information_schema.tables
            WHERE table_schema = 'public'
              AND table_type = 'BASE TABLE'
              AND table_name LIKE 'tf_%'
            ORDER BY table_name
        ";

        let rows: Vec<Row> = client.query(query, &[]).await.map_err(|e| {
            FraiseQLError::Database {
                message: format!("Failed to list fact tables: {e}"),
                sql_state: e.code().map(|c| c.code().to_string()),
            }
        })?;

        let tables = rows
            .into_iter()
            .map(|row| {
                let name: String = row.get(0);
                name
            })
            .collect();

        Ok(tables)
    }

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

    async fn get_sample_jsonb(&self, table_name: &str, column_name: &str) -> Result<Option<serde_json::Value>> {
        let client = self.pool.get().await.map_err(|e| {
            FraiseQLError::ConnectionPool {
                message: format!("Failed to acquire connection: {e}"),
            }
        })?;

        // Query for a sample row with non-null JSON data
        // Use format! for identifiers (safe because we validate table_name pattern)
        let query = format!(
            r#"
            SELECT "{column}"::text
            FROM "{table}"
            WHERE "{column}" IS NOT NULL
            LIMIT 1
            "#,
            table = table_name,
            column = column_name
        );

        let rows: Vec<Row> = client.query(&query, &[]).await.map_err(|e| {
            FraiseQLError::Database {
                message: format!("Failed to query sample JSONB: {e}"),
                sql_state: e.code().map(|c| c.code().to_string()),
            }
        })?;

        if rows.is_empty() {
            return Ok(None);
        }

        let json_text: Option<String> = rows[0].get(0);
        if let Some(text) = json_text {
            let value: serde_json::Value = serde_json::from_str(&text).map_err(|e| {
                FraiseQLError::Parse {
                    message: format!("Failed to parse JSONB sample: {e}"),
                    location: format!("{table_name}.{column_name}"),
                }
            })?;
            Ok(Some(value))
        } else {
            Ok(None)
        }
    }
}

#[cfg(all(test, feature = "test-postgres"))]
mod tests {
    use super::*;
    use crate::db::postgres::PostgresAdapter;
    use deadpool_postgres::{Config, ManagerConfig, RecyclingMethod, Runtime};
    use tokio_postgres::NoTls;

    const TEST_DB_URL: &str = "postgresql://fraiseql_test:fraiseql_test_password@localhost:5433/test_fraiseql";

    // Helper to create test introspector
    async fn create_test_introspector() -> PostgresIntrospector {
        let _adapter = PostgresAdapter::new(TEST_DB_URL)
            .await
            .expect("Failed to create test adapter");

        // Extract pool from adapter (we need a way to get the pool)
        // For now, create a new pool directly

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
    async fn test_database_type() {
        let introspector = create_test_introspector().await;
        assert_eq!(introspector.database_type(), DatabaseType::PostgreSQL);
    }
}
