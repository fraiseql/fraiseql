#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
use super::*;

#[test]
fn test_is_fact_table() {
    assert!(FactTableDetector::is_fact_table("tf_sales"));
    assert!(FactTableDetector::is_fact_table("tf_events"));
    assert!(FactTableDetector::is_fact_table("tf_api_requests"));
    assert!(!FactTableDetector::is_fact_table("ta_sales_by_day"));
    assert!(!FactTableDetector::is_fact_table("td_products"));
    assert!(!FactTableDetector::is_fact_table("v_user"));
    assert!(!FactTableDetector::is_fact_table("tb_user"));
}

#[test]
fn test_validate_valid_fact_table() {
    let metadata = FactTableMetadata {
        table_name: "tf_sales".to_string(),
        measures: vec![MeasureColumn {
            name: "revenue".to_string(),
            sql_type: SqlType::Decimal,
            nullable: false,
        }],
        dimensions: DimensionColumn {
            name: "dimensions".to_string(),
            paths: vec![],
        },
        denormalized_filters: vec![],
        calendar_dimensions: vec![],
    };

    FactTableDetector::validate(&metadata)
        .unwrap_or_else(|e| panic!("expected Ok for valid fact table: {e}"));
}

#[test]
fn test_validate_missing_measures() {
    let metadata = FactTableMetadata {
        table_name: "tf_sales".to_string(),
        measures: vec![],
        dimensions: DimensionColumn {
            name: "dimensions".to_string(),
            paths: vec![],
        },
        denormalized_filters: vec![],
        calendar_dimensions: vec![],
    };

    let result = FactTableDetector::validate(&metadata);
    let err = result.expect_err("expected Err for missing measures");
    assert!(err.to_string().contains("at least one measure"), "unexpected error: {err}");
}

#[test]
fn test_validate_non_numeric_measure() {
    let metadata = FactTableMetadata {
        table_name: "tf_sales".to_string(),
        measures: vec![MeasureColumn {
            name: "category".to_string(),
            sql_type: SqlType::Text, // Wrong type for measure!
            nullable: false,
        }],
        dimensions: DimensionColumn {
            name: "dimensions".to_string(),
            paths: vec![],
        },
        denormalized_filters: vec![],
        calendar_dimensions: vec![],
    };

    let result = FactTableDetector::validate(&metadata);
    let err = result.expect_err("expected Err for non-numeric measure");
    assert!(err.to_string().contains("must be numeric"), "unexpected error: {err}");
}

#[test]
fn test_from_columns() {
    let columns = vec![
        ("id", SqlType::BigInt, false),
        ("revenue", SqlType::Decimal, false),
        ("quantity", SqlType::Int, false),
        ("dimensions", SqlType::Jsonb, false),
        ("customer_id", SqlType::Uuid, false),
        ("occurred_at", SqlType::Timestamp, false),
    ];

    let metadata = FactTableDetector::from_columns("tf_sales".to_string(), columns).unwrap();

    assert_eq!(metadata.measures.len(), 2);
    assert_eq!(metadata.measures[0].name, "revenue");
    assert_eq!(metadata.measures[1].name, "quantity");
    assert_eq!(metadata.dimensions.name, "dimensions");
    assert_eq!(metadata.denormalized_filters.len(), 2);
    assert_eq!(metadata.denormalized_filters[0].name, "customer_id");
    assert_eq!(metadata.denormalized_filters[1].name, "occurred_at");
}

#[test]
fn test_sql_type_from_str_postgres() {
    assert_eq!(SqlType::from_str_postgres("integer"), SqlType::Int);
    assert_eq!(SqlType::from_str_postgres("BIGINT"), SqlType::BigInt);
    assert_eq!(SqlType::from_str_postgres("decimal"), SqlType::Decimal);
    assert_eq!(SqlType::from_str_postgres("FLOAT"), SqlType::Float);
    assert_eq!(SqlType::from_str_postgres("jsonb"), SqlType::Jsonb);
    assert_eq!(SqlType::from_str_postgres("text"), SqlType::Text);
    assert_eq!(SqlType::from_str_postgres("uuid"), SqlType::Uuid);
    assert_eq!(SqlType::from_str_postgres("timestamptz"), SqlType::Timestamp);
}

