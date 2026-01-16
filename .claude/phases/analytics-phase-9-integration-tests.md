# FraiseQL v2 - Analytics Phase 9: Integration Tests

**Status**: ⏳ Not Started
**Priority**: High
**Estimated Effort**: 2-3 days
**Dependencies**: Phases 1-8 complete

---

## Objective

Comprehensive integration testing across all analytics features and databases:
- End-to-end query execution tests
- Multi-database compatibility tests
- Performance benchmarks
- Error handling tests
- Real-world scenario tests

---

## Context

Phase 9 ensures all analytics components work together correctly across all supported databases. Tests cover:
- PostgreSQL (primary database, full features)
- MySQL 8.0+ (secondary database, basic features)
- SQLite 3.25+ (local dev, basic features)
- SQL Server (enterprise, statistical functions)

**Test Strategy**:
1. Unit tests → Module-level correctness
2. Integration tests → Cross-module correctness
3. End-to-end tests → Full pipeline validation
4. Database-specific tests → Compatibility verification

---

## Files to Create

### Test Infrastructure
```
tests/common/test_db.rs          # Database setup utilities
tests/common/test_data.rs        # Test data generators
tests/common/assertions.rs       # Custom assertion helpers
```

### Integration Test Suites
```
tests/integration/e2e_aggregate_queries.rs
tests/integration/e2e_window_functions.rs
tests/integration/database_compatibility.rs
tests/integration/error_handling.rs
tests/integration/performance_tests.rs
```

### Test Fixtures
```
tests/fixtures/fact_tables.sql           # Fact table schemas
tests/fixtures/sample_data.sql           # Sample data for testing
tests/fixtures/complex_queries.json      # Complex query scenarios
```

---

## Implementation Steps

### Step 1: Create Test Infrastructure

**Duration**: 4 hours

