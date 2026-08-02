//! Integration tests for fact table introspection.
//!
//! These tests require a running PostgreSQL database with analytics test data.
//!
//! To run:
//!   1. Start test database: docker compose -f docker-compose.test.yml up -d
//!   2. Wait for DB: docker compose -f docker-compose.test.yml exec postgres-test `pg_isready`
//!   3. Run tests: cargo test -p fraiseql-core --features test-postgres --test
//!      `fact_table_integration`
//!   4. Stop database: docker compose -f docker-compose.test.yml down

#![cfg(feature = "test-postgres")]
#![allow(clippy::unwrap_used)]
#![allow(clippy::print_stdout, clippy::print_stderr)] // Reason: CLI / test / example / bench code prints to stdout/stderr by design

use deadpool_postgres::{Config, ManagerConfig, RecyclingMethod, Runtime};
use fraiseql_core::{
    compiler::fact_table::{DatabaseIntrospector, FactTableDetector, SqlType},
    db::postgres::PostgresIntrospector,
};
use tokio_postgres::NoTls;

// Helper to create a test introspector against the harness-provided Postgres.
// Returns None when no database is available (caller skips). The caller keeps the
// returned Service alive so a locally spawned container outlives the test.
async fn create_test_introspector() -> Option<(fraiseql_test_support::Service, PostgresIntrospector)>
{
    let pg = fraiseql_test_support::postgres().await?;
    let mut cfg = Config::new();
    cfg.url = Some(pg.url().to_string());
    cfg.manager = Some(ManagerConfig {
        recycling_method: RecyclingMethod::Fast,
    });
    cfg.pool = Some(deadpool_postgres::PoolConfig::new(10));

    let pool = cfg.create_pool(Some(Runtime::Tokio1), NoTls).expect("Failed to create pool");

    Some((pg, PostgresIntrospector::new(pool)))
}

// ============================================================================
// Fact Table Detection Tests
// ============================================================================