#[test]
fn test_sql_type_from_str_mysql() {
    assert_eq!(SqlType::from_str_mysql("INT"), SqlType::Int);
    assert_eq!(SqlType::from_str_mysql("bigint"), SqlType::BigInt);
    assert_eq!(SqlType::from_str_mysql("DECIMAL"), SqlType::Decimal);
    assert_eq!(SqlType::from_str_mysql("double"), SqlType::Float);
    assert_eq!(SqlType::from_str_mysql("json"), SqlType::Json);
    assert_eq!(SqlType::from_str_mysql("VARCHAR"), SqlType::Text);
}

#[test]
fn test_sql_type_from_str_sqlite() {
    assert_eq!(SqlType::from_str_sqlite("INTEGER"), SqlType::BigInt);
    assert_eq!(SqlType::from_str_sqlite("real"), SqlType::Float);
    assert_eq!(SqlType::from_str_sqlite("TEXT"), SqlType::Text);
}

#[test]
fn test_sql_type_from_str_sqlserver() {
    assert_eq!(SqlType::from_str_sqlserver("INT"), SqlType::Int);
    assert_eq!(SqlType::from_str_sqlserver("BIGINT"), SqlType::BigInt);
    assert_eq!(SqlType::from_str_sqlserver("decimal"), SqlType::Decimal);
    assert_eq!(SqlType::from_str_sqlserver("float"), SqlType::Float);
    assert_eq!(SqlType::from_str_sqlserver("NVARCHAR"), SqlType::Text);
    assert_eq!(SqlType::from_str_sqlserver("uniqueidentifier"), SqlType::Uuid);
}

#[test]
fn test_is_numeric_type() {
    assert!(FactTableDetector::is_numeric_type(&SqlType::Int));
    assert!(FactTableDetector::is_numeric_type(&SqlType::BigInt));
    assert!(FactTableDetector::is_numeric_type(&SqlType::Decimal));
    assert!(FactTableDetector::is_numeric_type(&SqlType::Float));
    assert!(!FactTableDetector::is_numeric_type(&SqlType::Text));
    assert!(!FactTableDetector::is_numeric_type(&SqlType::Jsonb));
    assert!(!FactTableDetector::is_numeric_type(&SqlType::Uuid));
}

// =============================================================================
// Calendar Dimension Tests
// =============================================================================

#[test]
fn test_detect_calendar_dimensions() {
    let columns = vec![
        ("revenue".to_string(), "decimal".to_string(), false),
        ("data".to_string(), "jsonb".to_string(), false),
        ("date_info".to_string(), "jsonb".to_string(), false),
        ("month_info".to_string(), "jsonb".to_string(), false),
        ("occurred_at".to_string(), "timestamptz".to_string(), false),
    ];

    let indexed = std::collections::HashSet::new();
    let calendar_dims = FactTableDetector::detect_calendar_dimensions(&columns, &indexed).unwrap();

    assert_eq!(calendar_dims.len(), 1);
    assert_eq!(calendar_dims[0].source_column, "occurred_at");
    assert_eq!(calendar_dims[0].granularities.len(), 2); // date_info, month_info

    // Verify date_info buckets
    let date_info = &calendar_dims[0].granularities[0];
    assert_eq!(date_info.column_name, "date_info");
    assert_eq!(date_info.buckets.len(), 5); // day, week, month, quarter, year

    assert_eq!(date_info.buckets[0].json_key, "date");
    assert_eq!(
        date_info.buckets[0].bucket_type,
        crate::compiler::aggregate_types::TemporalBucket::Day
    );
    assert_eq!(date_info.buckets[0].data_type, "date");

    // Verify month_info buckets
    let month_info = &calendar_dims[0].granularities[1];
    assert_eq!(month_info.column_name, "month_info");
    assert_eq!(month_info.buckets.len(), 3); // month, quarter, year
}

