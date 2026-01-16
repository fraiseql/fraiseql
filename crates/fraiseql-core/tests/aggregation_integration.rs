//! Integration tests for aggregation queries.
//!
//! These tests verify the end-to-end aggregation pipeline:
//! - JSON query parsing
//! - Execution plan generation
//! - SQL generation
//! - Result projection
//!
//! To run:
//!   cargo test -p fraiseql-core --test aggregation_integration
//!
//! For database integration tests:
//!   1. Start test database: docker compose -f docker-compose.test.yml up -d
//!   2. Run tests: cargo test -p fraiseql-core --test aggregation_integration -- --ignored

use fraiseql_core::compiler::aggregate_types::{AggregateFunction, TemporalBucket};
use fraiseql_core::compiler::aggregation::{
    AggregateSelection, AggregationRequest, GroupBySelection,
};
use fraiseql_core::compiler::fact_table::{
    CalendarBucket, CalendarDimension, CalendarGranularity, DimensionColumn, DimensionPath,
    FactTableMetadata, FilterColumn, MeasureColumn, SqlType,
};
use fraiseql_core::runtime::{AggregateQueryParser, AggregationProjector, AggregationSqlGenerator};
use serde_json::json;

/// Helper to create test fact table metadata for tf_sales
fn create_test_metadata() -> FactTableMetadata {
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
                    name: "product".to_string(),
                    json_path: "data->>'product'".to_string(),
                    data_type: "text".to_string(),
                },
            ],
        },
        denormalized_filters: vec![FilterColumn {
            name: "occurred_at".to_string(),
            sql_type: SqlType::Timestamp,
            indexed: true,
        }],
        calendar_dimensions: vec![CalendarDimension {
            source_column: "occurred_at".to_string(),
            granularities: vec![CalendarGranularity {
                column_name: "date_info".to_string(),
                buckets: vec![
                    CalendarBucket {
                        json_key: "day".to_string(),
                        bucket_type: TemporalBucket::Day,
                        data_type: "date".to_string(),
                    },
                    CalendarBucket {
                        json_key: "month".to_string(),
                        bucket_type: TemporalBucket::Month,
                        data_type: "integer".to_string(),
                    },
                ],
            }],
        }],
    }
}

// ============================================================================
// Query Parsing Tests
// ============================================================================

#[test]
fn test_parse_simple_aggregate_query() {
    let metadata = create_test_metadata();
    let query = json!({
        "table": "tf_sales",
        "aggregates": [
            {"count": {}}
        ]
    });

    let request = AggregateQueryParser::parse(&query, &metadata).unwrap();

    assert_eq!(request.table_name, "tf_sales");
    assert_eq!(request.aggregates.len(), 1);
    assert_eq!(request.aggregates[0].alias(), "count");
}

#[test]
fn test_parse_group_by_with_aggregates() {
    let metadata = create_test_metadata();
    let query = json!({
        "table": "tf_sales",
        "groupBy": {
            "category": true,
            "occurred_at_day": true
        },
        "aggregates": [
            {"count": {}},
            {"revenue_sum": {}},
            {"revenue_avg": {}}
        ]
    });

    let request = AggregateQueryParser::parse(&query, &metadata).unwrap();

    assert_eq!(request.group_by.len(), 2);
    assert_eq!(request.aggregates.len(), 3);

    // Verify GROUP BY selections
    match &request.group_by[0] {
        GroupBySelection::Dimension { path, alias } => {
            assert_eq!(path, "category");
            assert_eq!(alias, "category");
        }
        _ => panic!("Expected Dimension selection"),
    }

    // Parser prefers CalendarDimension over TemporalBucket when calendar dimensions are defined
    match &request.group_by[1] {
        GroupBySelection::CalendarDimension {
            source_column,
            calendar_column,
            json_key,
            bucket,
            alias,
        } => {
            assert_eq!(source_column, "occurred_at");
            assert_eq!(calendar_column, "date_info");
            assert_eq!(json_key, "day");
            assert_eq!(*bucket, TemporalBucket::Day);
            assert_eq!(alias, "occurred_at_day");
        }
        GroupBySelection::TemporalBucket {
            column,
            bucket,
            alias,
        } => {
            // Fallback if no calendar dimension (shouldn't happen in this test)
            assert_eq!(column, "occurred_at");
            assert_eq!(*bucket, TemporalBucket::Day);
            assert_eq!(alias, "occurred_at_day");
        }
        _ => panic!("Expected CalendarDimension or TemporalBucket selection"),
    }
}