**Create `tests/common/test_db.rs`**:
```rust
//! Database setup and teardown utilities for integration tests

use deadpool_postgres::{Config, ManagerConfig, Pool, RecyclingMethod, Runtime};
use fraiseql_core::db::postgres::PostgresAdapter;
use fraiseql_core::db::types::DatabaseType;
use std::sync::Arc;
use tokio_postgres::NoTls;

pub const TEST_DB_URLS: TestDatabaseUrls = TestDatabaseUrls {
    postgres: "postgresql://fraiseql_test:fraiseql_test_password@localhost:5433/test_fraiseql",
    mysql: "mysql://fraiseql_test:fraiseql_test_password@localhost:3307/test_fraiseql",
    sqlite: "sqlite::memory:",
    sqlserver: "sqlserver://SA:YourStrong!Passw0rd@localhost:1434/test_fraiseql",
};

pub struct TestDatabaseUrls {
    pub postgres: &'static str,
    pub mysql: &'static str,
    pub sqlite: &'static str,
    pub sqlserver: &'static str,
}

/// Test database adapter
pub struct TestDatabase {
    pub adapter: Arc<dyn fraiseql_core::db::traits::DatabaseAdapter>,
    pub db_type: DatabaseType,
}

impl TestDatabase {
    /// Create PostgreSQL test database
    pub async fn postgres() -> Result<Self, Box<dyn std::error::Error>> {
        let adapter = Arc::new(PostgresAdapter::new(TEST_DB_URLS.postgres).await?);
        Ok(Self {
            adapter,
            db_type: DatabaseType::PostgreSQL,
        })
    }

    /// Create MySQL test database
    pub async fn mysql() -> Result<Self, Box<dyn std::error::Error>> {
        // TODO: Implement MySQL adapter
        unimplemented!("MySQL adapter not yet implemented")
    }

    /// Create SQLite test database
    pub async fn sqlite() -> Result<Self, Box<dyn std::error::Error>> {
        // TODO: Implement SQLite adapter
        unimplemented!("SQLite adapter not yet implemented")
    }

    /// Create SQL Server test database
    pub async fn sqlserver() -> Result<Self, Box<dyn std::error::Error>> {
        // TODO: Implement SQL Server adapter
        unimplemented!("SQL Server adapter not yet implemented")
    }

    /// Setup fact table schema
    pub async fn setup_fact_table(&self, table_name: &str) -> Result<(), Box<dyn std::error::Error>> {
        let sql = match self.db_type {
            DatabaseType::PostgreSQL => format!(
                r#"
                DROP TABLE IF EXISTS {table_name} CASCADE;
                CREATE TABLE {table_name} (
                    id BIGSERIAL PRIMARY KEY,
                    revenue DECIMAL(10,2) NOT NULL,
                    quantity INT NOT NULL,
                    data JSONB NOT NULL,
                    customer_id UUID NOT NULL,
                    occurred_at TIMESTAMPTZ NOT NULL
                );
                CREATE INDEX idx_{table_name}_customer ON {table_name}(customer_id);
                CREATE INDEX idx_{table_name}_occurred_at ON {table_name}(occurred_at);
                CREATE INDEX idx_{table_name}_data ON {table_name} USING GIN(data);
                "#,
                table_name = table_name
            ),
            DatabaseType::MySQL => format!(
                r#"
                DROP TABLE IF EXISTS {table_name};
                CREATE TABLE {table_name} (
                    id BIGINT AUTO_INCREMENT PRIMARY KEY,
                    revenue DECIMAL(10,2) NOT NULL,
                    quantity INT NOT NULL,
                    data JSON NOT NULL,
                    customer_id CHAR(36) NOT NULL,
                    occurred_at DATETIME NOT NULL,
                    INDEX idx_customer (customer_id),
                    INDEX idx_occurred_at (occurred_at)
                );
                "#,
                table_name = table_name
            ),
            DatabaseType::SQLite => format!(
                r#"
                DROP TABLE IF EXISTS {table_name};
                CREATE TABLE {table_name} (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    revenue REAL NOT NULL,
                    quantity INTEGER NOT NULL,
                    data TEXT NOT NULL,
                    customer_id TEXT NOT NULL,
                    occurred_at TEXT NOT NULL
                );
                CREATE INDEX idx_{table_name}_customer ON {table_name}(customer_id);
                CREATE INDEX idx_{table_name}_occurred_at ON {table_name}(occurred_at);
                "#,
                table_name = table_name
            ),
            DatabaseType::SQLServer => format!(
                r#"
                IF OBJECT_ID('{table_name}', 'U') IS NOT NULL DROP TABLE {table_name};
                CREATE TABLE {table_name} (
                    id BIGINT IDENTITY(1,1) PRIMARY KEY,
                    revenue DECIMAL(10,2) NOT NULL,
                    quantity INT NOT NULL,
                    data NVARCHAR(MAX) NOT NULL,
                    customer_id UNIQUEIDENTIFIER NOT NULL,
                    occurred_at DATETIME2 NOT NULL
                );
                CREATE INDEX idx_{table_name}_customer ON {table_name}(customer_id);
                CREATE INDEX idx_{table_name}_occurred_at ON {table_name}(occurred_at);
                "#,
                table_name = table_name
            ),
        };

        self.adapter.execute_raw(&sql, &[]).await?;
        Ok(())
    }

    /// Insert test data
    pub async fn insert_test_data(&self, table_name: &str, rows: Vec<TestRow>) -> Result<(), Box<dyn std::error::Error>> {
        for row in rows {
            let sql = match self.db_type {
                DatabaseType::PostgreSQL => format!(
                    r#"
                    INSERT INTO {table_name} (revenue, quantity, data, customer_id, occurred_at)
                    VALUES ($1, $2, $3, $4, $5)
                    "#,
                    table_name = table_name
                ),
                DatabaseType::MySQL => format!(
                    r#"
                    INSERT INTO {table_name} (revenue, quantity, data, customer_id, occurred_at)
                    VALUES (?, ?, ?, ?, ?)
                    "#,
                    table_name = table_name
                ),
                DatabaseType::SQLite | DatabaseType::SQLServer => format!(
                    r#"
                    INSERT INTO {table_name} (revenue, quantity, data, customer_id, occurred_at)
                    VALUES (?, ?, ?, ?, ?)
                    "#,
                    table_name = table_name
                ),
            };

            let params = vec![
                serde_json::json!(row.revenue),
                serde_json::json!(row.quantity),
                serde_json::json!(row.data),
                serde_json::json!(row.customer_id),
                serde_json::json!(row.occurred_at),
            ];

            self.adapter.execute_raw(&sql, &params).await?;
        }

        Ok(())
    }

    /// Cleanup (drop all test tables)
    pub async fn cleanup(&self) -> Result<(), Box<dyn std::error::Error>> {
        let tables = vec!["tf_sales", "tf_orders", "tf_events"];

        for table in tables {
            let sql = match self.db_type {
                DatabaseType::PostgreSQL => format!("DROP TABLE IF EXISTS {} CASCADE", table),
                DatabaseType::MySQL => format!("DROP TABLE IF EXISTS {}", table),
                DatabaseType::SQLite => format!("DROP TABLE IF EXISTS {}", table),
                DatabaseType::SQLServer => format!(
                    "IF OBJECT_ID('{}', 'U') IS NOT NULL DROP TABLE {}",
                    table, table
                ),
            };

            let _ = self.adapter.execute_raw(&sql, &[]).await; // Ignore errors
        }

        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct TestRow {
    pub revenue: f64,
    pub quantity: i32,
    pub data: serde_json::Value,
    pub customer_id: String,
    pub occurred_at: String,
}

impl TestRow {
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

/// Generate sample test data
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
```