#[test]
fn test_infer_calendar_buckets_date_info() {
    let buckets = FactTableDetector::infer_calendar_buckets("date_info");
    assert_eq!(buckets.len(), 5);

    assert_eq!(buckets[0].json_key, "date");
    assert_eq!(buckets[0].bucket_type, crate::compiler::aggregate_types::TemporalBucket::Day);

    assert_eq!(buckets[1].json_key, "week");
    assert_eq!(buckets[1].bucket_type, crate::compiler::aggregate_types::TemporalBucket::Week);

    assert_eq!(buckets[2].json_key, "month");
    assert_eq!(buckets[2].bucket_type, crate::compiler::aggregate_types::TemporalBucket::Month);

    assert_eq!(buckets[3].json_key, "quarter");
    assert_eq!(
        buckets[3].bucket_type,
        crate::compiler::aggregate_types::TemporalBucket::Quarter
    );

    assert_eq!(buckets[4].json_key, "year");
    assert_eq!(buckets[4].bucket_type, crate::compiler::aggregate_types::TemporalBucket::Year);
}

#[test]
fn test_infer_calendar_buckets_month_info() {
    let buckets = FactTableDetector::infer_calendar_buckets("month_info");
    assert_eq!(buckets.len(), 3);

    assert_eq!(buckets[0].json_key, "month");
    assert_eq!(buckets[1].json_key, "quarter");
    assert_eq!(buckets[2].json_key, "year");
}

#[test]
fn test_infer_calendar_buckets_year_info() {
    let buckets = FactTableDetector::infer_calendar_buckets("year_info");
    assert_eq!(buckets.len(), 1);

    assert_eq!(buckets[0].json_key, "year");
    assert_eq!(buckets[0].bucket_type, crate::compiler::aggregate_types::TemporalBucket::Year);
}

#[test]
fn test_infer_calendar_buckets_unknown() {
    let buckets = FactTableDetector::infer_calendar_buckets("unknown_info");
    assert_eq!(buckets.len(), 0);
}

#[test]
fn test_no_calendar_columns() {
    let columns = vec![
        ("revenue".to_string(), "decimal".to_string(), false),
        ("occurred_at".to_string(), "timestamptz".to_string(), false),
    ];

    let indexed = std::collections::HashSet::new();
    let calendar_dims = FactTableDetector::detect_calendar_dimensions(&columns, &indexed).unwrap();

    assert_eq!(calendar_dims.len(), 0); // No calendar columns detected
}

#[test]
fn test_calendar_detection_json_type() {
    // Test MySQL/SQLite JSON type (not just PostgreSQL JSONB)
    let columns = vec![
        ("revenue".to_string(), "decimal".to_string(), false),
        ("date_info".to_string(), "json".to_string(), false), // MySQL/SQLite
        ("occurred_at".to_string(), "timestamp".to_string(), false),
    ];

    let indexed = std::collections::HashSet::new();
    let calendar_dims = FactTableDetector::detect_calendar_dimensions(&columns, &indexed).unwrap();

    assert_eq!(calendar_dims.len(), 1);
    assert_eq!(calendar_dims[0].granularities.len(), 1); // date_info
    assert_eq!(calendar_dims[0].granularities[0].column_name, "date_info");
}