#[tokio::test]
async fn test_detect_tf_sales() {
    let Some((_pg, introspector)) = create_test_introspector().await else {
        eprintln!("SKIP fact_table integration: no postgres (set DATABASE_URL)");
        return;
    };

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
    let filter_names: Vec<String> =
        metadata.denormalized_filters.iter().map(|f| f.name.clone()).collect();
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
async fn test_detect_tf_events() {
    let Some((_pg, introspector)) = create_test_introspector().await else {
        eprintln!("SKIP fact_table integration: no postgres (set DATABASE_URL)");
        return;
    };

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
    let filter_names: Vec<String> =
        metadata.denormalized_filters.iter().map(|f| f.name.clone()).collect();
    assert!(filter_names.contains(&"endpoint".to_string()));
    assert!(filter_names.contains(&"occurred_at".to_string()));
}

// ============================================================================
// Non-Fact Table Rejection Tests
// ============================================================================

#[tokio::test]
async fn test_reject_aggregate_table() {
    let Some((_pg, introspector)) = create_test_introspector().await else {
        eprintln!("SKIP fact_table integration: no postgres (set DATABASE_URL)");
        return;
    };

    // ta_sales_by_day should be rejected (not a fact table - no tf_ prefix)
    let result = FactTableDetector::introspect(&introspector, "ta_sales_by_day").await;

    assert!(
        result.is_err(),
        "expected Err rejecting non-fact table ta_sales_by_day, got: {result:?}"
    );
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
async fn test_reject_view() {
    let Some((_pg, introspector)) = create_test_introspector().await else {
        eprintln!("SKIP fact_table integration: no postgres (set DATABASE_URL)");
        return;
    };

    // v_user is a view, not a fact table
    let result = FactTableDetector::introspect(&introspector, "v_user").await;

    assert!(
        result.is_err(),
        "expected Err rejecting view v_user as fact table, got: {result:?}"
    );
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
async fn test_reject_nonexistent_table() {
    let Some((_pg, introspector)) = create_test_introspector().await else {
        eprintln!("SKIP fact_table integration: no postgres (set DATABASE_URL)");
        return;
    };

    let result = FactTableDetector::introspect(&introspector, "tf_nonexistent").await;

    assert!(
        result.is_err(),
        "expected Err for nonexistent table tf_nonexistent, got: {result:?}"
    );
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
async fn test_measure_types() {
    let Some((_pg, introspector)) = create_test_introspector().await else {
        eprintln!("SKIP fact_table integration: no postgres (set DATABASE_URL)");
        return;
    };

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
async fn test_index_detection() {
    let Some((_pg, introspector)) = create_test_introspector().await else {
        eprintln!("SKIP fact_table integration: no postgres (set DATABASE_URL)");
        return;
    };

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
async fn test_sql_type_detection() {
    let Some((_pg, introspector)) = create_test_introspector().await else {
        eprintln!("SKIP fact_table integration: no postgres (set DATABASE_URL)");
        return;
    };

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
async fn test_get_columns_tf_sales() {
    let Some((_pg, introspector)) = create_test_introspector().await else {
        eprintln!("SKIP fact_table integration: no postgres (set DATABASE_URL)");
        return;
    };

    let columns = introspector.get_columns("tf_sales").await.expect("Failed to get columns");

    // Should have: id, revenue, quantity, cost, discount, data, customer_id, product_id,
    // occurred_at, created_at
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
    let Some((_pg, introspector)) = create_test_introspector().await else {
        eprintln!("SKIP fact_table integration: no postgres (set DATABASE_URL)");
        return;
    };

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

/// #825 — the documented calendar-dimension layout (docs/modules/fact-table.md):
/// the real dimensions column (`data`) followed by `*_info` JSONB calendar
/// columns. Introspection picked the LAST JSONB column by ordinal position, so
/// every calendar fact table got `year_info` as its dimensions column,
/// `introspect facts` printed it for the developer to paste, and
/// `validate-facts` hard-errored on a correct schema.
#[tokio::test]
async fn test_calendar_fact_table_dimensions_by_role() {
    let Some((pg, introspector)) = create_test_introspector().await else {
        eprintln!("SKIP fact_table integration: no postgres (set DATABASE_URL)");
        return;
    };
    let (client, connection) = tokio_postgres::connect(pg.url(), NoTls).await.unwrap();
    tokio::spawn(async move {
        let _ = connection.await;
    });

    client
        .batch_execute(
            "DROP TABLE IF EXISTS tf_p23_calendar_sales;
             CREATE TABLE tf_p23_calendar_sales (
                 id SERIAL PRIMARY KEY,
                 revenue NUMERIC(12,2) NOT NULL,
                 quantity INTEGER NOT NULL,
                 data JSONB,
                 date_info JSONB NOT NULL,
                 month_info JSONB NOT NULL,
                 quarter_info JSONB NOT NULL,
                 year_info JSONB NOT NULL,
                 customer_id INTEGER NOT NULL,
                 occurred_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
             );
             INSERT INTO tf_p23_calendar_sales
                 (revenue, quantity, data, date_info, month_info, quarter_info, year_info, customer_id)
             VALUES (100.0, 2,
                 '{\"region\": \"EU\", \"channel\": \"web\"}',
                 '{\"date\": \"2026-08-01\", \"week\": 31, \"month\": 8, \"quarter\": 3, \"year\": 2026}',
                 '{\"month\": 8, \"quarter\": 3, \"year\": 2026}',
                 '{\"quarter\": 3, \"year\": 2026}',
                 '{\"year\": 2026}', 7);",
        )
        .await
        .expect("create calendar fact table");

    let metadata = FactTableDetector::introspect(&introspector, "tf_p23_calendar_sales")
        .await
        .expect("calendar layout must introspect");

    assert_eq!(
        metadata.dimensions.name, "data",
        "the dimensions column is `data` — the *_info calendar columns are claimed by \
         calendar detection and must never win by ordinal position (#825)"
    );
    let path_names: Vec<&str> = metadata.dimensions.paths.iter().map(|p| p.name.as_str()).collect();
    assert!(
        path_names.contains(&"region") && path_names.contains(&"channel"),
        "dimension paths must come from the data column's sample, got: {path_names:?}"
    );
    assert!(
        !metadata.calendar_dimensions.is_empty(),
        "the *_info columns must still be detected as calendar dimensions"
    );

    // #825 secondary — a non-indexed numeric `*_id` column must surface as an
    // unindexed filter, not silently vanish from the metadata.
    let customer = metadata
        .denormalized_filters
        .iter()
        .find(|f| f.name == "customer_id")
        .expect("customer_id must be recorded as a filter even without an index (#825)");
    assert!(!customer.indexed, "customer_id has no index in this fixture");

    client.batch_execute("DROP TABLE IF EXISTS tf_p23_calendar_sales;").await.ok();
}