**Create `tests/common/assertions.rs`**:
```rust
//! Custom assertion helpers for analytics tests

use serde_json::Value;

/// Assert aggregate result structure
pub fn assert_aggregate_result(result: &Value, query_name: &str) {
    assert!(result["data"].is_object(), "Expected 'data' object");
    assert!(
        result["data"][query_name].is_array(),
        "Expected '{}' array",
        query_name
    );
}

/// Assert aggregate row has required fields
pub fn assert_aggregate_row_has_fields(row: &Value, fields: &[&str]) {
    for field in fields {
        assert!(
            row.get(field).is_some(),
            "Expected field '{}' in row: {:?}",
            field,
            row
        );
    }
}

/// Assert aggregate count matches expected
pub fn assert_result_count(result: &Value, query_name: &str, expected: usize) {
    let actual = result["data"][query_name].as_array().unwrap().len();
    assert_eq!(
        actual, expected,
        "Expected {} results, got {}",
        expected, actual
    );
}

/// Assert numeric field value is close (for floating point)
pub fn assert_numeric_close(actual: f64, expected: f64, epsilon: f64) {
    assert!(
        (actual - expected).abs() < epsilon,
        "Expected {} ± {}, got {}",
        expected,
        epsilon,
        actual
    );
}

/// Assert SQL contains expected clauses
pub fn assert_sql_contains(sql: &str, expected: &[&str]) {
    for clause in expected {
        assert!(
            sql.contains(clause),
            "Expected SQL to contain '{}'\nActual SQL: {}",
            clause,
            sql
        );
    }
}
```

**Verification**:
```bash
cargo test -p fraiseql-core --test '*' common::
```

---

### Step 2: End-to-End Aggregate Query Tests

**Duration**: 6 hours