#[test]
fn test_single_date_info_column() {
    // Test that a single date_info column is detected and used
    let columns = vec![
        ("revenue".to_string(), "decimal".to_string(), false),
        ("data".to_string(), "jsonb".to_string(), false),
        ("date_info".to_string(), "jsonb".to_string(), false), // Only this calendar column
        ("occurred_at".to_string(), "timestamptz".to_string(), false),
    ];

    let indexed = std::collections::HashSet::new();
    let calendar_dims = FactTableDetector::detect_calendar_dimensions(&columns, &indexed).unwrap();

    assert_eq!(calendar_dims.len(), 1);
    assert_eq!(calendar_dims[0].source_column, "occurred_at");
    assert_eq!(calendar_dims[0].granularities.len(), 1); // Only date_info

    // Verify date_info provides all 5 buckets
    let date_info = &calendar_dims[0].granularities[0];
    assert_eq!(date_info.column_name, "date_info");
    assert_eq!(date_info.buckets.len(), 5); // day, week, month, quarter, year

    // Can query any of these buckets from the single date_info column
    assert_eq!(date_info.buckets[0].json_key, "date"); // day bucket
    assert_eq!(date_info.buckets[1].json_key, "week"); // week bucket
    assert_eq!(date_info.buckets[2].json_key, "month"); // month bucket
    assert_eq!(date_info.buckets[3].json_key, "quarter"); // quarter bucket
    assert_eq!(date_info.buckets[4].json_key, "year"); // year bucket
}

// =============================================================================
// Test Helpers
// =============================================================================

/// Helper to find a path by name, returning a proper error instead of panicking
fn find_path_by_name<'a>(paths: &'a [DimensionPath], name: &str) -> Option<&'a DimensionPath> {
    paths.iter().find(|p| p.name == name)
}

// =============================================================================
// Dimension Path Extraction Tests
// =============================================================================

#[test]
fn test_extract_dimension_paths_simple() {
    let sample = serde_json::json!({
        "category": "electronics",
        "region": "north",
        "priority": 1
    });

    let paths =
        FactTableDetector::extract_dimension_paths(&sample, "dimensions", DatabaseType::PostgreSQL);

    assert_eq!(paths.len(), 3);

    // Check category path
    let category = find_path_by_name(&paths, "category").expect("category path");
    assert_eq!(category.json_path, "dimensions->>'category'");
    assert_eq!(category.data_type, "string");

    // Check region path
    let region = find_path_by_name(&paths, "region").expect("region path");
    assert_eq!(region.json_path, "dimensions->>'region'");
    assert_eq!(region.data_type, "string");

    // Check priority path (integer)
    let priority = find_path_by_name(&paths, "priority").expect("priority path");
    assert_eq!(priority.json_path, "dimensions->>'priority'");
    assert_eq!(priority.data_type, "integer");
}

#[test]
fn test_extract_dimension_paths_nested() {
    let sample = serde_json::json!({
        "customer": {
            "region": "north",
            "tier": "gold"
        },
        "product": "laptop"
    });

    let paths =
        FactTableDetector::extract_dimension_paths(&sample, "data", DatabaseType::PostgreSQL);

    // Should have: customer (object), customer_region, customer_tier, product
    assert!(paths.iter().any(|p| p.name == "customer"));
    assert!(paths.iter().any(|p| p.name == "customer_region"));
    assert!(paths.iter().any(|p| p.name == "customer_tier"));
    assert!(paths.iter().any(|p| p.name == "product"));

    // Check nested path syntax
    let customer_region =
        find_path_by_name(&paths, "customer_region").expect("customer_region path");
    assert_eq!(customer_region.json_path, "data->'customer'->>'region'");
}

#[test]
fn test_extract_dimension_paths_various_types() {
    let sample = serde_json::json!({
        "name": "test",
        "count": 42,
        "price": 19.99,
        "active": true,
        "tags": ["a", "b"],
        "metadata": {}
    });

    let paths =
        FactTableDetector::extract_dimension_paths(&sample, "dimensions", DatabaseType::PostgreSQL);

    // Check type inference
    let name = paths.iter().find(|p| p.name == "name").unwrap();
    assert_eq!(name.data_type, "string");

    let count = paths.iter().find(|p| p.name == "count").unwrap();
    assert_eq!(count.data_type, "integer");

    let price = paths.iter().find(|p| p.name == "price").unwrap();
    assert_eq!(price.data_type, "float");

    let active = paths.iter().find(|p| p.name == "active").unwrap();
    assert_eq!(active.data_type, "boolean");

    let tags = paths.iter().find(|p| p.name == "tags").unwrap();
    assert_eq!(tags.data_type, "array");

    let metadata = paths.iter().find(|p| p.name == "metadata").unwrap();
    assert_eq!(metadata.data_type, "object");
}

