//! Integration tests for fact table introspection.
//!
//! These tests require a running PostgreSQL database with analytics test data.
//!
//! To run:
//!   1. Start test database: docker compose -f docker-compose.test.yml up -d
//!   2. Wait for DB: docker compose -f docker-compose.test.yml exec postgres-test pg_isready
//!   3. Run tests: cargo test -p fraiseql-core --test fact_table_integration -- --ignored
//!   4. Stop database: docker compose -f docker-compose.test.yml down

use deadpool_postgres::{Config, ManagerConfig, RecyclingMethod, Runtime};
use fraiseql_core::compiler::fact_table::{DatabaseIntrospector, FactTableDetector, SqlType};
use fraiseql_core::db::postgres::PostgresIntrospector;
use tokio_postgres::NoTls;

const TEST_DB_URL: &str =
    "postgresql://fraiseql_test:fraiseql_test_password@localhost:5433/test_fraiseql";

// Helper to create test introspector
async fn create_test_introspector() -> PostgresIntrospector {
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

// ============================================================================
// Fact Table Detection Tests
// ============================================================================

#[tokio::test]
#[ignore = "Requires PostgreSQL Docker container with analytics schema"]
async fn test_detect_tf_sales() {
    let introspector = create_test_introspector().await;

    let metadata = FactTableDetector::introspect(&introspector, "tf_sales")
        .await
        .expect("Failed to introspect tf_sales");

    // Verify table name
    assert_eq!(metadata.table_name, "tf_sales");

    // Verify measures (revenue, quantity, cost, discount)
    assert_eq!(metadata.measures.len(), 4, "Expected 4 measures");
    let measure_names: Vec<String> = metadata.measures.iter().map(|m| m.name.clone()).collect();
    assert!(measure_names.contains(&"revenue".to_string()));
    assert!(measure_names.contains(&"quantity".to_string()));
    assert!(measure_names.contains(&"cost".to_string()));
    assert!(measure_names.contains(&"discount".to_string()));

    // Verify dimension column (data)
    assert_eq!(metadata.dimensions.name, "data");

    // Verify denormalized filters (customer_id, product_id, occurred_at, created_at)
    assert!(
        metadata.denormalized_filters.len() >= 3,
        "Expected at least 3 filters"
    );
    let filter_names: Vec<String> = metadata
        .denormalized_filters
        .iter()
        .map(|f| f.name.clone())
        .collect();
    assert!(filter_names.contains(&"customer_id".to_string()));
    assert!(filter_names.contains(&"product_id".to_string()));
    assert!(filter_names.contains(&"occurred_at".to_string()));

    // Verify some filters are indexed
    let indexed_filters: Vec<&str> = metadata
        .denormalized_filters
        .iter()
        .filter(|f| f.indexed)
        .map(|f| f.name.as_str())
        .collect();
    assert!(indexed_filters.contains(&"customer_id"));
    assert!(indexed_filters.contains(&"product_id"));
    assert!(indexed_filters.contains(&"occurred_at"));
}

#[tokio::test]
#[ignore = "Requires PostgreSQL Docker container with analytics schema"]
async fn test_detect_tf_events() {
    let introspector = create_test_introspector().await;

    let metadata = FactTableDetector::introspect(&introspector, "tf_events")
        .await
        .expect("Failed to introspect tf_events");

    // Verify table name
    assert_eq!(metadata.table_name, "tf_events");

    // Verify measures - the detector finds all numeric columns as measures
    // tf_events has: id (bigint), duration_ms (bigint), error_count (int),
    // request_size (bigint), response_size (bigint), status_code (int) = 6 numeric columns
    // But status_code is indexed so it becomes a filter, leaving 5 measures
    assert!(
        metadata.measures.len() >= 4,
        "Expected at least 4 measures, got {}",
        metadata.measures.len()
    );
    let measure_names: Vec<String> = metadata.measures.iter().map(|m| m.name.clone()).collect();
    assert!(measure_names.contains(&"duration_ms".to_string()));
    assert!(measure_names.contains(&"error_count".to_string()));
    assert!(measure_names.contains(&"request_size".to_string()));
    assert!(measure_names.contains(&"response_size".to_string()));

    // Verify dimension column
    assert_eq!(metadata.dimensions.name, "data");

    // Verify denormalized filters
    let filter_names: Vec<String> = metadata
        .denormalized_filters
        .iter()
        .map(|f| f.name.clone())
        .collect();
    assert!(filter_names.contains(&"endpoint".to_string()));
    assert!(filter_names.contains(&"occurred_at".to_string()));
}

// ============================================================================
// Non-Fact Table Rejection Tests
// ============================================================================

#[tokio::test]
#[ignore = "Requires PostgreSQL Docker container with analytics schema"]
async fn test_reject_aggregate_table() {
    let introspector = create_test_introspector().await;

    // ta_sales_by_day should be rejected (not a fact table - no tf_ prefix)
    let result = FactTableDetector::introspect(&introspector, "ta_sales_by_day").await;

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        err.to_string().contains("not a fact table")
            || err.to_string().contains("tf_")
            || err.to_string().contains("prefix"),
        "Expected error about not being a fact table, got: {}",
        err
    );
}