**Create `tests/integration/e2e_aggregate_queries.rs`**:
```rust
//! End-to-end aggregate query tests

use fraiseql_core::compiler::fact_table::*;
use fraiseql_core::runtime::{AggregateQueryParser, AggregationPlanner, AggregationSqlGenerator, Executor};
use fraiseql_core::schema::CompiledSchema;
use serde_json::json;

mod common;
use common::{TestDatabase, generate_test_data, assert_aggregate_result, assert_result_count};

#[tokio::test]
async fn test_simple_count_all() {
    let db = TestDatabase::postgres().await.unwrap();
    db.setup_fact_table("tf_sales").await.unwrap();
    db.insert_test_data("tf_sales", generate_test_data(100)).await.unwrap();

    let executor = create_executor(db.adapter.clone());
    let metadata = create_sales_metadata();

    let query = json!({
        "table": "tf_sales",
        "aggregates": [{"count": {}}]
    });

    let result = executor
        .execute_aggregate_query(&query, "sales_aggregate", &metadata)
        .await
        .unwrap();

    let response: serde_json::Value = serde_json::from_str(&result).unwrap();

    assert_aggregate_result(&response, "sales_aggregate");
    assert_result_count(&response, "sales_aggregate", 1);
    assert_eq!(response["data"]["sales_aggregate"][0]["count"], 100);

    db.cleanup().await.unwrap();
}

#[tokio::test]
async fn test_group_by_single_dimension() {
    let db = TestDatabase::postgres().await.unwrap();
    db.setup_fact_table("tf_sales").await.unwrap();
    db.insert_test_data("tf_sales", generate_test_data(50)).await.unwrap();

    let executor = create_executor(db.adapter.clone());
    let metadata = create_sales_metadata();

    let query = json!({
        "table": "tf_sales",
        "groupBy": {"category": true},
        "aggregates": [
            {"count": {}},
            {"revenue_sum": {}}
        ]
    });

    let result = executor
        .execute_aggregate_query(&query, "sales_aggregate", &metadata)
        .await
        .unwrap();

    let response: serde_json::Value = serde_json::from_str(&result).unwrap();

    assert_aggregate_result(&response, "sales_aggregate");

    let results = response["data"]["sales_aggregate"].as_array().unwrap();
    assert!(results.len() > 0 && results.len() <= 5); // 5 categories in test data

    // Verify each result has category, count, revenue_sum
    for row in results {
        assert!(row["category"].is_string());
        assert!(row["count"].is_number());
        assert!(row["revenue_sum"].is_number());
    }

    db.cleanup().await.unwrap();
}

#[tokio::test]
async fn test_group_by_multiple_dimensions() {
    let db = TestDatabase::postgres().await.unwrap();
    db.setup_fact_table("tf_sales").await.unwrap();
    db.insert_test_data("tf_sales", generate_test_data(100)).await.unwrap();

    let executor = create_executor(db.adapter.clone());
    let metadata = create_sales_metadata();

    let query = json!({
        "table": "tf_sales",
        "groupBy": {
            "category": true,
            "region": true
        },
        "aggregates": [
            {"count": {}},
            {"revenue_sum": {}},
            {"revenue_avg": {}},
            {"quantity_sum": {}}
        ]
    });

    let result = executor
        .execute_aggregate_query(&query, "sales_aggregate", &metadata)
        .await
        .unwrap();

    let response: serde_json::Value = serde_json::from_str(&result).unwrap();

    assert_aggregate_result(&response, "sales_aggregate");

    let results = response["data"]["sales_aggregate"].as_array().unwrap();
    assert!(results.len() > 0); // Should have multiple combinations

    for row in results {
        assert!(row["category"].is_string());
        assert!(row["region"].is_string());
        assert!(row["count"].is_number());
        assert!(row["revenue_sum"].is_number());
        assert!(row["revenue_avg"].is_number());
        assert!(row["quantity_sum"].is_number());
    }

    db.cleanup().await.unwrap();
}

#[tokio::test]
async fn test_temporal_bucketing_day() {
    let db = TestDatabase::postgres().await.unwrap();
    db.setup_fact_table("tf_sales").await.unwrap();
    db.insert_test_data("tf_sales", generate_test_data(90)).await.unwrap(); // 3 months of data

    let executor = create_executor(db.adapter.clone());
    let metadata = create_sales_metadata();

    let query = json!({
        "table": "tf_sales",
        "groupBy": {"occurred_at_day": true},
        "aggregates": [
            {"count": {}},
            {"revenue_sum": {}}
        ],
        "orderBy": [{"field": "occurred_at_day", "direction": "ASC"}]
    });

    let result = executor
        .execute_aggregate_query(&query, "sales_aggregate", &metadata)
        .await
        .unwrap();

    let response: serde_json::Value = serde_json::from_str(&result).unwrap();

    assert_aggregate_result(&response, "sales_aggregate");

    let results = response["data"]["sales_aggregate"].as_array().unwrap();
    assert!(results.len() > 0);

    // Verify temporal buckets are ordered
    for i in 1..results.len() {
        let prev_date = results[i - 1]["occurred_at_day"].as_str().unwrap();
        let curr_date = results[i]["occurred_at_day"].as_str().unwrap();
        assert!(curr_date >= prev_date, "Results should be ordered by date");
    }

    db.cleanup().await.unwrap();
}

#[tokio::test]
async fn test_having_clause() {
    let db = TestDatabase::postgres().await.unwrap();
    db.setup_fact_table("tf_sales").await.unwrap();
    db.insert_test_data("tf_sales", generate_test_data(100)).await.unwrap();

    let executor = create_executor(db.adapter.clone());
    let metadata = create_sales_metadata();

    let query = json!({
        "table": "tf_sales",
        "groupBy": {"category": true},
        "aggregates": [
            {"count": {}},
            {"revenue_sum": {}}
        ],
        "having": {
            "revenue_sum_gt": 1000.0
        }
    });

    let result = executor
        .execute_aggregate_query(&query, "sales_aggregate", &metadata)
        .await
        .unwrap();

    let response: serde_json::Value = serde_json::from_str(&result).unwrap();

    assert_aggregate_result(&response, "sales_aggregate");

    let results = response["data"]["sales_aggregate"].as_array().unwrap();

    // Verify all results meet HAVING condition
    for row in results {
        let revenue_sum = row["revenue_sum"].as_f64().unwrap();
        assert!(revenue_sum > 1000.0, "HAVING filter not applied correctly");
    }

    db.cleanup().await.unwrap();
}

#[tokio::test]
async fn test_order_by_aggregate() {
    let db = TestDatabase::postgres().await.unwrap();
    db.setup_fact_table("tf_sales").await.unwrap();
    db.insert_test_data("tf_sales", generate_test_data(100)).await.unwrap();

    let executor = create_executor(db.adapter.clone());
    let metadata = create_sales_metadata();

    let query = json!({
        "table": "tf_sales",
        "groupBy": {"category": true},
        "aggregates": [
            {"count": {}},
            {"revenue_sum": {}}
        ],
        "orderBy": [{"field": "revenue_sum", "direction": "DESC"}],
        "limit": 5
    });

    let result = executor
        .execute_aggregate_query(&query, "sales_aggregate", &metadata)
        .await
        .unwrap();

    let response: serde_json::Value = serde_json::from_str(&result).unwrap();

    assert_aggregate_result(&response, "sales_aggregate");
    assert_result_count(&response, "sales_aggregate", 5);

    let results = response["data"]["sales_aggregate"].as_array().unwrap();

    // Verify descending order
    for i in 1..results.len() {
        let prev_revenue = results[i - 1]["revenue_sum"].as_f64().unwrap();
        let curr_revenue = results[i]["revenue_sum"].as_f64().unwrap();
        assert!(
            curr_revenue <= prev_revenue,
            "Results not sorted correctly by revenue_sum DESC"
        );
    }

    db.cleanup().await.unwrap();
}

#[tokio::test]
async fn test_all_aggregate_functions() {
    let db = TestDatabase::postgres().await.unwrap();
    db.setup_fact_table("tf_sales").await.unwrap();
    db.insert_test_data("tf_sales", generate_test_data(50)).await.unwrap();

    let executor = create_executor(db.adapter.clone());
    let metadata = create_sales_metadata();

    let query = json!({
        "table": "tf_sales",
        "groupBy": {"category": true},
        "aggregates": [
            {"count": {}},
            {"revenue_sum": {}},
            {"revenue_avg": {}},
            {"revenue_min": {}},
            {"revenue_max": {}},
            {"revenue_stddev": {}},
            {"revenue_variance": {}},
            {"quantity_sum": {}},
            {"quantity_avg": {}}
        ]
    });

    let result = executor
        .execute_aggregate_query(&query, "sales_aggregate", &metadata)
        .await
        .unwrap();

    let response: serde_json::Value = serde_json::from_str(&result).unwrap();

    assert_aggregate_result(&response, "sales_aggregate");

    let results = response["data"]["sales_aggregate"].as_array().unwrap();
    assert!(results.len() > 0);

    // Verify all aggregate fields present
    let row = &results[0];
    assert!(row["count"].is_number());
    assert!(row["revenue_sum"].is_number());
    assert!(row["revenue_avg"].is_number());
    assert!(row["revenue_min"].is_number());
    assert!(row["revenue_max"].is_number());
    assert!(row["revenue_stddev"].is_number());
    assert!(row["revenue_variance"].is_number());
    assert!(row["quantity_sum"].is_number());
    assert!(row["quantity_avg"].is_number());

    // Verify aggregate logic
    let revenue_min = row["revenue_min"].as_f64().unwrap();
    let revenue_max = row["revenue_max"].as_f64().unwrap();
    let revenue_avg = row["revenue_avg"].as_f64().unwrap();
    assert!(revenue_min <= revenue_avg);
    assert!(revenue_avg <= revenue_max);

    db.cleanup().await.unwrap();
}

// Helper functions
fn create_executor(adapter: std::sync::Arc<dyn fraiseql_core::db::traits::DatabaseAdapter>) -> Executor {
    let schema = CompiledSchema::new();
    Executor::new(schema, adapter)
}

fn create_sales_metadata() -> FactTableMetadata {
    // Same as in previous test files
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
        denormalized_filters: vec![FilterColumn {
            name: "occurred_at".to_string(),
            sql_type: SqlType::Timestamp,
            indexed: true,
        }],
    }
}
```