#[test]
fn test_generate_json_path_postgres() {
    // Top-level
    assert_eq!(
        FactTableDetector::generate_json_path("dimensions", "category", DatabaseType::PostgreSQL),
        "dimensions->>'category'"
    );

    // Nested
    assert_eq!(
        FactTableDetector::generate_json_path("data", "customer.region", DatabaseType::PostgreSQL),
        "data->'customer'->>'region'"
    );

    // Deeply nested
    assert_eq!(
        FactTableDetector::generate_json_path("data", "a.b.c", DatabaseType::PostgreSQL),
        "data->'a'->'b'->>'c'"
    );
}

#[test]
fn test_generate_json_path_mysql() {
    assert_eq!(
        FactTableDetector::generate_json_path("dimensions", "category", DatabaseType::MySQL),
        "JSON_UNQUOTE(JSON_EXTRACT(dimensions, '$.category')"
    );

    assert_eq!(
        FactTableDetector::generate_json_path("data", "customer.region", DatabaseType::MySQL),
        "JSON_UNQUOTE(JSON_EXTRACT(data, '$.customer.region')"
    );
}

#[test]
fn test_generate_json_path_sqlite() {
    assert_eq!(
        FactTableDetector::generate_json_path("dimensions", "category", DatabaseType::SQLite),
        "json_extract(dimensions, '$.category')"
    );

    assert_eq!(
        FactTableDetector::generate_json_path("data", "customer.region", DatabaseType::SQLite),
        "json_extract(data, '$.customer.region')"
    );
}

#[test]
fn test_generate_json_path_sqlserver() {
    assert_eq!(
        FactTableDetector::generate_json_path("dimensions", "category", DatabaseType::SQLServer),
        "JSON_VALUE(dimensions, '$.category')"
    );

    assert_eq!(
        FactTableDetector::generate_json_path("data", "customer.region", DatabaseType::SQLServer),
        "JSON_VALUE(data, '$.customer.region')"
    );
}

#[test]
fn test_infer_json_type() {
    assert_eq!(FactTableDetector::infer_json_type(&serde_json::json!(null)), "string");
    assert_eq!(FactTableDetector::infer_json_type(&serde_json::json!(true)), "boolean");
    assert_eq!(FactTableDetector::infer_json_type(&serde_json::json!(42)), "integer");
    assert_eq!(FactTableDetector::infer_json_type(&serde_json::json!(1.5)), "float");
    assert_eq!(FactTableDetector::infer_json_type(&serde_json::json!("hello")), "string");
    assert_eq!(FactTableDetector::infer_json_type(&serde_json::json!([1, 2, 3])), "array");
    assert_eq!(FactTableDetector::infer_json_type(&serde_json::json!({"a": 1})), "object");
}

#[test]
fn test_extract_paths_depth_limit() {
    // Create deeply nested structure
    let sample = serde_json::json!({
        "level1": {
            "level2": {
                "level3": {
                    "level4": {
                        "level5": "too deep"
                    }
                }
            }
        }
    });

    let paths =
        FactTableDetector::extract_dimension_paths(&sample, "data", DatabaseType::PostgreSQL);

    // Should stop at depth 3 (level1, level2, level3, level4 but not level5)
    assert!(paths.iter().any(|p| p.name == "level1"));
    assert!(paths.iter().any(|p| p.name == "level1_level2"));
    assert!(paths.iter().any(|p| p.name == "level1_level2_level3"));
    assert!(paths.iter().any(|p| p.name == "level1_level2_level3_level4"));
    // level5 should NOT be extracted due to depth limit
    assert!(!paths.iter().any(|p| p.name.contains("level5")));
}

#[test]
fn test_extract_paths_empty_object() {
    let sample = serde_json::json!({});
    let paths =
        FactTableDetector::extract_dimension_paths(&sample, "dimensions", DatabaseType::PostgreSQL);
    assert!(paths.is_empty());
}