// ============================================================================
// SQL Generation Tests
// ============================================================================

#[test]
fn test_sql_generation_postgres() {
    use fraiseql_core::compiler::aggregation::AggregationPlanner;
    use fraiseql_core::db::types::DatabaseType;

    let metadata = create_test_metadata();
    let request = AggregationRequest {
        table_name: "tf_sales".to_string(),
        where_clause: None,
        group_by: vec![GroupBySelection::Dimension {
            path: "category".to_string(),
            alias: "category".to_string(),
        }],
        aggregates: vec![
            AggregateSelection::Count {
                alias: "count".to_string(),
            },
            AggregateSelection::MeasureAggregate {
                measure: "revenue".to_string(),
                function: AggregateFunction::Sum,
                alias: "revenue_sum".to_string(),
            },
        ],
        having: vec![],
        order_by: vec![],
        limit: None,
        offset: None,
    };

    // Generate execution plan
    let plan = AggregationPlanner::plan(request, metadata).unwrap();

    // Generate SQL
    let sql_generator = AggregationSqlGenerator::new(DatabaseType::PostgreSQL);
    let sql = sql_generator.generate(&plan).unwrap();

    // Verify SQL contains expected clauses
    assert!(sql.complete_sql.contains("data->>'category'"));
    assert!(sql.complete_sql.contains("COUNT(*)"));
    assert!(sql.complete_sql.contains("SUM(revenue)"));
    assert!(sql.complete_sql.contains("GROUP BY"));
    assert!(sql.complete_sql.contains("FROM tf_sales"));
}

#[test]
fn test_temporal_bucket_sql_generation() {
    use fraiseql_core::compiler::aggregation::AggregationPlanner;
    use fraiseql_core::db::types::DatabaseType;

    let metadata = create_test_metadata();
    let request = AggregationRequest {
        table_name: "tf_sales".to_string(),
        where_clause: None,
        group_by: vec![GroupBySelection::TemporalBucket {
            column: "occurred_at".to_string(),
            bucket: TemporalBucket::Day,
            alias: "day".to_string(),
        }],
        aggregates: vec![AggregateSelection::Count {
            alias: "count".to_string(),
        }],
        having: vec![],
        order_by: vec![],
        limit: None,
        offset: None,
    };

    let plan = AggregationPlanner::plan(request, metadata).unwrap();

    // Test PostgreSQL SQL generation
    let pg_generator = AggregationSqlGenerator::new(DatabaseType::PostgreSQL);
    let pg_sql = pg_generator.generate(&plan).unwrap();
    assert!(pg_sql.complete_sql.contains("DATE_TRUNC('day', occurred_at)"));

    // Test MySQL SQL generation
    let mysql_generator = AggregationSqlGenerator::new(DatabaseType::MySQL);
    let mysql_sql = mysql_generator.generate(&plan).unwrap();
    assert!(mysql_sql.complete_sql.contains("DATE_FORMAT(occurred_at"));

    // Test SQLite SQL generation
    let sqlite_generator = AggregationSqlGenerator::new(DatabaseType::SQLite);
    let sqlite_sql = sqlite_generator.generate(&plan).unwrap();
    assert!(sqlite_sql.complete_sql.contains("strftime"));

    // Test SQL Server SQL generation
    let sqlserver_generator = AggregationSqlGenerator::new(DatabaseType::SQLServer);
    let sqlserver_sql = sqlserver_generator.generate(&plan).unwrap();
    assert!(sqlserver_sql.complete_sql.contains("CAST(occurred_at AS DATE)"));
}

// ============================================================================
// Result Projection Tests
// ============================================================================