**Tests to Add** (8+ test cases):
- ✅ Simple count all
- ✅ Group by single dimension
- ✅ Group by multiple dimensions
- ✅ Temporal bucketing (day/week/month)
- ✅ HAVING clause filtering
- ✅ ORDER BY aggregates
- ✅ All aggregate functions
- [ ] WHERE + HAVING combined
- [ ] LIMIT + OFFSET pagination
- [ ] Empty result sets
- [ ] NULL handling

**Verification**:
```bash
cargo test --test e2e_aggregate_queries
```

---

### Step 3: Multi-Database Compatibility Tests

**Duration**: 4 hours

**Create `tests/integration/database_compatibility.rs`**:
```rust
//! Test analytics features across all databases

mod common;
use common::{TestDatabase, generate_test_data, assert_sql_contains};
use fraiseql_core::compiler::aggregation::*;
use fraiseql_core::runtime::AggregationSqlGenerator;
use fraiseql_core::db::types::DatabaseType;

#[test]
fn test_postgres_temporal_bucketing() {
    let generator = AggregationSqlGenerator::new(DatabaseType::PostgreSQL);
    let plan = create_temporal_plan(TemporalBucket::Day);

    let sql = generator.generate(&plan).unwrap();

    assert_sql_contains(&sql.complete_sql, &[
        "DATE_TRUNC('day'",
        "GROUP BY",
        "occurred_at"
    ]);
}

#[test]
fn test_mysql_temporal_bucketing() {
    let generator = AggregationSqlGenerator::new(DatabaseType::MySQL);
    let plan = create_temporal_plan(TemporalBucket::Day);

    let sql = generator.generate(&plan).unwrap();

    assert_sql_contains(&sql.complete_sql, &[
        "DATE_FORMAT",
        "GROUP BY",
        "occurred_at"
    ]);
}

#[test]
fn test_sqlite_temporal_bucketing() {
    let generator = AggregationSqlGenerator::new(DatabaseType::SQLite);
    let plan = create_temporal_plan(TemporalBucket::Day);

    let sql = generator.generate(&plan).unwrap();

    assert_sql_contains(&sql.complete_sql, &[
        "strftime",
        "GROUP BY",
        "occurred_at"
    ]);
}

#[test]
fn test_postgres_statistical_functions() {
    let generator = AggregationSqlGenerator::new(DatabaseType::PostgreSQL);
    let plan = create_statistical_plan();

    let sql = generator.generate(&plan).unwrap();

    assert_sql_contains(&sql.complete_sql, &[
        "STDDEV(revenue)",
        "VARIANCE(revenue)"
    ]);
}

#[test]
fn test_mysql_no_statistical_functions() {
    let generator = AggregationSqlGenerator::new(DatabaseType::MySQL);
    let plan = create_statistical_plan();

    // Should return error for unsupported functions
    let result = generator.generate(&plan);
    assert!(result.is_err());
}

// More database-specific tests...
```

