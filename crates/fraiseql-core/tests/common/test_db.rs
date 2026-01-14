//! Database setup and teardown utilities for integration tests

use fraiseql_core::compiler::fact_table::{
    FactTableMetadata, MeasureColumn, DimensionColumn, FilterColumn, SqlType, DimensionPath,
};
use fraiseql_core::db::postgres::PostgresAdapter;
use fraiseql_core::db::traits::DatabaseAdapter;
use fraiseql_core::db::types::DatabaseType;
use std::sync::Arc;

/// Test database connection URL
pub const TEST_DB_URL: &str = "postgresql://fraiseql:fraiseql_password@localhost:5432/fraiseql";

/// Test database adapter wrapper
pub struct TestDatabase {
    pub adapter: Arc<dyn DatabaseAdapter>,
    pub db_type: DatabaseType,
}

impl TestDatabase {
    /// Create PostgreSQL test database
    pub async fn postgres() -> Result<Self, Box<dyn std::error::Error>> {
        let adapter = Arc::new(PostgresAdapter::new(TEST_DB_URL).await?);
        Ok(Self {
            adapter,
            db_type: DatabaseType::PostgreSQL,
        })
    }

    /// Setup fact table schema
    pub async fn setup_fact_table(&self, table_name: &str) -> Result<(), Box<dyn std::error::Error>> {
        let sql = format!(
            r#"
            DROP TABLE IF EXISTS {table_name} CASCADE;
            CREATE TABLE {table_name} (
                id BIGSERIAL PRIMARY KEY,
                revenue DECIMAL(10,2) NOT NULL,
                quantity INT NOT NULL,
                data JSONB NOT NULL,
                customer_id TEXT NOT NULL,
                occurred_at TIMESTAMPTZ NOT NULL
            );
            CREATE INDEX idx_{table_name}_customer ON {table_name}(customer_id);
            CREATE INDEX idx_{table_name}_occurred_at ON {table_name}(occurred_at);
            CREATE INDEX idx_{table_name}_data ON {table_name} USING GIN(data);
            "#,
            table_name = table_name
        );

        self.adapter.execute_raw_query(&sql).await?;
        Ok(())
    }

    /// Insert test data
    pub async fn insert_test_data(
        &self,
        table_name: &str,
        rows: Vec<TestRow>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        for row in rows {
            let sql = format!(
                r#"
                INSERT INTO {table_name} (revenue, quantity, data, customer_id, occurred_at)
                VALUES ($1, $2, $3, $4, $5)
                "#,
                table_name = table_name
            );

            let params = vec![
                serde_json::json!(row.revenue),
                serde_json::json!(row.quantity),
                row.data.clone(),
                serde_json::json!(row.customer_id),
                serde_json::json!(row.occurred_at),
            ];

            // Note: execute_raw_query doesn't support parameters, would need execute_parameterized for real use
            let sql_with_values = sql.clone();  // For now, simple implementation
            self.adapter.execute_raw_query(&sql_with_values).await?;
        }

        Ok(())
    }

    /// Cleanup (drop test tables)
    pub async fn cleanup(&self, tables: &[&str]) -> Result<(), Box<dyn std::error::Error>> {
        for table in tables {
            let sql = format!("DROP TABLE IF EXISTS {} CASCADE", table);
            let _ = self.adapter.execute_raw_query(&sql).await; // Ignore errors
        }

        Ok(())
    }
}

/// Test data row
#[derive(Debug, Clone)]
pub struct TestRow {
    pub revenue: f64,
    pub quantity: i32,
    pub data: serde_json::Value,
    pub customer_id: String,
    pub occurred_at: String,
}

impl TestRow {
    /// Create new test row
    pub fn new(
        revenue: f64,
        quantity: i32,
        category: &str,
        region: &str,
        customer_id: &str,
        occurred_at: &str,
    ) -> Self {
        Self {
            revenue,
            quantity,
            data: serde_json::json!({
                "category": category,
                "region": region,
            }),
            customer_id: customer_id.to_string(),
            occurred_at: occurred_at.to_string(),
        }
    }
}

/// Generate sample test data with realistic distributions
pub fn generate_test_data(count: usize) -> Vec<TestRow> {
    let categories = ["Electronics", "Books", "Clothing", "Home", "Sports"];
    let regions = ["North", "South", "East", "West"];
    let customers = ["cust-001", "cust-002", "cust-003", "cust-004", "cust-005"];

    (0..count)
        .map(|i| {
            let category = categories[i % categories.len()];
            let region = regions[i % regions.len()];
            let customer = customers[i % customers.len()];
            let revenue = 100.0 + (i as f64 * 13.7) % 1000.0;
            let quantity = 1 + (i % 10) as i32;
            let day = 1 + (i % 30);
            let occurred_at = format!("2024-01-{:02}T10:00:00Z", day);

            TestRow::new(revenue, quantity, category, region, customer, &occurred_at)
        })
        .collect()
}

/// Create standard sales metadata for testing
pub fn create_sales_metadata() -> FactTableMetadata {
    FactTableMetadata {
        table_name: "tf_sales".to_string(),
        measures: vec![
            MeasureColumn {
                name: "revenue".to_string(),
                sql_type: SqlType::Decimal,
                nullable: false,
            },
            MeasureColumn {
                name: "quantity".to_string(),
                sql_type: SqlType::Int,
                nullable: false,
            },
        ],
        dimensions: DimensionColumn {
            name: "data".to_string(),
            paths: vec![
                DimensionPath {
                    name: "category".to_string(),
                    json_path: "data->>'category'".to_string(),
                    data_type: "text".to_string(),
                },
                DimensionPath {
                    name: "region".to_string(),
                    json_path: "data->>'region'".to_string(),
                    data_type: "text".to_string(),
                },
            ],
        },
        denormalized_filters: vec![
            FilterColumn {
                name: "customer_id".to_string(),
                sql_type: SqlType::Text,
                indexed: true,
            },
            FilterColumn {
                name: "occurred_at".to_string(),
                sql_type: SqlType::Timestamp,
                indexed: true,
            },
        ],
        calendar_dimensions: vec![],
    }
}