#[tokio::test]
#[ignore = "Requires PostgreSQL Docker container with analytics schema"]
async fn test_reject_view() {
    let introspector = create_test_introspector().await;

    // v_user is a view, not a fact table
    let result = FactTableDetector::introspect(&introspector, "v_user").await;

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        err.to_string().contains("not a fact table")
            || err.to_string().contains("tf_")
            || err.to_string().contains("prefix"),
        "Expected error about not being a fact table, got: {}",
        err
    );
}

#[tokio::test]
#[ignore = "Requires PostgreSQL Docker container with analytics schema"]
async fn test_reject_nonexistent_table() {
    let introspector = create_test_introspector().await;

    let result = FactTableDetector::introspect(&introspector, "tf_nonexistent").await;

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        err.to_string().contains("not found")
            || err.to_string().contains("no columns")
            || err.to_string().contains("does not exist"),
        "Expected error about table not found, got: {}",
        err
    );
}

// ============================================================================
// Validation Tests
// ============================================================================

#[tokio::test]
#[ignore = "Requires PostgreSQL Docker container with analytics schema"]
async fn test_measure_types() {
    let introspector = create_test_introspector().await;

    let metadata = FactTableDetector::introspect(&introspector, "tf_sales")
        .await
        .expect("Failed to introspect tf_sales");

    // Verify all measures are numeric types
    for measure in &metadata.measures {
        assert!(
            matches!(
                measure.sql_type,
                SqlType::Int | SqlType::BigInt | SqlType::Decimal | SqlType::Float
            ),
            "Measure {} has non-numeric type: {:?}",
            measure.name,
            measure.sql_type
        );
    }
}

#[tokio::test]
#[ignore = "Requires PostgreSQL Docker container with analytics schema"]
async fn test_index_detection() {
    let introspector = create_test_introspector().await;

    let metadata = FactTableDetector::introspect(&introspector, "tf_sales")
        .await
        .expect("Failed to introspect tf_sales");

    // Count indexed filters
    let indexed_count = metadata
        .denormalized_filters
        .iter()
        .filter(|f| f.indexed)
        .count();

    // Should have at least 3 indexed columns (customer_id, product_id, occurred_at)
    assert!(
        indexed_count >= 3,
        "Expected at least 3 indexed columns, got {}",
        indexed_count
    );
}

// ============================================================================
// Database Type Detection Tests
// ============================================================================

#[tokio::test]
#[ignore = "Requires PostgreSQL Docker container with analytics schema"]
async fn test_sql_type_detection() {
    let introspector = create_test_introspector().await;

    let metadata = FactTableDetector::introspect(&introspector, "tf_sales")
        .await
        .expect("Failed to introspect tf_sales");

    // Find specific measures and check their types
    let revenue = metadata
        .measures
        .iter()
        .find(|m| m.name == "revenue")
        .expect("revenue not found");
    assert_eq!(revenue.sql_type, SqlType::Decimal);

    let quantity = metadata
        .measures
        .iter()
        .find(|m| m.name == "quantity")
        .expect("quantity not found");
    assert_eq!(quantity.sql_type, SqlType::Int);

    // Find dimension column
    assert_eq!(metadata.dimensions.name, "data");

    // Find UUID filter
    let customer_id = metadata
        .denormalized_filters
        .iter()
        .find(|f| f.name == "customer_id")
        .expect("customer_id not found");
    assert_eq!(customer_id.sql_type, SqlType::Uuid);
}

// ============================================================================
// Introspector Low-Level Tests
// ============================================================================

#[tokio::test]
#[ignore = "Requires PostgreSQL Docker container with analytics schema"]
async fn test_get_columns_tf_sales() {
    let introspector = create_test_introspector().await;

    let columns = introspector
        .get_columns("tf_sales")
        .await
        .expect("Failed to get columns");

    // Should have: id, revenue, quantity, cost, discount, data, customer_id, product_id, occurred_at, created_at
    assert!(
        columns.len() >= 10,
        "Expected at least 10 columns, got {}",
        columns.len()
    );

    // Check for key columns
    let column_names: Vec<String> = columns.iter().map(|(name, _, _)| name.clone()).collect();
    assert!(column_names.contains(&"revenue".to_string()));
    assert!(column_names.contains(&"quantity".to_string()));
    assert!(column_names.contains(&"data".to_string()));
    assert!(column_names.contains(&"customer_id".to_string()));
}

#[tokio::test]
#[ignore = "Requires PostgreSQL Docker container with analytics schema"]
async fn test_get_indexed_columns_tf_sales() {
    let introspector = create_test_introspector().await;

    let indexed = introspector
        .get_indexed_columns("tf_sales")
        .await
        .expect("Failed to get indexed columns");

    // Should have indexes on: id (PK), customer_id, product_id, occurred_at, data (GIN)
    assert!(
        indexed.len() >= 4,
        "Expected at least 4 indexed columns, got {}",
        indexed.len()
    );

    assert!(indexed.contains(&"customer_id".to_string()));
    assert!(indexed.contains(&"product_id".to_string()));
    assert!(indexed.contains(&"occurred_at".to_string()));
}