#[test]
fn test_extract_paths_non_object() {
    // Array at root level
    let sample = serde_json::json!([1, 2, 3]);
    let paths =
        FactTableDetector::extract_dimension_paths(&sample, "dimensions", DatabaseType::PostgreSQL);
    assert!(paths.is_empty());

    // Scalar at root level
    let sample = serde_json::json!("just a string");
    let paths =
        FactTableDetector::extract_dimension_paths(&sample, "dimensions", DatabaseType::PostgreSQL);
    assert!(paths.is_empty());
}

// ==================== Explicit Fact Table Declaration Tests ====================

#[test]
fn test_aggregation_strategy_serialization() {
    // Test incremental
    let incremental_json = serde_json::json!("incremental");
    let strategy: AggregationStrategy = serde_json::from_value(incremental_json).unwrap();
    assert_eq!(strategy, AggregationStrategy::Incremental);

    // Test accumulating_snapshot
    let accum_json = serde_json::json!("accumulating_snapshot");
    let strategy: AggregationStrategy = serde_json::from_value(accum_json).unwrap();
    assert_eq!(strategy, AggregationStrategy::AccumulatingSnapshot);

    // Test periodic_snapshot
    let periodic_json = serde_json::json!("periodic_snapshot");
    let strategy: AggregationStrategy = serde_json::from_value(periodic_json).unwrap();
    assert_eq!(strategy, AggregationStrategy::PeriodicSnapshot);
}

#[test]
fn test_aggregation_strategy_default() {
    let strategy = AggregationStrategy::default();
    assert_eq!(strategy, AggregationStrategy::Incremental);
}

#[test]
fn test_aggregation_strategy_equality() {
    assert_eq!(AggregationStrategy::Incremental, AggregationStrategy::Incremental);
    assert_ne!(AggregationStrategy::Incremental, AggregationStrategy::AccumulatingSnapshot);
}

#[test]
fn test_fact_table_declaration_basic() {
    let decl = FactTableDeclaration {
        name: "tf_sales".to_string(),
        measures: vec!["amount".to_string(), "quantity".to_string()],
        dimensions: vec!["product_id".to_string(), "region_id".to_string()],
        primary_key: "id".to_string(),
        metadata: None,
    };

    assert_eq!(decl.name, "tf_sales");
    assert_eq!(decl.measures.len(), 2);
    assert_eq!(decl.dimensions.len(), 2);
    assert_eq!(decl.primary_key, "id");
    assert!(decl.metadata.is_none());
}

#[test]
fn test_fact_table_declaration_with_metadata() {
    let metadata = FactTableDeclarationMetadata {
        aggregation_strategy: AggregationStrategy::Incremental,
        grain: vec!["date".to_string(), "product".to_string()],
        snapshot_date_column: None,
        is_slowly_changing_dimension: false,
    };

    let decl = FactTableDeclaration {
        name: "tf_events".to_string(),
        measures: vec!["count".to_string()],
        dimensions: vec!["user_id".to_string(), "event_type".to_string()],
        primary_key: "id".to_string(),
        metadata: Some(metadata),
    };

    assert!(decl.metadata.is_some());
    let meta = decl.metadata.unwrap();
    assert_eq!(meta.aggregation_strategy, AggregationStrategy::Incremental);
    assert_eq!(meta.grain.len(), 2);
}

#[test]
fn test_fact_table_declaration_periodic_snapshot() {
    let metadata = FactTableDeclarationMetadata {
        aggregation_strategy: AggregationStrategy::PeriodicSnapshot,
        grain: vec!["date".to_string()],
        snapshot_date_column: Some("snapshot_date".to_string()),
        is_slowly_changing_dimension: false,
    };

    let decl = FactTableDeclaration {
        name: "tf_inventory".to_string(),
        measures: vec!["quantity_on_hand".to_string()],
        dimensions: vec!["warehouse_id".to_string()],
        primary_key: "id".to_string(),
        metadata: Some(metadata),
    };

    assert_eq!(decl.name, "tf_inventory");
    let meta = decl.metadata.unwrap();
    assert_eq!(meta.aggregation_strategy, AggregationStrategy::PeriodicSnapshot);
    assert_eq!(meta.snapshot_date_column, Some("snapshot_date".to_string()));
}