**Verification**:
```bash
cargo test --test database_compatibility
```

---

### Step 4: Performance Tests

**Duration**: 3 hours

**Create `tests/integration/performance_tests.rs`**:
```rust
//! Performance benchmarks for analytics queries

mod common;
use common::{TestDatabase, generate_test_data};
use std::time::Instant;

#[tokio::test]
#[ignore] // Run manually with --ignored
async fn benchmark_simple_aggregation() {
    let db = TestDatabase::postgres().await.unwrap();
    db.setup_fact_table("tf_sales").await.unwrap();

    // Insert 100K rows
    db.insert_test_data("tf_sales", generate_test_data(100_000))
        .await
        .unwrap();

    let executor = create_executor(db.adapter.clone());
    let metadata = create_sales_metadata();

    let query = json!({
        "table": "tf_sales",
        "groupBy": {"category": true},
        "aggregates": [{"count": {}}, {"revenue_sum": {}}]
    });

    let start = Instant::now();
    let _ = executor
        .execute_aggregate_query(&query, "sales_aggregate", &metadata)
        .await
        .unwrap();
    let elapsed = start.elapsed();

    println!("Simple aggregation (100K rows): {:?}", elapsed);
    assert!(elapsed.as_millis() < 500, "Query too slow: {:?}", elapsed);

    db.cleanup().await.unwrap();
}

// More performance tests...
```