#[test]
fn test_result_projection() {
    use fraiseql_core::compiler::aggregation::{AggregateExpression, AggregationPlan, GroupByExpression};
    use std::collections::HashMap;

    let metadata = create_test_metadata();
    let request = AggregationRequest {
        table_name: "tf_sales".to_string(),
        where_clause: None,
        group_by: vec![GroupBySelection::Dimension {
            path: "category".to_string(),
            alias: "category".to_string(),
        }],
        aggregates: vec![AggregateSelection::Count {
            alias: "count".to_string(),
        }],
        having: vec![],
        order_by: vec![],
        limit: None,
        offset: None,
    };

    let plan = AggregationPlan {
        metadata,
        request,
        group_by_expressions: vec![GroupByExpression::JsonbPath {
            jsonb_column: "data".to_string(),
            path: "category".to_string(),
            alias: "category".to_string(),
        }],
        aggregate_expressions: vec![AggregateExpression::Count {
            alias: "count".to_string(),
        }],
        having_conditions: vec![],
    };

    // Mock SQL results
    let rows = vec![
        {
            let mut row = HashMap::new();
            row.insert("category".to_string(), json!("Electronics"));
            row.insert("count".to_string(), json!(42));
            row
        },
        {
            let mut row = HashMap::new();
            row.insert("category".to_string(), json!("Books"));
            row.insert("count".to_string(), json!(15));
            row
        },
    ];

    // Project results
    let projected = AggregationProjector::project(rows, &plan).unwrap();

    // Verify projection
    assert!(projected.is_array());
    let arr = projected.as_array().unwrap();
    assert_eq!(arr.len(), 2);
    assert_eq!(arr[0]["category"], "Electronics");
    assert_eq!(arr[0]["count"], 42);
    assert_eq!(arr[1]["category"], "Books");
    assert_eq!(arr[1]["count"], 15);
}

#[test]
fn test_wrap_in_graphql_envelope() {
    let projected = json!([
        {"category": "Electronics", "count": 42}
    ]);

    let response = AggregationProjector::wrap_in_data_envelope(projected, "sales_aggregate");

    assert!(response.get("data").is_some());
    assert!(response["data"].get("sales_aggregate").is_some());
    assert_eq!(
        response["data"]["sales_aggregate"][0]["category"],
        "Electronics"
    );
}

// ============================================================================
// Database Integration Tests (ignored by default)
// ============================================================================

#[tokio::test]
#[ignore = "Requires PostgreSQL Docker container with analytics schema"]
async fn test_end_to_end_aggregate_query() {
    use fraiseql_core::db::postgres::PostgresAdapter;
    use fraiseql_core::runtime::Executor;
    use fraiseql_core::schema::CompiledSchema;
    use std::sync::Arc;

    const TEST_DB_URL: &str =
        "postgresql://fraiseql_test:fraiseql_test_password@localhost:5433/test_fraiseql";

    // Setup test database
    let adapter = Arc::new(
        PostgresAdapter::new(TEST_DB_URL)
            .await
            .expect("Failed to connect to test database"),
    );

    // Create test schema
    let schema = CompiledSchema::new();
    let executor = Executor::new(schema, adapter);

    // Create test fact table metadata
    let metadata = create_test_metadata();

    // Create aggregate query
    let query_json = json!({
        "table": "tf_sales",
        "groupBy": {
            "category": true
        },
        "aggregates": [
            {"count": {}},
            {"revenue_sum": {}}
        ],
        "limit": 10
    });

    // Execute aggregate query
    let result = executor
        .execute_aggregate_query(&query_json, "sales_aggregate", &metadata)
        .await
        .expect("Failed to execute aggregate query");

    // Parse response
    let response: serde_json::Value = serde_json::from_str(&result).unwrap();

    // Verify response structure
    assert!(response.get("data").is_some());
    assert!(response["data"].get("sales_aggregate").is_some());
    assert!(response["data"]["sales_aggregate"].is_array());

    println!(
        "Aggregate query result: {}",
        serde_json::to_string_pretty(&response).unwrap()
    );
}
