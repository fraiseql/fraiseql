//! Integration tests for fact table introspection.
//!
//! These tests require a running PostgreSQL database with test data.
//!
//! To run:
//!   1. Start test database: docker compose -f docker-compose.test.yml up -d
//!   2. Wait for DB: docker compose -f docker-compose.test.yml exec postgres-test pg_isready
//!   3. Run tests: cargo test compiler::fact_table -- --ignored --nocapture
//!   4. Stop database: docker compose -f docker-compose.test.yml down

use fraiseql_core::compiler::fact_table::{FactTableDetector, DatabaseIntrospector};
use fraiseql_core::db::postgres::PostgresIntrospector;

const TEST_DB_URL: &str = "postgresql://fraiseql_test:fraiseql_test_password@localhost:5433/test_fraiseql";

// Helper to create test introspector
async fn create_test_introspector() -> PostgresIntrospector {
    use deadpool_postgres::{Config, ManagerConfig, Pool, RecyclingMethod, Runtime};
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

// ============================================================================
// Fact Table Detection Tests
// ============================================================================

#[tokio::test]
#[ignore]
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
    assert!(metadata.denormalized_filters.len() >= 3, "Expected at least 3 filters");
    let filter_names: Vec<String> = metadata.denormalized_filters.iter().map(|f| f.name.clone()).collect();
    assert!(filter_names.contains(&"customer_id".to_string()));
    assert!(filter_names.contains(&"product_id".to_string()));
    assert!(filter_names.contains(&"occurred_at".to_string()));

    // Verify some filters are indexed
    let indexed_filters: Vec<&str> = metadata.denormalized_filters
        .iter()
        .filter(|f| f.indexed)
        .map(|f| f.name.as_str())
        .collect();
    assert!(indexed_filters.contains(&"customer_id"));
    assert!(indexed_filters.contains(&"product_id"));
    assert!(indexed_filters.contains(&"occurred_at"));
}

#[tokio::test]
#[ignore]
async fn test_detect_tf_events() {
    let introspector = create_test_introspector().await;

    let metadata = FactTableDetector::introspect(&introspector, "tf_events")
        .await
        .expect("Failed to introspect tf_events");

    // Verify table name
    assert_eq!(metadata.table_name, "tf_events");

    // Verify measures (duration_ms, error_count, request_size, response_size)
    assert_eq!(metadata.measures.len(), 4, "Expected 4 measures");
    let measure_names: Vec<String> = metadata.measures.iter().map(|m| m.name.clone()).collect();
    assert!(measure_names.contains(&"duration_ms".to_string()));
    assert!(measure_names.contains(&"error_count".to_string()));
    assert!(measure_names.contains(&"request_size".to_string()));
    assert!(measure_names.contains(&"response_size".to_string()));

    // Verify dimension column
    assert_eq!(metadata.dimensions.name, "data");

    // Verify denormalized filters
    let filter_names: Vec<String> = metadata.denormalized_filters.iter().map(|f| f.name.clone()).collect();
    assert!(filter_names.contains(&"endpoint".to_string()));
    assert!(filter_names.contains(&"status_code".to_string()));
    assert!(filter_names.contains(&"occurred_at".to_string()));
}

// ============================================================================
// Non-Fact Table Rejection Tests
// ============================================================================

#[tokio::test]
#[ignore]
async fn test_reject_aggregate_table() {
    let introspector = create_test_introspector().await;

    // ta_sales_by_day should be rejected (not a fact table)
    let result = FactTableDetector::introspect(&introspector, "ta_sales_by_day").await;

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.to_string().contains("not a fact table"));
}

#[tokio::test]
#[ignore]
async fn test_reject_view() {
    let introspector = create_test_introspector().await;

    // v_user is a view, not a fact table
    let result = FactTableDetector::introspect(&introspector, "v_user").await;

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.to_string().contains("not a fact table"));
}

#[tokio::test]
#[ignore]
async fn test_reject_nonexistent_table() {
    let introspector = create_test_introspector().await;

    let result = FactTableDetector::introspect(&introspector, "tf_nonexistent").await;

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.to_string().contains("not found") || err.to_string().contains("no columns"));
}

// ============================================================================
// Validation Tests
// ============================================================================

#[tokio::test]
#[ignore]
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
                fraiseql_core::compiler::fact_table::SqlType::Int
                    | fraiseql_core::compiler::fact_table::SqlType::BigInt
                    | fraiseql_core::compiler::fact_table::SqlType::Decimal
                    | fraiseql_core::compiler::fact_table::SqlType::Float
            ),
            "Measure {} has non-numeric type: {:?}",
            measure.name,
            measure.sql_type
        );
    }
}

#[tokio::test]
#[ignore]
async fn test_index_detection() {
    let introspector = create_test_introspector().await;

    let metadata = FactTableDetector::introspect(&introspector, "tf_sales")
        .await
        .expect("Failed to introspect tf_sales");

    // Count indexed filters
    let indexed_count = metadata.denormalized_filters.iter().filter(|f| f.indexed).count();

    // Should have at least 3 indexed columns (customer_id, product_id, occurred_at)
    assert!(indexed_count >= 3, "Expected at least 3 indexed columns, got {}", indexed_count);
}

// ============================================================================
// Database Type Detection Tests
// ============================================================================

#[tokio::test]
#[ignore]
async fn test_sql_type_detection() {
    let introspector = create_test_introspector().await;

    let metadata = FactTableDetector::introspect(&introspector, "tf_sales")
        .await
        .expect("Failed to introspect tf_sales");

    // Find specific measures and check their types
    let revenue = metadata.measures.iter().find(|m| m.name == "revenue").expect("revenue not found");
    assert_eq!(revenue.sql_type, fraiseql_core::compiler::fact_table::SqlType::Decimal);

    let quantity = metadata.measures.iter().find(|m| m.name == "quantity").expect("quantity not found");
    assert_eq!(quantity.sql_type, fraiseql_core::compiler::fact_table::SqlType::Int);

    // Find dimension column
    assert_eq!(metadata.dimensions.name, "data");

    // Find UUID filter
    let customer_id = metadata.denormalized_filters.iter().find(|f| f.name == "customer_id").expect("customer_id not found");
    assert_eq!(customer_id.sql_type, fraiseql_core::compiler::fact_table::SqlType::Uuid);
}