**Verification**:
```bash
cargo test --test performance_tests -- --ignored --nocapture
```

---

## Acceptance Criteria

- [ ] 30+ integration tests created
- [ ] All tests pass on PostgreSQL
- [ ] MySQL compatibility tests pass (or skipped with clear reason)
- [ ] SQLite compatibility tests pass (or skipped)
- [ ] SQL Server compatibility tests pass (or skipped)
- [ ] End-to-end aggregate query tests (10+ scenarios)
- [ ] Window function tests (if Phase 7 complete)
- [ ] Error handling tests
- [ ] Performance benchmarks run successfully
- [ ] Test coverage >80% for analytics modules
- [ ] CI/CD integration configured

---

## Verification Commands

```bash
# Run all integration tests
cargo test --test '*' -- --test-threads=1

# Run with PostgreSQL
cargo test --test e2e_aggregate_queries

# Run database compatibility
cargo test --test database_compatibility

# Run performance benchmarks (manual)
cargo test --test performance_tests -- --ignored --nocapture

# Check test coverage (requires cargo-tarpaulin)
cargo tarpaulin --out Html --output-dir coverage
```

**Expected Output**:
```
running 35 tests
test e2e_aggregate_queries::test_simple_count_all ... ok
test e2e_aggregate_queries::test_group_by_single_dimension ... ok
test e2e_aggregate_queries::test_temporal_bucketing_day ... ok
test database_compatibility::test_postgres_temporal_bucketing ... ok
test database_compatibility::test_mysql_temporal_bucketing ... ok
...
test result: ok. 35 passed; 0 failed; 0 ignored
```

---

## DO NOT

- ❌ Don't skip database-specific tests without documentation
- ❌ Don't hardcode test data in tests (use generators)
- ❌ Don't forget to cleanup test databases
- ❌ Don't run performance tests in CI (too slow)
- ❌ Don't ignore flaky tests (fix or mark as known issue)

---

## Notes

**Test Organization**:
- `common/` - Shared test utilities
- `integration/` - Cross-module integration tests
- `fixtures/` - SQL schemas and sample data

**Database Setup**:
- PostgreSQL: Docker container on port 5433
- MySQL: Docker container on port 3307
- SQLite: In-memory (no setup needed)
- SQL Server: Docker container on port 1434

**CI/CD Integration**:
```yaml
# .github/workflows/test.yml
name: Analytics Tests
on: [push, pull_request]
jobs:
  test:
    runs-on: ubuntu-latest
    services:
      postgres:
        image: postgres:15
        env:
          POSTGRES_PASSWORD: fraiseql_test_password
          POSTGRES_USER: fraiseql_test
          POSTGRES_DB: test_fraiseql
        ports:
          - 5433:5432
    steps:
      - uses: actions/checkout@v3
      - name: Run tests
        run: cargo test --test '*'
```

**Test Data Considerations**:
- Generate deterministic data for reproducible tests
- Use realistic data distributions
- Include edge cases (nulls, empty strings, etc.)
- Vary data sizes (10, 100, 1000, 10K rows)