#[test]
fn test_fact_table_declaration_json_serialization() {
    let json_str = r#"{
        "name": "tf_sales",
        "measures": ["amount", "quantity"],
        "dimensions": ["product_id"],
        "primary_key": "id",
        "metadata": {
            "aggregation_strategy": "incremental",
            "grain": ["date", "product"],
            "is_slowly_changing_dimension": false
        }
    }"#;

    let decl: FactTableDeclaration = serde_json::from_str(json_str).unwrap();

    assert_eq!(decl.name, "tf_sales");
    assert_eq!(decl.measures.len(), 2);
    assert!(decl.metadata.is_some());

    let meta = decl.metadata.unwrap();
    assert_eq!(meta.aggregation_strategy, AggregationStrategy::Incremental);
}

#[test]
fn test_fact_table_declaration_json_roundtrip() {
    let original = FactTableDeclaration {
        name: "tf_orders".to_string(),
        measures: vec!["amount".to_string()],
        dimensions: vec!["customer_id".to_string()],
        primary_key: "id".to_string(),
        metadata: Some(FactTableDeclarationMetadata {
            aggregation_strategy: AggregationStrategy::AccumulatingSnapshot,
            grain: vec!["order_id".to_string()],
            snapshot_date_column: None,
            is_slowly_changing_dimension: false,
        }),
    };

    // Serialize
    let json = serde_json::to_string(&original).unwrap();

    // Deserialize
    let deserialized: FactTableDeclaration = serde_json::from_str(&json).unwrap();

    // Verify roundtrip
    assert_eq!(original, deserialized);
}

#[test]
fn test_fact_table_declaration_metadata_default_strategy() {
    let json_str = r#"{
        "name": "tf_events",
        "measures": ["count"],
        "dimensions": ["event_type"],
        "primary_key": "id",
        "metadata": {
            "grain": ["date"]
        }
    }"#;

    let decl: FactTableDeclaration = serde_json::from_str(json_str).unwrap();
    let meta = decl.metadata.unwrap();

    // Should default to Incremental
    assert_eq!(meta.aggregation_strategy, AggregationStrategy::default());
}

#[test]
fn test_multiple_fact_table_declarations() {
    let declarations = [
        FactTableDeclaration {
            name: "tf_sales".to_string(),
            measures: vec!["amount".to_string()],
            dimensions: vec!["product_id".to_string()],
            primary_key: "id".to_string(),
            metadata: None,
        },
        FactTableDeclaration {
            name: "tf_events".to_string(),
            measures: vec!["count".to_string()],
            dimensions: vec!["user_id".to_string()],
            primary_key: "id".to_string(),
            metadata: None,
        },
    ];

    assert_eq!(declarations.len(), 2);
    assert_eq!(declarations[0].name, "tf_sales");
    assert_eq!(declarations[1].name, "tf_events");
}

#[test]
fn test_fact_table_declaration_large_grain() {
    let metadata = FactTableDeclarationMetadata {
        aggregation_strategy: AggregationStrategy::Incremental,
        grain: vec![
            "date".to_string(),
            "product".to_string(),
            "region".to_string(),
            "customer".to_string(),
        ],
        snapshot_date_column: None,
        is_slowly_changing_dimension: false,
    };

    let decl = FactTableDeclaration {
        name: "tf_sales_detailed".to_string(),
        measures: vec!["amount".to_string(), "quantity".to_string()],
        dimensions: vec![
            "date_id".to_string(),
            "product_id".to_string(),
            "region_id".to_string(),
            "customer_id".to_string(),
        ],
        primary_key: "id".to_string(),
        metadata: Some(metadata),
    };

    let meta = decl.metadata.unwrap();
    assert_eq!(meta.grain.len(), 4);
    assert_eq!(decl.dimensions.len(), 4);
}
