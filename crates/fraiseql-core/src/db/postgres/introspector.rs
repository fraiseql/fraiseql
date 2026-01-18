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

impl PostgresIntrospector {
    /// Get indexed columns for a view/table that match the nested path naming convention.
    ///
    /// This method introspects the database to find columns that follow the FraiseQL
    /// indexed column naming conventions:
    /// - Human-readable: `items__product__category__code` (double underscore separated)
    /// - Entity ID format: `f{entity_id}__{field_name}` (e.g., `f200100__code`)
    ///
    /// These columns are created by DBAs to optimize filtering on nested GraphQL paths
    /// by avoiding JSONB extraction at runtime.
    ///
    /// # Arguments
    ///
    /// * `view_name` - Name of the view or table to introspect
    ///
    /// # Returns
    ///
    /// Set of column names that match the indexed column naming conventions.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let introspector = PostgresIntrospector::new(pool);
    /// let indexed_cols = introspector.get_indexed_nested_columns("v_order_items").await?;
    /// // Returns: {"items__product__category__code", "f200100__code", ...}
    /// ```
    pub async fn get_indexed_nested_columns(
        &self,
        view_name: &str,
    ) -> Result<std::collections::HashSet<String>> {
        let client = self.pool.get().await.map_err(|e| {
            FraiseQLError::ConnectionPool {
                message: format!("Failed to acquire connection: {e}"),
            }
        })?;

        // Query information_schema for columns matching __ pattern
        // This works for both views and tables
        let query = r"
            SELECT column_name
            FROM information_schema.columns
            WHERE table_name = $1
              AND table_schema = 'public'
              AND column_name LIKE '%__%'
            ORDER BY column_name
        ";

        let rows: Vec<Row> = client.query(query, &[&view_name]).await.map_err(|e| {
            FraiseQLError::Database {
                message: format!("Failed to query view columns: {e}"),
                sql_state: e.code().map(|c| c.code().to_string()),
            }
        })?;

        let indexed_columns: std::collections::HashSet<String> = rows
            .into_iter()
            .map(|row| {
                let name: String = row.get(0);
                name
            })
            .filter(|name| {
                // Filter to only columns that match our naming conventions:
                // 1. Human-readable: path__to__field (at least one __ separator)
                // 2. Entity ID: f{digits}__field_name
                Self::is_indexed_column_name(name)
            })
            .collect();

        Ok(indexed_columns)
    }

    /// Check if a column name matches the indexed column naming convention.
    ///
    /// Valid patterns:
    /// - `items__product__category__code` (human-readable nested path)
    /// - `f200100__code` (entity ID format)
    fn is_indexed_column_name(name: &str) -> bool {
        // Must contain at least one double underscore
        if !name.contains("__") {
            return false;
        }

        // Check for entity ID format: f{digits}__field
        if let Some(rest) = name.strip_prefix('f') {
            if let Some(underscore_pos) = rest.find("__") {
                let digits = &rest[..underscore_pos];
                if digits.chars().all(|c| c.is_ascii_digit()) && !digits.is_empty() {
                    // Verify the field part is valid
                    let field_part = &rest[underscore_pos + 2..];
                    if !field_part.is_empty()
                        && field_part.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
                        && !field_part.starts_with(|c: char| c.is_ascii_digit())
                    {
                        return true;
                    }
                }
            }
        }

        // Human-readable format: split by __ and check each segment is valid identifier
        // Must have at least 2 segments, and first segment must NOT be just 'f'
        let segments: Vec<&str> = name.split("__").collect();
        if segments.len() < 2 {
            return false;
        }

        // Reject if first segment is just 'f' (reserved for entity ID format)
        if segments[0] == "f" {
            return false;
        }

        // Each segment should be a valid identifier (alphanumeric + underscore, not starting with digit)
        segments.iter().all(|s| {
            !s.is_empty()
                && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
                && !s.starts_with(|c: char| c.is_ascii_digit())
        })
    }

    /// Get all column names for a view/table.
    ///
    /// # Arguments
    ///
    /// * `view_name` - Name of the view or table to introspect
    ///
    /// # Returns
    ///
    /// List of all column names in the view/table.
    pub async fn get_view_columns(&self, view_name: &str) -> Result<Vec<String>> {
        let client = self.pool.get().await.map_err(|e| {
            FraiseQLError::ConnectionPool {
                message: format!("Failed to acquire connection: {e}"),
            }
        })?;

        let query = r"
            SELECT column_name
            FROM information_schema.columns
            WHERE table_name = $1
              AND table_schema = 'public'
            ORDER BY ordinal_position
        ";

        let rows: Vec<Row> = client.query(query, &[&view_name]).await.map_err(|e| {
            FraiseQLError::Database {
                message: format!("Failed to query view columns: {e}"),
                sql_state: e.code().map(|c| c.code().to_string()),
            }
        })?;

        let columns = rows
            .into_iter()
            .map(|row| {
                let name: String = row.get(0);
                name
            })
            .collect();

        Ok(columns)
    }
}

/// Unit tests that don't require a PostgreSQL connection.
#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    fn test_is_indexed_column_name_human_readable() {
        // Valid human-readable patterns
        assert!(PostgresIntrospector::is_indexed_column_name("items__product"));
        assert!(PostgresIntrospector::is_indexed_column_name("items__product__category"));
        assert!(PostgresIntrospector::is_indexed_column_name("items__product__category__code"));
        assert!(PostgresIntrospector::is_indexed_column_name("order_items__product_name"));

        // Invalid patterns
        assert!(!PostgresIntrospector::is_indexed_column_name("items"));
        assert!(!PostgresIntrospector::is_indexed_column_name("items_product")); // single underscore
        assert!(!PostgresIntrospector::is_indexed_column_name("__items")); // empty first segment
        assert!(!PostgresIntrospector::is_indexed_column_name("items__")); // empty last segment
    }

    #[test]
    fn test_is_indexed_column_name_entity_id() {
        // Valid entity ID patterns
        assert!(PostgresIntrospector::is_indexed_column_name("f200100__code"));
        assert!(PostgresIntrospector::is_indexed_column_name("f1__name"));
        assert!(PostgresIntrospector::is_indexed_column_name("f123456789__field"));

        // Invalid entity ID patterns (that also aren't valid human-readable)
        assert!(!PostgresIntrospector::is_indexed_column_name("f__code")); // no digits after 'f', and 'f' alone is reserved

        // Note: fx123__code IS valid as a human-readable pattern (fx123 is a valid identifier)
        assert!(PostgresIntrospector::is_indexed_column_name("fx123__code")); // valid as human-readable
    }
}

/// Integration tests that require a PostgreSQL connection.
#[cfg(all(test, feature = "test-postgres"))]
mod integration_tests {
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
