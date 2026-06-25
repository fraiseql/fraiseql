//! Tests for the `runtime` module.

#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable
mod aggregate_parser_tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
    use serde_json::json;

    use crate::{
        compiler::{
            aggregate_types::{AggregateFunction, HavingOperator, TemporalBucket},
            aggregation::{AggregateSelection, GroupBySelection, OrderDirection},
            fact_table::{
                DimensionColumn, FactTableMetadata, FilterColumn, MeasureColumn, SqlType,
            },
        },
        error::FraiseQLError,
        runtime::aggregate_parser::*,
    };

    fn create_test_metadata() -> FactTableMetadata {
        use crate::compiler::fact_table::DimensionPath;

        FactTableMetadata {
            table_name:               "tf_sales".to_string(),
            measures:                 vec![
                MeasureColumn {
                    name:     "revenue".to_string(),
                    sql_type: SqlType::Decimal,
                    nullable: false,
                },
                MeasureColumn {
                    name:     "quantity".to_string(),
                    sql_type: SqlType::Int,
                    nullable: false,
                },
            ],
            dimensions:               DimensionColumn {
                name:  "dimensions".to_string(),
                paths: vec![
                    DimensionPath {
                        name:      "category".to_string(),
                        json_path: "data->>'category'".to_string(),
                        data_type: "text".to_string(),
                    },
                    DimensionPath {
                        name:      "product".to_string(),
                        json_path: "data->>'product'".to_string(),
                        data_type: "text".to_string(),
                    },
                ],
            },
            denormalized_filters:     vec![FilterColumn {
                name:     "occurred_at".to_string(),
                sql_type: SqlType::Timestamp,
                indexed:  true,
            }],
            calendar_dimensions:      vec![],
            partial_period:           None,
            native_measures:          std::collections::HashMap::new(),
            native_dimension_mapping: std::collections::HashMap::new(),
        }
    }

    #[test]
    fn test_parse_simple_count() {
        let metadata = create_test_metadata();
        let query = json!({
            "table": "tf_sales",
            "aggregates": [
                {"count": {}}
            ]
        });

        let request =
            AggregateQueryParser::parse(&query, &metadata, &std::collections::HashMap::new())
                .unwrap();

        assert_eq!(request.table_name, "tf_sales");
        assert_eq!(request.aggregates.len(), 1);
        assert_eq!(request.aggregates[0].alias(), "count");
    }

    #[test]
    fn test_parse_group_by_dimension() {
        let metadata = create_test_metadata();
        let query = json!({
            "table": "tf_sales",
            "groupBy": {
                "category": true
            },
            "aggregates": [
                {"count": {}}
            ]
        });

        let request =
            AggregateQueryParser::parse(&query, &metadata, &std::collections::HashMap::new())
                .unwrap();

        assert_eq!(request.group_by.len(), 1);
        match &request.group_by[0] {
            GroupBySelection::Dimension { path, alias } => {
                assert_eq!(path, "category");
                assert_eq!(alias, "category");
            },
            _ => panic!("Expected Dimension selection"),
        }
    }

    #[test]
    fn test_parse_temporal_bucket() {
        let metadata = create_test_metadata();
        let query = json!({
            "table": "tf_sales",
            "groupBy": {
                "occurred_at_day": true
            },
            "aggregates": [
                {"count": {}}
            ]
        });

        let request =
            AggregateQueryParser::parse(&query, &metadata, &std::collections::HashMap::new())
                .unwrap();

        assert_eq!(request.group_by.len(), 1);
        match &request.group_by[0] {
            GroupBySelection::TemporalBucket {
                column,
                bucket,
                alias,
            } => {
                assert_eq!(column, "occurred_at");
                assert_eq!(*bucket, TemporalBucket::Day);
                assert_eq!(alias, "occurred_at_day");
            },
            _ => panic!("Expected TemporalBucket selection"),
        }
    }

    #[test]
    fn test_parse_multiple_aggregates() {
        let metadata = create_test_metadata();
        let query = json!({
            "table": "tf_sales",
            "aggregates": [
                {"count": {}},
                {"revenue_sum": {}},
                {"revenue_avg": {}},
                {"quantity_max": {}}
            ]
        });

        let request =
            AggregateQueryParser::parse(&query, &metadata, &std::collections::HashMap::new())
                .unwrap();

        assert_eq!(request.aggregates.len(), 4);
        assert_eq!(request.aggregates[0].alias(), "count");
        assert_eq!(request.aggregates[1].alias(), "revenue_sum");
        assert_eq!(request.aggregates[2].alias(), "revenue_avg");
        assert_eq!(request.aggregates[3].alias(), "quantity_max");
    }

    #[test]
    fn test_parse_having_condition() {
        let metadata = create_test_metadata();
        let query = json!({
            "table": "tf_sales",
            "aggregates": [
                {"revenue_sum": {}}
            ],
            "having": {
                "revenue_sum_gt": 1000
            }
        });

        let request =
            AggregateQueryParser::parse(&query, &metadata, &std::collections::HashMap::new())
                .unwrap();

        assert_eq!(request.having.len(), 1);
        assert_eq!(request.having[0].operator, HavingOperator::Gt);
        assert_eq!(request.having[0].value, json!(1000));
    }

    #[test]
    fn test_parse_order_by() {
        let metadata = create_test_metadata();
        let query = json!({
            "table": "tf_sales",
            "aggregates": [
                {"revenue_sum": {}}
            ],
            "orderBy": {
                "revenue_sum": "DESC"
            }
        });

        let request =
            AggregateQueryParser::parse(&query, &metadata, &std::collections::HashMap::new())
                .unwrap();

        assert_eq!(request.order_by.len(), 1);
        assert_eq!(request.order_by[0].field, "revenue_sum");
        assert_eq!(request.order_by[0].direction, OrderDirection::Desc);
    }

    #[test]
    fn test_parse_limit_offset() {
        let metadata = create_test_metadata();
        let query = json!({
            "table": "tf_sales",
            "aggregates": [
                {"count": {}}
            ],
            "limit": 10,
            "offset": 5
        });

        let request =
            AggregateQueryParser::parse(&query, &metadata, &std::collections::HashMap::new())
                .unwrap();

        assert_eq!(request.limit, Some(10));
        assert_eq!(request.offset, Some(5));
    }

    #[test]
    fn test_parse_complex_query() {
        let metadata = create_test_metadata();
        let query = json!({
            "table": "tf_sales",
            "groupBy": {
                "category": true,
                "occurred_at_month": true
            },
            "aggregates": [
                {"count": {}},
                {"revenue_sum": {}},
                {"revenue_avg": {}},
                {"quantity_sum": {}}
            ],
            "having": {
                "revenue_sum_gt": 1000,
                "count_gte": 5
            },
            "orderBy": {
                "revenue_sum": "DESC",
                "count": "ASC"
            },
            "limit": 20
        });

        let request =
            AggregateQueryParser::parse(&query, &metadata, &std::collections::HashMap::new())
                .unwrap();

        assert_eq!(request.table_name, "tf_sales");
        assert_eq!(request.group_by.len(), 2);
        assert_eq!(request.aggregates.len(), 4);
        assert_eq!(request.having.len(), 2);
        assert_eq!(request.order_by.len(), 2);
        assert_eq!(request.limit, Some(20));
    }

    #[test]
    fn test_parse_count_distinct_default() {
        let metadata = create_test_metadata();
        let query = json!({
            "table": "tf_sales",
            "aggregates": [
                {"count_distinct": {}}
            ]
        });

        let request =
            AggregateQueryParser::parse(&query, &metadata, &std::collections::HashMap::new())
                .unwrap();

        assert_eq!(request.aggregates.len(), 1);
        match &request.aggregates[0] {
            AggregateSelection::CountDistinct { field, alias } => {
                // Defaults to first dimension: "category"
                assert_eq!(field, "category");
                assert_eq!(alias, "count_distinct");
            },
            _ => panic!("Expected CountDistinct selection"),
        }
    }

    #[test]
    fn test_parse_count_distinct_with_field() {
        let metadata = create_test_metadata();
        let query = json!({
            "table": "tf_sales",
            "aggregates": [
                {"product_count_distinct": {}}
            ]
        });

        let request =
            AggregateQueryParser::parse(&query, &metadata, &std::collections::HashMap::new())
                .unwrap();

        assert_eq!(request.aggregates.len(), 1);
        match &request.aggregates[0] {
            AggregateSelection::CountDistinct { field, alias } => {
                assert_eq!(field, "product");
                assert_eq!(alias, "product_count_distinct");
            },
            _ => panic!("Expected CountDistinct selection"),
        }
    }

    #[test]
    fn test_parse_count_distinct_on_measure() {
        let metadata = create_test_metadata();
        let query = json!({
            "table": "tf_sales",
            "aggregates": [
                {"revenue_count_distinct": {}}
            ]
        });

        let request =
            AggregateQueryParser::parse(&query, &metadata, &std::collections::HashMap::new())
                .unwrap();

        assert_eq!(request.aggregates.len(), 1);
        match &request.aggregates[0] {
            AggregateSelection::CountDistinct { field, alias } => {
                assert_eq!(field, "revenue");
                assert_eq!(alias, "revenue_count_distinct");
            },
            _ => panic!("Expected CountDistinct selection"),
        }
    }

    #[test]
    fn test_parse_count_distinct_invalid_field() {
        let metadata = create_test_metadata();
        let query = json!({
            "table": "tf_sales",
            "aggregates": [
                {"nonexistent_count_distinct": {}}
            ]
        });

        let result =
            AggregateQueryParser::parse(&query, &metadata, &std::collections::HashMap::new());

        let err = result.expect_err("expected Err for invalid count_distinct field");
        match err {
            FraiseQLError::Validation { message, .. } => {
                assert!(
                    message.contains("COUNT DISTINCT field 'nonexistent' not found"),
                    "unexpected message: {message}"
                );
            },
            other => panic!("expected Validation error, got: {other:?}"),
        }
    }

    #[test]
    fn test_parse_multiple_count_distinct() {
        let metadata = create_test_metadata();
        let query = json!({
            "table": "tf_sales",
            "aggregates": [
                {"count": {}},
                {"category_count_distinct": {}},
                {"product_count_distinct": {}},
                {"revenue_sum": {}}
            ]
        });

        let request =
            AggregateQueryParser::parse(&query, &metadata, &std::collections::HashMap::new())
                .unwrap();

        assert_eq!(request.aggregates.len(), 4);
        assert_eq!(request.aggregates[0].alias(), "count");
        assert_eq!(request.aggregates[1].alias(), "category_count_distinct");
        assert_eq!(request.aggregates[2].alias(), "product_count_distinct");
        assert_eq!(request.aggregates[3].alias(), "revenue_sum");
    }

    #[test]
    fn test_parser_resolves_native_measure_aggregate() {
        let metadata = FactTableMetadata {
            table_name:               "mv_daily_sales".to_string(),
            measures:                 vec![],
            dimensions:               DimensionColumn {
                name:  "data".to_string(),
                paths: vec![],
            },
            denormalized_filters:     vec![],
            calendar_dimensions:      vec![],
            partial_period:           None,
            native_measures:          std::collections::HashMap::from([(
                "measures.volume".to_string(),
                "volume".to_string(),
            )]),
            native_dimension_mapping: std::collections::HashMap::new(),
        };

        let query = json!({
            "table": "mv_daily_sales",
            "aggregates": [
                {"measures.volume_sum": {}}
            ]
        });

        let request =
            AggregateQueryParser::parse(&query, &metadata, &std::collections::HashMap::new())
                .unwrap();

        assert_eq!(request.aggregates.len(), 1);
        assert_eq!(request.aggregates[0].alias(), "measures.volume_sum");
        // The measure field should be the JSONB path, resolved by planner later
        match &request.aggregates[0] {
            crate::compiler::aggregation::AggregateSelection::MeasureAggregate {
                measure,
                function,
                ..
            } => {
                assert_eq!(measure, "measures.volume");
                assert_eq!(*function, AggregateFunction::Sum);
            },
            other => panic!("Expected MeasureAggregate, got: {other:?}"),
        }
    }

    #[test]
    fn test_group_by_uses_dimension_mapping() {
        let metadata = FactTableMetadata {
            table_name:               "mv_daily_sales".to_string(),
            measures:                 vec![],
            dimensions:               DimensionColumn {
                name:  "data".to_string(),
                paths: vec![],
            },
            denormalized_filters:     vec![],
            calendar_dimensions:      vec![],
            partial_period:           None,
            native_measures:          std::collections::HashMap::new(),
            native_dimension_mapping: std::collections::HashMap::from([(
                "dimensions.category.id".to_string(),
                "category_id".to_string(),
            )]),
        };

        let query = json!({
            "table": "mv_daily_sales",
            "groupBy": {"dimensions.category.id": true},
            "aggregates": [{"count": {}}]
        });

        let request =
            AggregateQueryParser::parse(&query, &metadata, &std::collections::HashMap::new())
                .unwrap();

        assert_eq!(request.group_by.len(), 1);
        match &request.group_by[0] {
            crate::compiler::aggregation::GroupBySelection::NativeDimension { column, .. } => {
                assert_eq!(column, "category_id");
            },
            other => panic!("Expected NativeDimension, got: {other:?}"),
        }
    }

    #[test]
    fn test_mapped_dimension_not_in_group_by_unless_selected() {
        let metadata = FactTableMetadata {
            table_name:               "mv_daily_sales".to_string(),
            measures:                 vec![MeasureColumn {
                name:     "volume".to_string(),
                sql_type: SqlType::BigInt,
                nullable: false,
            }],
            dimensions:               DimensionColumn {
                name:  "data".to_string(),
                paths: vec![],
            },
            denormalized_filters:     vec![],
            calendar_dimensions:      vec![],
            partial_period:           None,
            native_measures:          std::collections::HashMap::new(),
            native_dimension_mapping: std::collections::HashMap::from([(
                "dimensions.category.id".to_string(),
                "category_id".to_string(),
            )]),
        };

        // Query selects only an aggregate, NOT the mapped dimension
        let query = json!({
            "table": "mv_daily_sales",
            "aggregates": [{"volume_sum": {}}]
        });

        let request =
            AggregateQueryParser::parse(&query, &metadata, &std::collections::HashMap::new())
                .unwrap();

        // No GROUP BY at all — the mapped dimension should NOT appear
        assert!(request.group_by.is_empty());
    }
}

mod aggregate_projector_tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
    use std::collections::HashMap;

    use serde_json::{Value, json};

    use crate::{
        compiler::{
            aggregate_types::AggregateFunction,
            aggregation::{
                AggregateExpression, AggregateSelection, AggregationPlan, AggregationRequest,
                GroupByExpression, GroupBySelection,
            },
            fact_table::{
                DimensionColumn, FactTableMetadata, FilterColumn, MeasureColumn, SqlType,
            },
        },
        runtime::aggregate_projector::*,
    };

    fn create_test_plan() -> AggregationPlan {
        use crate::compiler::fact_table::DimensionPath;

        let metadata = FactTableMetadata {
            table_name:               "tf_sales".to_string(),
            measures:                 vec![MeasureColumn {
                name:     "revenue".to_string(),
                sql_type: SqlType::Decimal,
                nullable: false,
            }],
            dimensions:               DimensionColumn {
                name:  "dimensions".to_string(),
                paths: vec![DimensionPath {
                    name:      "category".to_string(),
                    json_path: "data->>'category'".to_string(),
                    data_type: "text".to_string(),
                }],
            },
            denormalized_filters:     vec![FilterColumn {
                name:     "occurred_at".to_string(),
                sql_type: SqlType::Timestamp,
                indexed:  true,
            }],
            calendar_dimensions:      vec![],
            partial_period:           None,
            native_measures:          std::collections::HashMap::new(),
            native_dimension_mapping: std::collections::HashMap::new(),
        };

        let request = AggregationRequest {
            table_name:   "tf_sales".to_string(),
            where_clause: None,
            group_by:     vec![GroupBySelection::Dimension {
                path:  "category".to_string(),
                alias: "category".to_string(),
            }],
            aggregates:   vec![
                AggregateSelection::Count {
                    alias: "count".to_string(),
                },
                AggregateSelection::MeasureAggregate {
                    measure:  "revenue".to_string(),
                    function: AggregateFunction::Sum,
                    alias:    "revenue_sum".to_string(),
                },
            ],
            having:       vec![],
            order_by:     vec![],
            limit:        None,
            offset:       None,
        };

        AggregationPlan {
            metadata,
            request,
            group_by_expressions: vec![GroupByExpression::JsonbPath {
                jsonb_column: "data".to_string(),
                path:         "category".to_string(),
                alias:        "category".to_string(),
            }],
            aggregate_expressions: vec![
                AggregateExpression::Count {
                    alias: "count".to_string(),
                },
                AggregateExpression::MeasureAggregate {
                    column:   "revenue".to_string(),
                    function: AggregateFunction::Sum,
                    alias:    "revenue_sum".to_string(),
                    native:   false,
                },
            ],
            having_conditions: vec![],
        }
    }

    #[test]
    fn test_project_simple_result() {
        let plan = create_test_plan();
        let rows = vec![
            {
                let mut row = HashMap::new();
                row.insert("category".to_string(), json!("Electronics"));
                row.insert("count".to_string(), json!(42));
                row.insert("revenue_sum".to_string(), json!(5280.50));
                row
            },
            {
                let mut row = HashMap::new();
                row.insert("category".to_string(), json!("Books"));
                row.insert("count".to_string(), json!(15));
                row.insert("revenue_sum".to_string(), json!(450.25));
                row
            },
        ];

        let result = AggregationProjector::project(rows, &plan).unwrap();

        assert!(result.is_array());
        let arr = result.as_array().unwrap();
        assert_eq!(arr.len(), 2);

        assert_eq!(arr[0]["category"], "Electronics");
        assert_eq!(arr[0]["count"], 42);
        assert_eq!(arr[0]["revenue_sum"], 5280.50);

        assert_eq!(arr[1]["category"], "Books");
        assert_eq!(arr[1]["count"], 15);
        assert_eq!(arr[1]["revenue_sum"], 450.25);
    }

    #[test]
    fn test_project_empty_result() {
        let plan = create_test_plan();
        let rows = vec![];

        let result = AggregationProjector::project(rows, &plan).unwrap();

        assert!(result.is_array());
        let arr = result.as_array().unwrap();
        assert_eq!(arr.len(), 0);
    }

    #[test]
    fn test_wrap_in_data_envelope() {
        let projected = json!([
            {"category": "Electronics", "count": 42}
        ]);

        let response = AggregationProjector::wrap_in_data_envelope(projected, "sales_aggregate");

        assert!(response.get("data").is_some());
        assert!(response["data"].get("sales_aggregate").is_some());
        assert!(response["data"]["sales_aggregate"].is_array());
        assert_eq!(response["data"]["sales_aggregate"][0]["category"], "Electronics");
    }

    #[test]
    fn test_project_single() {
        let plan = create_test_plan();
        let mut row = HashMap::new();
        row.insert("count".to_string(), json!(100));
        row.insert("revenue_sum".to_string(), json!(10000.0));

        let result = AggregationProjector::project_single(row, &plan).unwrap();

        assert!(result.is_object());
        assert_eq!(result["count"], 100);
        assert_eq!(result["revenue_sum"], 10000.0);
    }

    #[test]
    fn test_project_with_temporal_bucket() {
        let plan = create_test_plan();
        let rows = vec![{
            let mut row = HashMap::new();
            row.insert("category".to_string(), json!("Electronics"));
            row.insert("occurred_at_day".to_string(), json!("2025-01-01"));
            row.insert("count".to_string(), json!(25));
            row.insert("revenue_sum".to_string(), json!(3000.0));
            row
        }];

        let result = AggregationProjector::project(rows, &plan).unwrap();

        assert!(result.is_array());
        let arr = result.as_array().unwrap();
        assert_eq!(arr[0]["occurred_at_day"], "2025-01-01");
    }

    #[test]
    fn test_project_with_null_values() {
        let plan = create_test_plan();
        let rows = vec![{
            let mut row = HashMap::new();
            row.insert("category".to_string(), Value::Null);
            row.insert("count".to_string(), json!(10));
            row.insert("revenue_sum".to_string(), json!(500.0));
            row
        }];

        let result = AggregationProjector::project(rows, &plan).unwrap();

        assert!(result.is_array());
        let arr = result.as_array().unwrap();
        assert_eq!(arr[0]["category"], Value::Null);
        assert_eq!(arr[0]["count"], 10);
    }

    // ========================================
    // Advanced Aggregates Projection Tests
    // ========================================

    #[test]
    fn test_project_array_agg_result() {
        let plan = create_test_plan();
        let rows = vec![{
            let mut row = HashMap::new();
            row.insert("category".to_string(), json!("Electronics"));
            row.insert("count".to_string(), json!(10));
            // PostgreSQL ARRAY_AGG result
            row.insert("products".to_string(), json!(["prod_1", "prod_2", "prod_3"]));
            row
        }];

        let result = AggregationProjector::project(rows, &plan).unwrap();

        assert!(result.is_array());
        let arr = result.as_array().unwrap();
        assert_eq!(arr[0]["category"], "Electronics");
        assert_eq!(arr[0]["products"], json!(["prod_1", "prod_2", "prod_3"]));
    }

    #[test]
    fn test_project_json_agg_result() {
        let plan = create_test_plan();
        let rows = vec![{
            let mut row = HashMap::new();
            row.insert("category".to_string(), json!("Electronics"));
            row.insert("count".to_string(), json!(10));
            // PostgreSQL JSON_AGG result
            row.insert(
                "items".to_string(),
                json!([
                    {"product": "prod_1", "revenue": 1500},
                    {"product": "prod_2", "revenue": 1200}
                ]),
            );
            row
        }];

        let result = AggregationProjector::project(rows, &plan).unwrap();

        assert!(result.is_array());
        let arr = result.as_array().unwrap();
        assert_eq!(arr[0]["category"], "Electronics");
        assert!(arr[0]["items"].is_array());
        let items = arr[0]["items"].as_array().unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0]["product"], "prod_1");
        assert_eq!(items[0]["revenue"], 1500);
    }

    #[test]
    fn test_project_string_agg_result() {
        let plan = create_test_plan();
        let rows = vec![{
            let mut row = HashMap::new();
            row.insert("category".to_string(), json!("Electronics"));
            row.insert("count".to_string(), json!(10));
            // PostgreSQL STRING_AGG result
            row.insert("product_names".to_string(), json!("Laptop, Phone, Tablet"));
            row
        }];

        let result = AggregationProjector::project(rows, &plan).unwrap();

        assert!(result.is_array());
        let arr = result.as_array().unwrap();
        assert_eq!(arr[0]["category"], "Electronics");
        assert_eq!(arr[0]["product_names"], "Laptop, Phone, Tablet");
    }

    #[test]
    fn test_project_bool_agg_result() {
        let plan = create_test_plan();
        let rows = vec![{
            let mut row = HashMap::new();
            row.insert("category".to_string(), json!("Electronics"));
            row.insert("count".to_string(), json!(10));
            // PostgreSQL BOOL_AND result
            row.insert("all_active".to_string(), json!(true));
            // PostgreSQL BOOL_OR result
            row.insert("any_discounted".to_string(), json!(false));
            row
        }];

        let result = AggregationProjector::project(rows, &plan).unwrap();

        assert!(result.is_array());
        let arr = result.as_array().unwrap();
        assert_eq!(arr[0]["category"], "Electronics");
        assert_eq!(arr[0]["all_active"], true);
        assert_eq!(arr[0]["any_discounted"], false);
    }

    #[test]
    fn test_project_mixed_aggregates() {
        let plan = create_test_plan();
        let rows = vec![{
            let mut row = HashMap::new();
            row.insert("category".to_string(), json!("Electronics"));
            // Basic aggregates
            row.insert("count".to_string(), json!(42));
            row.insert("revenue_sum".to_string(), json!(5280.50));
            row.insert("revenue_avg".to_string(), json!(125.73));
            // Advanced aggregates
            row.insert("products".to_string(), json!(["prod_1", "prod_2"]));
            row.insert("product_names".to_string(), json!("Laptop, Phone"));
            row.insert("all_active".to_string(), json!(true));
            row
        }];

        let result = AggregationProjector::project(rows, &plan).unwrap();

        assert!(result.is_array());
        let arr = result.as_array().unwrap();
        // Verify basic aggregates
        assert_eq!(arr[0]["count"], 42);
        assert_eq!(arr[0]["revenue_sum"], 5280.50);
        // Verify advanced aggregates
        assert_eq!(arr[0]["products"], json!(["prod_1", "prod_2"]));
        assert_eq!(arr[0]["product_names"], "Laptop, Phone");
        assert_eq!(arr[0]["all_active"], true);
    }

    #[test]
    fn test_project_empty_array_agg() {
        let plan = create_test_plan();
        let rows = vec![{
            let mut row = HashMap::new();
            row.insert("category".to_string(), json!("Empty"));
            row.insert("count".to_string(), json!(0));
            // Empty ARRAY_AGG result (NULL in PostgreSQL, [] in others)
            row.insert("products".to_string(), Value::Null);
            row
        }];

        let result = AggregationProjector::project(rows, &plan).unwrap();

        assert!(result.is_array());
        let arr = result.as_array().unwrap();
        assert_eq!(arr[0]["category"], "Empty");
        assert!(arr[0]["products"].is_null());
    }
}

mod cascade_tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
    use serde_json::json;

    use crate::runtime::cascade::*;

    #[test]
    fn to_cascade_code_is_one_to_one() {
        let pairs = [
            (MutationErrorClass::Validation, CascadeErrorCode::ValidationError),
            (MutationErrorClass::Conflict, CascadeErrorCode::Conflict),
            (MutationErrorClass::NotFound, CascadeErrorCode::NotFound),
            (MutationErrorClass::Unauthorized, CascadeErrorCode::Unauthorized),
            (MutationErrorClass::Forbidden, CascadeErrorCode::Forbidden),
            (MutationErrorClass::Internal, CascadeErrorCode::InternalError),
            (MutationErrorClass::TransactionFailed, CascadeErrorCode::TransactionFailed),
            (MutationErrorClass::Timeout, CascadeErrorCode::Timeout),
            (MutationErrorClass::RateLimited, CascadeErrorCode::RateLimited),
            (MutationErrorClass::ServiceUnavailable, CascadeErrorCode::ServiceUnavailable),
        ];
        for (class, expected) in pairs {
            assert_eq!(class.to_cascade_code(), expected, "class = {class:?}");
        }
    }

    #[test]
    fn deserializes_from_pg_enum_snake_case() {
        let pairs = [
            ("validation", MutationErrorClass::Validation),
            ("conflict", MutationErrorClass::Conflict),
            ("not_found", MutationErrorClass::NotFound),
            ("unauthorized", MutationErrorClass::Unauthorized),
            ("forbidden", MutationErrorClass::Forbidden),
            ("internal", MutationErrorClass::Internal),
            ("transaction_failed", MutationErrorClass::TransactionFailed),
            ("timeout", MutationErrorClass::Timeout),
            ("rate_limited", MutationErrorClass::RateLimited),
            ("service_unavailable", MutationErrorClass::ServiceUnavailable),
        ];
        for (raw, expected) in pairs {
            let got: MutationErrorClass = serde_json::from_value(json!(raw)).unwrap();
            assert_eq!(got, expected, "raw = {raw}");
        }
    }

    #[test]
    fn cascade_code_deserializes_from_screaming_snake_case() {
        let pairs = [
            (CascadeErrorCode::ValidationError, "VALIDATION_ERROR"),
            (CascadeErrorCode::Conflict, "CONFLICT"),
            (CascadeErrorCode::NotFound, "NOT_FOUND"),
            (CascadeErrorCode::Unauthorized, "UNAUTHORIZED"),
            (CascadeErrorCode::Forbidden, "FORBIDDEN"),
            (CascadeErrorCode::InternalError, "INTERNAL_ERROR"),
            (CascadeErrorCode::TransactionFailed, "TRANSACTION_FAILED"),
            (CascadeErrorCode::Timeout, "TIMEOUT"),
            (CascadeErrorCode::RateLimited, "RATE_LIMITED"),
            (CascadeErrorCode::ServiceUnavailable, "SERVICE_UNAVAILABLE"),
        ];
        for (code, raw) in pairs {
            let got: CascadeErrorCode = serde_json::from_value(json!(raw)).unwrap();
            assert_eq!(got, code, "raw = {raw}");
        }
    }
}

mod field_filter_tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
    use crate::{
        runtime::field_filter::*,
        schema::{FieldDefinition, FieldDenyPolicy, FieldType, RoleDefinition, SecurityConfig},
        security::SecurityContext,
    };

    fn create_test_field(name: &str, requires_scope: Option<&str>) -> FieldDefinition {
        FieldDefinition {
            name:           name.into(),
            field_type:     FieldType::String,
            nullable:       false,
            default_value:  None,
            description:    None,
            vector_config:  None,
            alias:          None,
            deprecation:    None,
            requires_scope: requires_scope.map(|s| s.to_string()),
            on_deny:        FieldDenyPolicy::default(),
            authorize:      false,
            encryption:     None,
            hierarchy:      None,
        }
    }

    fn create_test_context(roles: &[&str]) -> SecurityContext {
        SecurityContext {
            user_id:          "test-user".into(),
            roles:            roles.iter().map(|&r| r.to_string()).collect(),
            tenant_id:        None,
            scopes:           vec![],
            attributes:       std::collections::HashMap::new(),
            request_id:       "test-req".to_string(),
            ip_address:       None,
            authenticated_at: chrono::Utc::now(),
            expires_at:       chrono::Utc::now() + chrono::Duration::hours(1),
            issuer:           None,
            audience:         None,
            email:            None,
            display_name:     None,
        }
    }

    #[test]
    fn test_can_access_public_field() {
        let field = create_test_field("email", None);
        let context = create_test_context(&[]);
        let config = SecurityConfig::new();

        assert!(
            can_access_field(&context, &config, &field),
            "Public field should be accessible to any user"
        );
    }

    #[test]
    fn test_cannot_access_scoped_field_without_role() {
        let field = create_test_field("password", Some("admin:*"));
        let context = create_test_context(&["viewer"]);
        let mut config = SecurityConfig::new();
        config.add_role(RoleDefinition::new("viewer".to_string(), vec!["read:*".to_string()]));
        config.add_role(RoleDefinition::new("admin".to_string(), vec!["admin:*".to_string()]));

        assert!(
            !can_access_field(&context, &config, &field),
            "User without admin role cannot access admin field"
        );
    }

    #[test]
    fn test_can_access_scoped_field_with_role() {
        let field = create_test_field("password", Some("admin:*"));
        let context = create_test_context(&["admin"]);
        let mut config = SecurityConfig::new();
        config.add_role(RoleDefinition::new("admin".to_string(), vec!["admin:*".to_string()]));

        assert!(
            can_access_field(&context, &config, &field),
            "User with admin role can access admin field"
        );
    }

    #[test]
    fn test_filter_fields_removes_inaccessible() {
        let fields = vec![
            create_test_field("id", None),                       // public
            create_test_field("name", None),                     // public
            create_test_field("email", Some("read:User.email")), // scoped
            create_test_field("password", Some("admin:*")),      // admin only
        ];

        let context = create_test_context(&["viewer"]);
        let mut config = SecurityConfig::new();
        config.add_role(RoleDefinition::new("viewer".to_string(), vec!["read:User.*".to_string()]));
        config.add_role(RoleDefinition::new("admin".to_string(), vec!["admin:*".to_string()]));

        let accessible = filter_fields(&context, &config, &fields);

        // Should have: id, name, email (viewer has read:User.*)
        // Should not have: password (requires admin:*)
        assert_eq!(accessible.len(), 3, "Should have 3 accessible fields");
        assert_eq!(accessible[0].name, "id");
        assert_eq!(accessible[1].name, "name");
        assert_eq!(accessible[2].name, "email");
    }

    #[test]
    fn test_filter_fields_all_accessible() {
        let fields = vec![
            create_test_field("id", None),
            create_test_field("name", None),
            create_test_field("email", Some("read:User.email")),
            create_test_field("password", Some("admin:*")),
        ];

        let context = create_test_context(&["admin"]);
        let mut config = SecurityConfig::new();
        config.add_role(RoleDefinition::new("admin".to_string(), vec!["*".to_string()]));

        let accessible = filter_fields(&context, &config, &fields);

        // Admin has global wildcard (*) which matches all scopes
        assert_eq!(accessible.len(), 4, "Admin with global wildcard should access all fields");
    }

    // =========================================================================
    // classify_field_access tests
    // =========================================================================

    fn create_field_with_deny(
        name: &str,
        requires_scope: Option<&str>,
        on_deny: FieldDenyPolicy,
    ) -> FieldDefinition {
        FieldDefinition {
            name: name.into(),
            field_type: FieldType::String,
            nullable: false,
            default_value: None,
            description: None,
            vector_config: None,
            alias: None,
            deprecation: None,
            requires_scope: requires_scope.map(|s| s.to_string()),
            on_deny,
            authorize: false,
            encryption: None,
            hierarchy: None,
        }
    }

    #[test]
    fn test_classify_all_public_fields() {
        let fields = vec![
            create_field_with_deny("id", None, FieldDenyPolicy::Reject),
            create_field_with_deny("name", None, FieldDenyPolicy::Reject),
        ];
        let ctx = create_test_context(&[]);
        let config = SecurityConfig::new();

        let result = classify_field_access(
            &ctx,
            &config,
            &fields,
            vec!["id".to_string(), "name".to_string()],
        );

        let access = result.expect("should succeed");
        assert_eq!(access.allowed, vec!["id", "name"]);
        assert!(access.masked.is_empty());
    }

    #[test]
    fn test_classify_mask_field_unauthorized() {
        let fields = vec![
            create_field_with_deny("id", None, FieldDenyPolicy::Reject),
            create_field_with_deny("email", Some("read:email"), FieldDenyPolicy::Mask),
        ];
        let ctx = create_test_context(&["viewer"]);
        let mut config = SecurityConfig::new();
        config.add_role(RoleDefinition::new("viewer".to_string(), vec!["read:name".to_string()]));

        let result = classify_field_access(
            &ctx,
            &config,
            &fields,
            vec!["id".to_string(), "email".to_string()],
        );

        let access = result.expect("should succeed (mask, not reject)");
        assert_eq!(access.allowed, vec!["id"]);
        assert_eq!(access.masked, vec!["email"]);
    }

    #[test]
    fn test_classify_reject_field_unauthorized() {
        let fields = vec![
            create_field_with_deny("id", None, FieldDenyPolicy::Reject),
            create_field_with_deny("salary", Some("admin:*"), FieldDenyPolicy::Reject),
        ];
        let ctx = create_test_context(&["viewer"]);
        let mut config = SecurityConfig::new();
        config.add_role(RoleDefinition::new("viewer".to_string(), vec!["read:*".to_string()]));

        let result = classify_field_access(
            &ctx,
            &config,
            &fields,
            vec!["id".to_string(), "salary".to_string()],
        );

        assert_eq!(result.unwrap_err(), "salary");
    }

    #[test]
    fn test_classify_authorized_user_gets_all_fields() {
        let fields = vec![
            create_field_with_deny("id", None, FieldDenyPolicy::Reject),
            create_field_with_deny("email", Some("read:email"), FieldDenyPolicy::Mask),
            create_field_with_deny("salary", Some("admin:*"), FieldDenyPolicy::Reject),
        ];
        let ctx = create_test_context(&["admin"]);
        let mut config = SecurityConfig::new();
        config.add_role(RoleDefinition::new("admin".to_string(), vec!["*".to_string()]));

        let result = classify_field_access(
            &ctx,
            &config,
            &fields,
            vec!["id".to_string(), "email".to_string(), "salary".to_string()],
        );

        let access = result.expect("admin has all scopes");
        assert_eq!(access.allowed, vec!["id", "email", "salary"]);
        assert!(access.masked.is_empty());
    }

    #[test]
    fn test_classify_mixed_mask_and_reject_rejects() {
        // If a query requests both mask and reject fields the user lacks,
        // the reject field causes failure (reject wins).
        let fields = vec![
            create_field_with_deny("id", None, FieldDenyPolicy::Reject),
            create_field_with_deny("email", Some("read:email"), FieldDenyPolicy::Mask),
            create_field_with_deny("salary", Some("hr:*"), FieldDenyPolicy::Reject),
        ];
        let ctx = create_test_context(&["viewer"]);
        let mut config = SecurityConfig::new();
        config.add_role(RoleDefinition::new("viewer".to_string(), vec!["read:name".to_string()]));

        let result = classify_field_access(
            &ctx,
            &config,
            &fields,
            vec!["id".to_string(), "email".to_string(), "salary".to_string()],
        );

        // salary is reject → error
        assert_eq!(result.unwrap_err(), "salary");
    }

    #[test]
    fn test_classify_unrequested_protected_field_no_error() {
        // If a protected field exists but isn't requested, no error.
        let fields = vec![
            create_field_with_deny("id", None, FieldDenyPolicy::Reject),
            create_field_with_deny("salary", Some("admin:*"), FieldDenyPolicy::Reject),
        ];
        let ctx = create_test_context(&["viewer"]);
        let config = SecurityConfig::new();

        let result = classify_field_access(
            &ctx,
            &config,
            &fields,
            vec!["id".to_string()], // salary not requested
        );

        let access = result.expect("should succeed — salary not requested");
        assert_eq!(access.allowed, vec!["id"]);
        assert!(access.masked.is_empty());
    }
}

mod input_validator_tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
    use serde_json::Value;

    use crate::{
        error::{FraiseQLError, ValidationFieldError},
        runtime::input_validator::*,
        validation::{CompiledPattern, ValidationRule},
    };

    #[test]
    fn test_validation_error_collection() {
        let mut errors = ValidationErrorCollection::new();
        assert!(errors.is_empty());

        errors.add_error(ValidationFieldError::new("email", "pattern", "Invalid email"));
        assert!(!errors.is_empty());
        assert_eq!(errors.len(), 1);
    }

    #[test]
    fn test_validation_error_collection_to_error() {
        let mut errors = ValidationErrorCollection::new();
        errors.add_error(ValidationFieldError::new("email", "pattern", "Invalid email"));

        let err = errors.to_error();
        assert!(matches!(err, FraiseQLError::Validation { .. }));
    }

    #[test]
    fn test_validate_required_field() {
        let rule = ValidationRule::Required;
        let result = validate_string_field("value", "field", &rule);
        result.unwrap_or_else(|e| panic!("expected Ok for non-empty value: {e}"));

        let result = validate_string_field("", "field", &rule);
        assert!(
            matches!(result, Err(FraiseQLError::Validation { .. })),
            "expected Validation error for empty required field, got: {result:?}"
        );
    }

    #[test]
    fn test_validate_pattern() {
        let rule = ValidationRule::Pattern {
            pattern: CompiledPattern::new("^[a-z]+$").expect("valid regex"),
            message: None,
        };

        let result = validate_string_field("hello", "field", &rule);
        result.unwrap_or_else(|e| panic!("expected Ok for matching pattern: {e}"));

        let result = validate_string_field("Hello", "field", &rule);
        assert!(
            matches!(result, Err(FraiseQLError::Validation { .. })),
            "expected Validation error for non-matching pattern, got: {result:?}"
        );
    }

    #[test]
    fn test_validate_length() {
        let rule = ValidationRule::Length {
            min: Some(3),
            max: Some(10),
        };

        let result = validate_string_field("hello", "field", &rule);
        result.unwrap_or_else(|e| panic!("expected Ok for in-range length: {e}"));

        let result = validate_string_field("hi", "field", &rule);
        assert!(
            matches!(result, Err(FraiseQLError::Validation { .. })),
            "expected Validation error for too-short string, got: {result:?}"
        );

        let result = validate_string_field("this is too long", "field", &rule);
        assert!(
            matches!(result, Err(FraiseQLError::Validation { .. })),
            "expected Validation error for too-long string, got: {result:?}"
        );
    }

    #[test]
    fn test_validate_enum() {
        let rule = ValidationRule::Enum {
            values: vec!["active".to_string(), "inactive".to_string()],
        };

        let result = validate_string_field("active", "field", &rule);
        result.unwrap_or_else(|e| panic!("expected Ok for valid enum value: {e}"));

        let result = validate_string_field("unknown", "field", &rule);
        assert!(
            matches!(result, Err(FraiseQLError::Validation { .. })),
            "expected Validation error for invalid enum value, got: {result:?}"
        );
    }

    #[test]
    fn test_validate_null_field() {
        let rule = ValidationRule::Required;
        let result = validate_input(&Value::Null, "field", &[rule]);
        assert!(
            matches!(result, Err(FraiseQLError::Validation { .. })),
            "expected Validation error for null required field, got: {result:?}"
        );
    }

    #[test]
    fn test_validate_custom_scalar_library_code_valid() {
        use crate::{
            schema::CompiledSchema,
            validation::{CustomTypeDef, CustomTypeRegistry, CustomTypeRegistryConfig},
        };

        let schema = {
            let mut s = CompiledSchema::new();
            let registry = CustomTypeRegistry::new(CustomTypeRegistryConfig::default());

            let mut def = CustomTypeDef::new("LibraryCode".to_string());
            def.validation_rules = vec![ValidationRule::Pattern {
                pattern: CompiledPattern::new(r"^LIB-[0-9]{4}$").expect("valid regex"),
                message: Some("Library code must be LIB-#### format".to_string()),
            }];

            registry.register("LibraryCode".to_string(), def).unwrap();

            s.custom_scalars = registry;
            s
        };

        let value = serde_json::json!("LIB-1234");
        let result = validate_custom_scalar_from_schema(&value, "LibraryCode", &schema);
        result.unwrap_or_else(|e| panic!("expected Ok for valid LibraryCode: {e}"));
    }

    #[test]
    fn test_validate_custom_scalar_library_code_invalid() {
        use crate::{
            schema::CompiledSchema,
            validation::{CustomTypeDef, CustomTypeRegistry, CustomTypeRegistryConfig},
        };

        let schema = {
            let mut s = CompiledSchema::new();
            let registry = CustomTypeRegistry::new(CustomTypeRegistryConfig::default());

            let mut def = CustomTypeDef::new("LibraryCode".to_string());
            def.validation_rules = vec![ValidationRule::Pattern {
                pattern: CompiledPattern::new(r"^LIB-[0-9]{4}$").expect("valid regex"),
                message: Some("Library code must be LIB-#### format".to_string()),
            }];

            registry.register("LibraryCode".to_string(), def).unwrap();

            s.custom_scalars = registry;
            s
        };

        let value = serde_json::json!("INVALID");
        let result = validate_custom_scalar_from_schema(&value, "LibraryCode", &schema);
        assert!(
            matches!(result, Err(FraiseQLError::Validation { .. })),
            "expected Validation error for invalid LibraryCode, got: {result:?}"
        );
    }

    #[test]
    fn test_validate_custom_scalar_student_id_with_length() {
        use crate::{
            schema::CompiledSchema,
            validation::{CustomTypeDef, CustomTypeRegistry, CustomTypeRegistryConfig},
        };

        let schema = {
            let mut s = CompiledSchema::new();
            let registry = CustomTypeRegistry::new(CustomTypeRegistryConfig::default());

            let mut def = CustomTypeDef::new("StudentID".to_string());
            def.validation_rules = vec![
                ValidationRule::Pattern {
                    pattern: CompiledPattern::new(r"^STU-[0-9]{4}-[0-9]{3}$").expect("valid regex"),
                    message: None,
                },
                ValidationRule::Length {
                    min: Some(12),
                    max: Some(12),
                },
            ];

            registry.register("StudentID".to_string(), def).unwrap();

            s.custom_scalars = registry;
            s
        };

        // Valid: matches pattern and length
        let value = serde_json::json!("STU-2024-001");
        let result = validate_custom_scalar_from_schema(&value, "StudentID", &schema);
        result.unwrap_or_else(|e| panic!("expected Ok for valid StudentID: {e}"));

        // Invalid: wrong pattern
        let value = serde_json::json!("STUDENT-2024");
        let result = validate_custom_scalar_from_schema(&value, "StudentID", &schema);
        assert!(
            matches!(result, Err(FraiseQLError::Validation { .. })),
            "expected Validation error for invalid StudentID, got: {result:?}"
        );
    }

    #[test]
    fn test_validate_unknown_scalar_type_passthrough() {
        use crate::schema::CompiledSchema;

        let schema = CompiledSchema::new();

        // Unknown scalar types should pass through (they're built-in types)
        let value = serde_json::json!("any value");
        let result = validate_custom_scalar_from_schema(&value, "UnknownType", &schema);
        result.unwrap_or_else(|e| panic!("expected Ok for unknown scalar passthrough: {e}"));
    }

    #[test]
    fn test_validate_custom_scalar_patient_id_passthrough() {
        use crate::schema::CompiledSchema;

        // Schema without PatientID definition
        let schema = CompiledSchema::new();

        let value = serde_json::json!("PAT-123456");
        let result = validate_custom_scalar_from_schema(&value, "PatientID", &schema);
        // Should pass through (not registered as custom scalar)
        result
            .unwrap_or_else(|e| panic!("expected Ok for unregistered PatientID passthrough: {e}"));
    }

    #[test]
    fn test_validate_custom_scalar_with_elo_expression() {
        use crate::{
            schema::CompiledSchema,
            validation::{CustomTypeDef, CustomTypeRegistry, CustomTypeRegistryConfig},
        };

        let schema = {
            let mut s = CompiledSchema::new();
            let registry = CustomTypeRegistry::new(CustomTypeRegistryConfig::default());

            let mut def = CustomTypeDef::new("StudentID".to_string());
            def.elo_expression = Some("matches(value, \"^STU-[0-9]{4}-[0-9]{3}$\")".to_string());

            registry.register("StudentID".to_string(), def).unwrap();

            s.custom_scalars = registry;
            s
        };

        // Valid: matches ELO expression
        let value = serde_json::json!("STU-2024-001");
        let result = validate_custom_scalar_from_schema(&value, "StudentID", &schema);
        result.unwrap_or_else(|e| panic!("expected Ok for StudentID matching ELO expression: {e}"));

        // Invalid: doesn't match ELO expression
        let value = serde_json::json!("INVALID");
        let result = validate_custom_scalar_from_schema(&value, "StudentID", &schema);
        assert!(
            matches!(result, Err(FraiseQLError::Validation { .. })),
            "expected Validation error for StudentID not matching ELO expression, got: {result:?}"
        );
    }

    #[test]
    fn test_validate_custom_scalar_combined_rules_and_elo() {
        use crate::{
            schema::CompiledSchema,
            validation::{CustomTypeDef, CustomTypeRegistry, CustomTypeRegistryConfig},
        };

        let schema = {
            let mut s = CompiledSchema::new();
            let registry = CustomTypeRegistry::new(CustomTypeRegistryConfig::default());

            let mut def = CustomTypeDef::new("PatientID".to_string());
            def.validation_rules = vec![ValidationRule::Length {
                min: Some(10),
                max: Some(10),
            }];
            def.elo_expression = Some("matches(value, \"^PAT-[0-9]{6}$\")".to_string());

            registry.register("PatientID".to_string(), def).unwrap();

            s.custom_scalars = registry;
            s
        };

        // Valid: passes both length rule and ELO expression
        let value = serde_json::json!("PAT-123456");
        let result = validate_custom_scalar_from_schema(&value, "PatientID", &schema);
        result.unwrap_or_else(|e| panic!("expected Ok for valid PatientID: {e}"));

        // Invalid: passes length but fails ELO expression
        let value = serde_json::json!("NOTVALID!");
        let result = validate_custom_scalar_from_schema(&value, "PatientID", &schema);
        assert!(
            matches!(result, Err(FraiseQLError::Validation { .. })),
            "expected Validation error for PatientID failing ELO expression, got: {result:?}"
        );

        // Invalid: fails length rule
        let value = serde_json::json!("PAT-12345");
        let result = validate_custom_scalar_from_schema(&value, "PatientID", &schema);
        assert!(
            matches!(result, Err(FraiseQLError::Validation { .. })),
            "expected Validation error for PatientID failing length rule, got: {result:?}"
        );
    }
}

mod jsonb_strategy_tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

    use crate::runtime::jsonb_strategy::{JsonbOptimizationOptions, JsonbStrategy};

    // ========================================================================
    // Strategy Parsing Tests
    // ========================================================================

    #[test]
    fn test_jsonb_strategy_from_str_project() {
        let strategy: JsonbStrategy = "project".parse().unwrap();
        assert_eq!(strategy, JsonbStrategy::Project);
    }

    #[test]
    fn test_jsonb_strategy_from_str_stream() {
        let strategy: JsonbStrategy = "stream".parse().unwrap();
        assert_eq!(strategy, JsonbStrategy::Stream);
    }

    #[test]
    fn test_jsonb_strategy_from_str_case_insensitive() {
        assert_eq!("PROJECT".parse::<JsonbStrategy>().unwrap(), JsonbStrategy::Project);
        assert_eq!("Stream".parse::<JsonbStrategy>().unwrap(), JsonbStrategy::Stream);
        assert_eq!("pRoJeCt".parse::<JsonbStrategy>().unwrap(), JsonbStrategy::Project);
    }

    #[test]
    fn test_jsonb_strategy_from_str_invalid() {
        let result = "invalid".parse::<JsonbStrategy>();
        let err = result.expect_err("expected Err for invalid JSONB strategy string");
        assert!(err.contains("Invalid JSONB strategy"), "unexpected error message: {err}");
    }

    #[test]
    fn test_jsonb_strategy_deserialize() {
        let json = r#""project""#;
        let strategy: JsonbStrategy = serde_json::from_str(json).unwrap();
        assert_eq!(strategy, JsonbStrategy::Project);
    }

    #[test]
    fn test_jsonb_strategy_deserialize_stream() {
        let json = r#""stream""#;
        let strategy: JsonbStrategy = serde_json::from_str(json).unwrap();
        assert_eq!(strategy, JsonbStrategy::Stream);
    }

    // ========================================================================
    // Strategy Selection Tests
    // ========================================================================

    #[test]
    fn test_choose_strategy_below_threshold() {
        let opts = JsonbOptimizationOptions {
            default_strategy:       JsonbStrategy::Project,
            auto_threshold_percent: 80,
        };

        let strategy = opts.choose_strategy(5, 10);
        assert_eq!(strategy, JsonbStrategy::Project);
    }

    #[test]
    fn test_choose_strategy_at_threshold() {
        let opts = JsonbOptimizationOptions {
            default_strategy:       JsonbStrategy::Project,
            auto_threshold_percent: 80,
        };

        let strategy = opts.choose_strategy(8, 10);
        assert_eq!(strategy, JsonbStrategy::Stream);
    }

    #[test]
    fn test_choose_strategy_above_threshold() {
        let opts = JsonbOptimizationOptions {
            default_strategy:       JsonbStrategy::Project,
            auto_threshold_percent: 80,
        };

        let strategy = opts.choose_strategy(9, 10);
        assert_eq!(strategy, JsonbStrategy::Stream);
    }

    #[test]
    fn test_choose_strategy_respects_default() {
        let opts = JsonbOptimizationOptions {
            default_strategy:       JsonbStrategy::Stream,
            auto_threshold_percent: 80,
        };

        let strategy = opts.choose_strategy(2, 10);
        assert_eq!(strategy, JsonbStrategy::Stream);
    }

    #[test]
    fn test_choose_strategy_zero_total() {
        let opts = JsonbOptimizationOptions::default();
        let strategy = opts.choose_strategy(0, 0);
        assert_eq!(strategy, JsonbStrategy::Project);
    }
}

mod matcher_tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
    use std::collections::HashMap;

    use indexmap::IndexMap;

    use crate::{
        error::FraiseQLError,
        runtime::matcher::*,
        schema::{CompiledSchema, CursorType, QueryDefinition},
    };

    fn test_schema() -> CompiledSchema {
        let mut schema = CompiledSchema::new();
        schema.queries.push(QueryDefinition {
            name:                "users".to_string(),
            return_type:         "User".to_string(),
            returns_list:        true,
            nullable:            false,
            arguments:           Vec::new(),
            sql_source:          Some("v_user".to_string()),
            description:         None,
            auto_params:         crate::schema::AutoParams::default(),
            deprecation:         None,
            jsonb_column:        "data".to_string(),
            relay:               false,
            relay_cursor_column: None,
            relay_cursor_type:   CursorType::default(),
            inject_params:       IndexMap::default(),
            cache_ttl_seconds:   None,
            additional_views:    vec![],
            requires_role:       None,
            rest_path:           None,
            rest_method:         None,
            native_columns:      HashMap::new(),
        });
        schema
    }

    #[test]
    fn test_matcher_new() {
        let schema = test_schema();
        let matcher = QueryMatcher::new(schema);
        assert_eq!(matcher.schema().queries.len(), 1);
    }

    #[test]
    fn test_match_simple_query() {
        let schema = test_schema();
        let matcher = QueryMatcher::new(schema);

        let query = "{ users { id name } }";
        let result = matcher.match_query(query, None).unwrap();

        assert_eq!(result.query_def.name, "users");
        assert_eq!(result.fields.len(), 1); // "users" is the root field
        assert!(result.selections[0].nested_fields.len() >= 2); // id, name
    }

    #[test]
    fn test_match_query_with_operation_name() {
        let schema = test_schema();
        let matcher = QueryMatcher::new(schema);

        let query = "query GetUsers { users { id name } }";
        let result = matcher.match_query(query, None).unwrap();

        assert_eq!(result.query_def.name, "users");
        assert_eq!(result.operation_name, Some("GetUsers".to_string()));
    }

    #[test]
    fn test_match_query_with_fragment() {
        let schema = test_schema();
        let matcher = QueryMatcher::new(schema);

        let query = r"
            fragment UserFields on User {
                id
                name
            }
            query { users { ...UserFields } }
        ";
        let result = matcher.match_query(query, None).unwrap();

        assert_eq!(result.query_def.name, "users");
        // Fragment should be resolved - nested fields should contain id, name
        let root_selection = &result.selections[0];
        assert!(root_selection.nested_fields.iter().any(|f| f.name == "id"));
        assert!(root_selection.nested_fields.iter().any(|f| f.name == "name"));
    }

    #[test]
    fn test_match_query_with_skip_directive() {
        let schema = test_schema();
        let matcher = QueryMatcher::new(schema);

        let query = r"{ users { id name @skip(if: true) } }";
        let result = matcher.match_query(query, None).unwrap();

        assert_eq!(result.query_def.name, "users");
        // "name" should be skipped due to @skip(if: true)
        let root_selection = &result.selections[0];
        assert!(root_selection.nested_fields.iter().any(|f| f.name == "id"));
        assert!(!root_selection.nested_fields.iter().any(|f| f.name == "name"));
    }

    #[test]
    fn test_match_query_with_include_directive_variable() {
        let schema = test_schema();
        let matcher = QueryMatcher::new(schema);

        let query =
            r"query($includeEmail: Boolean!) { users { id email @include(if: $includeEmail) } }";
        let variables = serde_json::json!({ "includeEmail": false });
        let result = matcher.match_query(query, Some(&variables)).unwrap();

        assert_eq!(result.query_def.name, "users");
        // "email" should be excluded because $includeEmail is false
        let root_selection = &result.selections[0];
        assert!(root_selection.nested_fields.iter().any(|f| f.name == "id"));
        assert!(!root_selection.nested_fields.iter().any(|f| f.name == "email"));
    }

    #[test]
    fn test_match_query_unknown_query() {
        let schema = test_schema();
        let matcher = QueryMatcher::new(schema);

        let query = "{ unknown { id } }";
        let result = matcher.match_query(query, None);

        assert!(
            matches!(result, Err(FraiseQLError::Validation { .. })),
            "expected Validation error for unknown query, got: {result:?}"
        );
    }

    #[test]
    fn test_extract_arguments_none() {
        let _schema = test_schema();

        let args = QueryMatcher::extract_arguments(None);
        assert!(args.is_empty());
    }

    #[test]
    fn test_extract_arguments_some() {
        let _schema = test_schema();

        let variables = serde_json::json!({
            "id": "123",
            "limit": 10
        });

        let args = QueryMatcher::extract_arguments(Some(&variables));
        assert_eq!(args.len(), 2);
        assert_eq!(args.get("id"), Some(&serde_json::json!("123")));
        assert_eq!(args.get("limit"), Some(&serde_json::json!(10)));
    }

    // =========================================================================
    // suggest_similar / levenshtein tests
    // =========================================================================

    #[test]
    fn test_suggest_similar_exact_typo() {
        let suggestions = suggest_similar("userr", &["users", "posts", "comments"]);
        assert_eq!(suggestions, vec!["users"]);
    }

    #[test]
    fn test_suggest_similar_transposition() {
        let suggestions = suggest_similar("suers", &["users", "posts"]);
        assert_eq!(suggestions, vec!["users"]);
    }

    #[test]
    fn test_suggest_similar_no_match() {
        // "zzz" is far from everything — no suggestion expected.
        let suggestions = suggest_similar("zzz", &["users", "posts", "comments"]);
        assert!(suggestions.is_empty());
    }

    #[test]
    fn test_suggest_similar_capped_at_three() {
        // All four candidates are within distance 2 of "us".
        let suggestions =
            suggest_similar("us", &["users", "user", "uses", "usher", "something_far"]);
        assert!(suggestions.len() <= 3);
    }

    #[test]
    fn test_levenshtein_identical() {
        assert_eq!(levenshtein("foo", "foo"), 0);
    }

    #[test]
    fn test_levenshtein_insertion() {
        assert_eq!(levenshtein("foo", "fooo"), 1);
    }

    #[test]
    fn test_levenshtein_deletion() {
        assert_eq!(levenshtein("fooo", "foo"), 1);
    }

    #[test]
    fn test_levenshtein_substitution() {
        assert_eq!(levenshtein("foo", "bar"), 3);
    }

    #[test]
    fn test_uzer_typo_suggests_user() {
        let mut schema = CompiledSchema::new();
        schema.queries.push(QueryDefinition {
            name:                "user".to_string(),
            return_type:         "User".to_string(),
            returns_list:        false,
            nullable:            true,
            arguments:           Vec::new(),
            sql_source:          Some("v_user".to_string()),
            description:         None,
            auto_params:         crate::schema::AutoParams::default(),
            deprecation:         None,
            jsonb_column:        "data".to_string(),
            relay:               false,
            relay_cursor_column: None,
            relay_cursor_type:   CursorType::default(),
            inject_params:       IndexMap::default(),
            cache_ttl_seconds:   None,
            additional_views:    vec![],
            requires_role:       None,
            rest_path:           None,
            rest_method:         None,
            native_columns:      HashMap::new(),
        });
        let matcher = QueryMatcher::new(schema);

        // "uzer" is one edit away from "user" — should suggest it.
        let result = matcher.match_query("{ uzer { id } }", None);
        let err = result.expect_err("expected Err for typo'd query name");
        let msg = err.to_string();
        assert!(msg.contains("Did you mean"), "expected 'Did you mean' suggestion in: {msg}");
    }

    #[test]
    fn test_unknown_query_error_includes_suggestion() {
        let mut schema = CompiledSchema::new();
        schema.queries.push(QueryDefinition {
            name:                "users".to_string(),
            return_type:         "User".to_string(),
            returns_list:        true,
            nullable:            false,
            arguments:           Vec::new(),
            sql_source:          Some("v_user".to_string()),
            description:         None,
            auto_params:         crate::schema::AutoParams::default(),
            deprecation:         None,
            jsonb_column:        "data".to_string(),
            relay:               false,
            relay_cursor_column: None,
            relay_cursor_type:   CursorType::default(),
            inject_params:       IndexMap::default(),
            cache_ttl_seconds:   None,
            additional_views:    vec![],
            requires_role:       None,
            rest_path:           None,
            rest_method:         None,
            native_columns:      HashMap::new(),
        });
        let matcher = QueryMatcher::new(schema);

        // "userr" is one edit away from "users" — should suggest it.
        let result = matcher.match_query("{ userr { id } }", None);
        let err = result.expect_err("expected Err for typo'd query name");
        let msg = err.to_string();
        assert!(msg.contains("Did you mean 'users'?"), "expected suggestion in: {msg}");
    }

    // =========================================================================
    // resolve_inline_arg tests (C11)
    // =========================================================================

    #[test]
    fn test_resolve_inline_arg_literal_integer() {
        let arg = crate::graphql::GraphQLArgument {
            name:       "limit".to_string(),
            value_json: "3".to_string(),
            value_type: "int".to_string(),
        };
        let vars = HashMap::new();
        let result = QueryMatcher::resolve_inline_arg(&arg, &vars);
        assert_eq!(result, Some(serde_json::json!(3)));
    }

    #[test]
    fn test_resolve_inline_arg_literal_string() {
        let arg = crate::graphql::GraphQLArgument {
            name:       "status".to_string(),
            value_json: "\"active\"".to_string(),
            value_type: "string".to_string(),
        };
        let vars = HashMap::new();
        let result = QueryMatcher::resolve_inline_arg(&arg, &vars);
        assert_eq!(result, Some(serde_json::json!("active")));
    }

    #[test]
    fn test_resolve_inline_arg_literal_boolean() {
        let arg = crate::graphql::GraphQLArgument {
            name:       "active".to_string(),
            value_json: "true".to_string(),
            value_type: "boolean".to_string(),
        };
        let vars = HashMap::new();
        let result = QueryMatcher::resolve_inline_arg(&arg, &vars);
        assert_eq!(result, Some(serde_json::json!(true)));
    }

    #[test]
    fn test_resolve_inline_arg_literal_null() {
        let arg = crate::graphql::GraphQLArgument {
            name:       "limit".to_string(),
            value_json: "null".to_string(),
            value_type: "null".to_string(),
        };
        let vars = HashMap::new();
        let result = QueryMatcher::resolve_inline_arg(&arg, &vars);
        assert_eq!(result, Some(serde_json::Value::Null));
    }

    #[test]
    fn test_resolve_inline_arg_variable_reference_json_quoted() {
        // Parser serializes Variable("myLimit") as "\"$myLimit\""
        let arg = crate::graphql::GraphQLArgument {
            name:       "limit".to_string(),
            value_json: "\"$myLimit\"".to_string(),
            value_type: "variable".to_string(),
        };
        let mut vars = HashMap::new();
        vars.insert("myLimit".to_string(), serde_json::json!(5));
        let result = QueryMatcher::resolve_inline_arg(&arg, &vars);
        assert_eq!(result, Some(serde_json::json!(5)));
    }

    #[test]
    fn test_resolve_inline_arg_variable_reference_raw() {
        // Defensive: unquoted $var format
        let arg = crate::graphql::GraphQLArgument {
            name:       "limit".to_string(),
            value_json: "$limit".to_string(),
            value_type: "variable".to_string(),
        };
        let mut vars = HashMap::new();
        vars.insert("limit".to_string(), serde_json::json!(10));
        let result = QueryMatcher::resolve_inline_arg(&arg, &vars);
        assert_eq!(result, Some(serde_json::json!(10)));
    }

    #[test]
    fn test_resolve_inline_arg_variable_not_found() {
        let arg = crate::graphql::GraphQLArgument {
            name:       "limit".to_string(),
            value_json: "\"$missing\"".to_string(),
            value_type: "variable".to_string(),
        };
        let vars = HashMap::new();
        let result = QueryMatcher::resolve_inline_arg(&arg, &vars);
        assert_eq!(result, None);
    }

    #[test]
    fn test_resolve_inline_arg_object() {
        let arg = crate::graphql::GraphQLArgument {
            name:       "where".to_string(),
            value_json: r#"{"status":{"eq":"active"}}"#.to_string(),
            value_type: "object".to_string(),
        };
        let vars = HashMap::new();
        let result = QueryMatcher::resolve_inline_arg(&arg, &vars);
        assert_eq!(result, Some(serde_json::json!({"status": {"eq": "active"}})));
    }

    #[test]
    fn test_resolve_inline_arg_list() {
        let arg = crate::graphql::GraphQLArgument {
            name:       "ids".to_string(),
            value_json: "[1,2,3]".to_string(),
            value_type: "list".to_string(),
        };
        let vars = HashMap::new();
        let result = QueryMatcher::resolve_inline_arg(&arg, &vars);
        assert_eq!(result, Some(serde_json::json!([1, 2, 3])));
    }

    // resolve_inline_arg: variables NESTED inside object/list literals
    // ------------------------------------------------------------------------
    // `where: { field: { eq: $v } }` / `input: { f: $v }` — the parser serializes
    // each nested Variable("v") to the JSON string "$v", so they must be
    // substituted at any depth (not only whole-argument `field: $var`). Without the
    // fix these placeholders reached SQL/coercion verbatim (filters matched
    // nothing; inline mutation inputs surfaced as missing required arguments).

    #[test]
    fn test_resolve_inline_arg_nested_variable_in_object() {
        // where: { status: { eq: $v } }
        let arg = crate::graphql::GraphQLArgument {
            name:       "where".to_string(),
            value_json: r#"{"status":{"eq":"$v"}}"#.to_string(),
            value_type: "object".to_string(),
        };
        let mut vars = HashMap::new();
        vars.insert("v".to_string(), serde_json::json!("active"));
        let result = QueryMatcher::resolve_inline_arg(&arg, &vars);
        assert_eq!(result, Some(serde_json::json!({"status": {"eq": "active"}})));
    }

    #[test]
    fn test_resolve_inline_arg_nested_variable_in_list() {
        // ids: [$a, 2, $b]
        let arg = crate::graphql::GraphQLArgument {
            name:       "ids".to_string(),
            value_json: r#"["$a",2,"$b"]"#.to_string(),
            value_type: "list".to_string(),
        };
        let mut vars = HashMap::new();
        vars.insert("a".to_string(), serde_json::json!(1));
        vars.insert("b".to_string(), serde_json::json!(3));
        let result = QueryMatcher::resolve_inline_arg(&arg, &vars);
        assert_eq!(result, Some(serde_json::json!([1, 2, 3])));
    }

    #[test]
    fn test_resolve_inline_arg_nested_variable_in_mutation_input() {
        // createMachine(input: { name: $n, count: $c })
        let arg = crate::graphql::GraphQLArgument {
            name:       "input".to_string(),
            value_json: r#"{"name":"$n","count":"$c"}"#.to_string(),
            value_type: "object".to_string(),
        };
        let mut vars = HashMap::new();
        vars.insert("n".to_string(), serde_json::json!("widget"));
        vars.insert("c".to_string(), serde_json::json!(7));
        let result = QueryMatcher::resolve_inline_arg(&arg, &vars);
        assert_eq!(result, Some(serde_json::json!({"name": "widget", "count": 7})));
    }

    #[test]
    fn test_resolve_inline_arg_nested_unknown_variable_is_null() {
        // where: { f: { eq: $missing } } with no such variable → null
        // (GraphQL's treatment of an omitted nullable; not a verbatim "$missing").
        let arg = crate::graphql::GraphQLArgument {
            name:       "where".to_string(),
            value_json: r#"{"f":{"eq":"$missing"}}"#.to_string(),
            value_type: "object".to_string(),
        };
        let vars = HashMap::new();
        let result = QueryMatcher::resolve_inline_arg(&arg, &vars);
        assert_eq!(result, Some(serde_json::json!({"f": {"eq": serde_json::Value::Null}})));
    }

    #[test]
    fn test_resolve_inline_arg_nested_plain_strings_preserved() {
        // Non-`$` strings nested in a literal must pass through unchanged.
        let arg = crate::graphql::GraphQLArgument {
            name:       "where".to_string(),
            value_json: r#"{"status":{"eq":"active"},"tags":["a","b"]}"#.to_string(),
            value_type: "object".to_string(),
        };
        let vars = HashMap::new();
        let result = QueryMatcher::resolve_inline_arg(&arg, &vars);
        assert_eq!(
            result,
            Some(serde_json::json!({"status": {"eq": "active"}, "tags": ["a", "b"]}))
        );
    }
}

mod runtime_mod_tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
    use crate::runtime::*;

    #[test]
    fn test_default_config() {
        let config = RuntimeConfig::default();
        assert!(config.cache_query_plans);
        assert_eq!(config.max_query_depth, 10);
        assert_eq!(config.max_query_complexity, 1000);
        assert!(!config.enable_tracing);
    }
}

mod mutation_result_tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
    use std::collections::HashMap;

    use serde_json::{Value as JsonValue, json};

    use crate::{
        error::{FraiseQLError, Result},
        runtime::{cascade::MutationErrorClass, mutation_result::*},
    };

    /// Terse builder for constructing row fixtures in tests.
    #[derive(Default)]
    struct Row(HashMap<String, JsonValue>);

    impl Row {
        fn new(succeeded: bool, state_changed: bool) -> Self {
            let mut r = Self::default();
            r.0.insert("succeeded".into(), json!(succeeded));
            r.0.insert("state_changed".into(), json!(state_changed));
            r
        }

        fn with(mut self, key: &str, value: JsonValue) -> Self {
            self.0.insert(key.into(), value);
            self
        }

        fn parse(&self) -> Result<MutationOutcome> {
            parse_mutation_row(&self.0)
        }
    }

    // ── Deserialization ────────────────────────────────────────────────────

    #[test]
    fn deserializes_all_columns() {
        let eid = "550e8400-e29b-41d4-a716-446655440000";
        let mut row = HashMap::new();
        row.insert("succeeded".to_string(), json!(false));
        row.insert("state_changed".to_string(), json!(false));
        row.insert("error_class".to_string(), json!("validation"));
        row.insert("status_detail".to_string(), json!("duplicate_email"));
        row.insert("http_status".to_string(), json!(422));
        row.insert("message".to_string(), json!("email already in use"));
        row.insert("entity_id".to_string(), json!(eid));
        row.insert("entity_type".to_string(), json!("User"));
        row.insert("entity".to_string(), json!({"id": eid}));
        row.insert("updated_fields".to_string(), json!(["email"]));
        row.insert("cascade".to_string(), json!({}));
        row.insert("error_detail".to_string(), json!({"field": "email"}));
        row.insert("metadata".to_string(), json!({"trace_id": "abc"}));

        let obj: serde_json::Map<String, JsonValue> = row.into_iter().collect();
        let parsed: MutationResponse = serde_json::from_value(JsonValue::Object(obj)).unwrap();

        assert!(!parsed.succeeded);
        assert!(!parsed.state_changed);
        assert_eq!(parsed.error_class, Some(MutationErrorClass::Validation));
        assert_eq!(parsed.status_detail.as_deref(), Some("duplicate_email"));
        assert_eq!(parsed.http_status, Some(422));
        assert_eq!(parsed.message.as_deref(), Some("email already in use"));
        assert_eq!(parsed.entity_id.map(|u| u.to_string()).as_deref(), Some(eid));
        assert_eq!(parsed.entity_type.as_deref(), Some("User"));
        assert_eq!(parsed.updated_fields, vec!["email".to_string()]);
        assert_eq!(parsed.error_detail["field"], "email");
        assert_eq!(parsed.metadata["trace_id"], "abc");
    }

    #[test]
    fn defaults_missing_jsonb_columns_to_null() {
        let parsed: MutationResponse = serde_json::from_value(json!({
            "succeeded": true,
            "state_changed": false,
        }))
        .unwrap();
        assert!(parsed.entity.is_null());
        assert!(parsed.cascade.is_null());
        assert!(parsed.error_detail.is_null());
        assert!(parsed.metadata.is_null());
        assert!(parsed.updated_fields.is_empty());
        assert!(parsed.entity_id.is_none());
    }

    /// A `mutation_response` row whose `updated_fields` column is SQL NULL — the
    /// natural state for a failure branch that never assigns it, rendered by
    /// `row_to_map` as JSON `null` — must parse as an empty list, not fail with
    /// "invalid type: null, expected a sequence". `#[serde(default)]` only covers an
    /// *absent* key; an explicit null still routes to `Vec<String>`'s deserializer,
    /// which rejects it — so a real failed mutation surfaced as an opaque parse error
    /// instead of the typed error arm (#473).
    #[test]
    fn null_updated_fields_parses_as_empty() {
        // Failure path: a function that doesn't assign updated_fields leaves it NULL.
        let outcome = Row::new(false, false)
            .with("error_class", json!("not_found"))
            .with("message", json!("absent"))
            .with("updated_fields", JsonValue::Null)
            .parse()
            .expect("null updated_fields must not fail to deserialize on the error path");
        assert!(
            matches!(outcome, MutationOutcome::Error { .. }),
            "a failed row with null updated_fields must still route to the error outcome"
        );

        // Success path: an explicit-null updated_fields surfaces as an empty list.
        let outcome = Row::new(true, true)
            .with("entity", json!({"id": "x"}))
            .with("updated_fields", JsonValue::Null)
            .parse()
            .expect("null updated_fields must not fail to deserialize on the success path");
        match outcome {
            MutationOutcome::Success { updated_fields, .. } => {
                assert!(
                    updated_fields.is_empty(),
                    "null updated_fields must surface as an empty list"
                );
            },
            MutationOutcome::Error { .. } => panic!("expected Success"),
        }
    }

    // ── Semantics table ────────────────────────────────────────────────────

    #[test]
    fn semantics_success_state_changed_true() {
        let entity = json!({"id": "x"});
        let outcome = Row::new(true, true)
            .with("entity", entity.clone())
            .with("entity_type", json!("Machine"))
            .parse()
            .unwrap();
        match outcome {
            MutationOutcome::Success {
                entity: e,
                entity_type,
                entity_id,
                cascade,
                updated_fields,
            } => {
                assert_eq!(e, entity);
                assert_eq!(entity_type.as_deref(), Some("Machine"));
                assert!(entity_id.is_none());
                assert!(cascade.is_none());
                assert!(updated_fields.is_empty(), "no updated_fields column set → empty");
            },
            MutationOutcome::Error { .. } => panic!("expected Success"),
        }
    }

    #[test]
    fn semantics_success_noop() {
        let entity = json!({"id": "x", "name": "current"});
        let outcome = Row::new(true, false).with("entity", entity.clone()).parse().unwrap();
        match outcome {
            MutationOutcome::Success { entity: e, .. } => assert_eq!(e, entity),
            MutationOutcome::Error { .. } => panic!("expected Success (noop)"),
        }
    }

    #[test]
    fn semantics_error_routes_to_error_outcome() {
        let outcome = Row::new(false, false)
            .with("error_class", json!("conflict"))
            .with("message", json!("duplicate"))
            .with("http_status", json!(409))
            .with("error_detail", json!({"field": "email"}))
            .with("metadata", json!({"trace_id": "zzz"}))
            .parse()
            .unwrap();
        match outcome {
            MutationOutcome::Error {
                error_class,
                message,
                http_status,
                metadata,
                ..
            } => {
                assert_eq!(error_class, MutationErrorClass::Conflict);
                assert_eq!(message, "duplicate");
                assert_eq!(http_status, Some(409));
                // error_detail (not metadata) feeds the error-field projection.
                assert_eq!(metadata, json!({"field": "email"}));
            },
            MutationOutcome::Success { .. } => panic!("expected Error"),
        }
    }

    #[test]
    fn semantics_illegal_partial_failure_rejected() {
        let err = Row::new(false, true)
            .with("error_class", json!("internal"))
            .parse()
            .expect_err("partial failure must be rejected");
        match err {
            FraiseQLError::Validation { message, .. } => {
                assert!(message.contains("state_changed=true is illegal"), "got: {message}");
            },
            other => panic!("expected Validation error, got {other:?}"),
        }
    }

    #[test]
    fn error_requires_error_class() {
        let err = Row::new(false, false)
            .parse()
            .expect_err("error row without error_class must be rejected");
        assert!(matches!(err, FraiseQLError::Validation { .. }));
    }

    #[test]
    fn success_rejects_error_class() {
        let err = Row::new(true, true)
            .with("error_class", json!("validation"))
            .parse()
            .expect_err("succeeded=true with error_class must be rejected");
        assert!(matches!(err, FraiseQLError::Validation { .. }));
    }

    #[test]
    fn http_status_range_enforced() {
        let err = Row::new(true, false)
            .with("http_status", json!(42))
            .parse()
            .expect_err("http_status out of range must be rejected");
        match err {
            FraiseQLError::Validation { message, .. } => {
                assert!(message.contains("http_status"), "got: {message}");
            },
            other => panic!("expected Validation error, got {other:?}"),
        }
    }

    #[test]
    fn http_status_boundaries_accepted() {
        for code in [100_i16, 200, 422, 599] {
            Row::new(true, false)
                .with("http_status", json!(code))
                .parse()
                .unwrap_or_else(|e| panic!("code {code} should be accepted: {e:?}"));
        }
    }

    #[test]
    fn as_str_round_trips_all_error_classes() {
        let cases = [
            (MutationErrorClass::Validation, "validation"),
            (MutationErrorClass::Conflict, "conflict"),
            (MutationErrorClass::NotFound, "not_found"),
            (MutationErrorClass::Unauthorized, "unauthorized"),
            (MutationErrorClass::Forbidden, "forbidden"),
            (MutationErrorClass::Internal, "internal"),
            (MutationErrorClass::TransactionFailed, "transaction_failed"),
            (MutationErrorClass::Timeout, "timeout"),
            (MutationErrorClass::RateLimited, "rate_limited"),
            (MutationErrorClass::ServiceUnavailable, "service_unavailable"),
        ];
        for (class, expected) in cases {
            assert_eq!(class.as_str(), expected, "class = {class:?}");
        }
    }

    #[test]
    fn entity_id_uuid_serialized_back_to_canonical_string() {
        let eid = "550e8400-e29b-41d4-a716-446655440000";
        let outcome = Row::new(true, true)
            .with("entity_id", json!(eid))
            .with("entity", json!({"id": eid}))
            .parse()
            .unwrap();
        match outcome {
            MutationOutcome::Success { entity_id, .. } => {
                assert_eq!(entity_id.as_deref(), Some(eid));
            },
            MutationOutcome::Error { .. } => panic!("expected Success"),
        }
    }

    #[test]
    fn extra_columns_ignored() {
        // Rows may contain columns the parser doesn't know about (e.g. schema_version
        // from older DB functions). These must be silently ignored.
        let outcome = Row::new(true, true)
            .with("entity", json!({"id": "1"}))
            .with("schema_version", json!(2))
            .with("some_future_column", json!("whatever"))
            .parse()
            .unwrap();
        assert!(matches!(outcome, MutationOutcome::Success { .. }));
    }
}

mod planner_tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
    use std::collections::HashMap;

    use indexmap::IndexMap;

    use crate::{
        graphql::{FieldSelection, ParsedQuery},
        runtime::{JsonbOptimizationOptions, JsonbStrategy, matcher::QueryMatch, planner::*},
        schema::{AutoParams, CursorType, QueryDefinition},
    };

    fn test_query_match() -> QueryMatch {
        QueryMatch {
            query_def:      QueryDefinition {
                name:                "users".to_string(),
                return_type:         "User".to_string(),
                returns_list:        true,
                nullable:            false,
                arguments:           Vec::new(),
                sql_source:          Some("v_user".to_string()),
                description:         None,
                auto_params:         AutoParams::default(),
                deprecation:         None,
                jsonb_column:        "data".to_string(),
                relay:               false,
                relay_cursor_column: None,
                relay_cursor_type:   CursorType::default(),
                inject_params:       IndexMap::default(),
                cache_ttl_seconds:   None,
                additional_views:    vec![],
                requires_role:       None,
                rest_path:           None,
                rest_method:         None,
                native_columns:      HashMap::new(),
            },
            fields:         vec!["id".to_string(), "name".to_string()],
            selections:     vec![FieldSelection {
                name:          "users".to_string(),
                alias:         None,
                arguments:     vec![],
                nested_fields: vec![
                    FieldSelection {
                        name:          "id".to_string(),
                        alias:         None,
                        arguments:     vec![],
                        nested_fields: vec![],
                        directives:    vec![],
                    },
                    FieldSelection {
                        name:          "name".to_string(),
                        alias:         None,
                        arguments:     vec![],
                        nested_fields: vec![],
                        directives:    vec![],
                    },
                ],
                directives:    vec![],
            }],
            arguments:      HashMap::new(),
            operation_name: Some("users".to_string()),
            parsed_query:   ParsedQuery {
                operation_type: "query".to_string(),
                operation_name: Some("users".to_string()),
                root_field:     "users".to_string(),
                selections:     vec![],
                variables:      vec![],
                fragments:      vec![],
                source:         std::sync::Arc::from("{ users { id name } }"),
            },
        }
    }

    #[test]
    fn test_planner_new() {
        let planner = QueryPlanner::new(true);
        assert!(planner.cache_enabled());

        let planner = QueryPlanner::new(false);
        assert!(!planner.cache_enabled());
    }

    #[test]
    fn test_generate_sql() {
        let planner = QueryPlanner::new(true);
        let query_match = test_query_match();

        let sql = planner.generate_sql(&query_match);
        assert_eq!(sql, "SELECT data FROM v_user");
    }

    #[test]
    fn test_extract_parameters() {
        let planner = QueryPlanner::new(true);
        let mut query_match = test_query_match();
        query_match.arguments.insert("id".to_string(), serde_json::json!("123"));
        query_match.arguments.insert("limit".to_string(), serde_json::json!(10));

        let params = planner.extract_parameters(&query_match);
        assert_eq!(params.len(), 2);
    }

    #[test]
    fn test_estimate_cost() {
        let planner = QueryPlanner::new(true);
        let query_match = test_query_match();

        let cost = planner.estimate_cost(&query_match);
        // base (100) + 2 fields (20) + 0 args (0) = 120
        assert_eq!(cost, 120);
    }

    #[test]
    fn test_plan() {
        let planner = QueryPlanner::new(true);
        let query_match = test_query_match();

        let plan = planner.plan(&query_match).unwrap();
        assert!(!plan.sql.is_empty());
        assert_eq!(plan.projection_fields.len(), 2);
        assert!(!plan.is_cached);
        assert_eq!(plan.estimated_cost, 120);
        assert_eq!(plan.jsonb_strategy, JsonbStrategy::Project);
    }

    // ========================================================================

    // ========================================================================

    #[test]
    fn test_projection_fields_exclude_typename() {
        let planner = QueryPlanner::new(true);
        let mut query_match = test_query_match();

        // Add __typename to the nested fields of the root selection
        query_match.selections[0].nested_fields.push(FieldSelection {
            name:          "__typename".to_string(),
            alias:         None,
            arguments:     vec![],
            nested_fields: vec![],
            directives:    vec![],
        });

        let plan = planner.plan(&query_match).unwrap();

        // __typename must NOT appear in projection fields (it's a GraphQL meta-field)
        assert!(!plan.projection_fields.contains(&"__typename".to_string()));
        assert_eq!(plan.projection_fields, vec!["id".to_string(), "name".to_string()]);
    }

    #[test]
    fn test_plan_includes_jsonb_strategy() {
        let planner = QueryPlanner::new(true);
        let query_match = test_query_match();

        let plan = planner.plan(&query_match).unwrap();
        // Should include strategy in execution plan
        assert_eq!(plan.jsonb_strategy, JsonbStrategy::Project);
    }

    #[test]
    fn test_planner_always_projects_when_fields_present() {
        let custom_options = JsonbOptimizationOptions {
            default_strategy:       JsonbStrategy::Stream,
            auto_threshold_percent: 50,
        };
        let planner = QueryPlanner::with_jsonb_options(true, custom_options);
        let query_match = test_query_match();

        let plan = planner.plan(&query_match).unwrap();
        // Even with Stream default, must use Project when selections exist
        // to ensure camelCase response keys
        assert_eq!(plan.jsonb_strategy, JsonbStrategy::Project);
    }

    #[test]
    fn test_choose_jsonb_strategy_forces_project_with_fields() {
        let options = JsonbOptimizationOptions {
            default_strategy:       JsonbStrategy::Stream,
            auto_threshold_percent: 80,
        };
        let planner = QueryPlanner::with_jsonb_options(true, options);

        // Any non-empty selection set must use Project for camelCase keys
        let strategy = planner.choose_jsonb_strategy(&["id".to_string(), "name".to_string()]);
        assert_eq!(strategy, JsonbStrategy::Project);
    }

    #[test]
    fn test_choose_jsonb_strategy_forces_project_with_many_fields() {
        let options = JsonbOptimizationOptions {
            default_strategy:       JsonbStrategy::Project,
            auto_threshold_percent: 80,
        };
        let planner = QueryPlanner::with_jsonb_options(true, options);

        // Even with many fields (above old threshold), must use Project
        let many_fields = (0..9).map(|i| format!("field_{}", i)).collect::<Vec<_>>();
        let strategy = planner.choose_jsonb_strategy(&many_fields);
        assert_eq!(strategy, JsonbStrategy::Project);
    }

    #[test]
    fn test_choose_jsonb_strategy_empty_fields_uses_default() {
        let options = JsonbOptimizationOptions {
            default_strategy:       JsonbStrategy::Stream,
            auto_threshold_percent: 80,
        };
        let planner = QueryPlanner::with_jsonb_options(true, options);

        // Empty selection set falls back to default strategy
        let strategy = planner.choose_jsonb_strategy(&[]);
        assert_eq!(strategy, JsonbStrategy::Stream);
    }
}

mod projection_tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
    use serde_json::json;

    use crate::{
        db::types::JsonbValue, error::FraiseQLError, graphql::FieldSelection,
        runtime::projection::*,
    };

    #[test]
    fn test_projection_mapper_new() {
        let mapper = ProjectionMapper::new(vec!["id".to_string(), "name".to_string()]);
        assert_eq!(mapper.fields.len(), 2);
    }

    #[test]
    fn test_project_object() {
        let mapper = ProjectionMapper::new(vec!["id".to_string(), "name".to_string()]);

        let data = json!({
            "id": "123",
            "name": "Alice",
            "email": "alice@example.com"
        });

        let jsonb = JsonbValue::new(data);
        let result = mapper.project(&jsonb).unwrap();

        assert_eq!(result, json!({ "id": "123", "name": "Alice" }));
    }

    #[test]
    fn test_project_array() {
        let mapper = ProjectionMapper::new(vec!["id".to_string()]);

        let data = json!([
            { "id": "1", "name": "Alice" },
            { "id": "2", "name": "Bob" }
        ]);

        let jsonb = JsonbValue::new(data);
        let result = mapper.project(&jsonb).unwrap();

        assert_eq!(result, json!([{ "id": "1" }, { "id": "2" }]));
    }

    #[test]
    fn test_result_projector_list() {
        let projector = ResultProjector::new(vec!["id".to_string()]);

        let data = json!({ "id": "1", "name": "Alice" });
        let results = vec![JsonbValue::new(data)];
        let result = projector.project_results(&results, true).unwrap();

        assert_eq!(result, json!([{ "id": "1" }]));
    }

    #[test]
    fn test_result_projector_single() {
        let projector = ResultProjector::new(vec!["id".to_string()]);

        let data = json!({ "id": "1", "name": "Alice" });
        let results = vec![JsonbValue::new(data)];
        let result = projector.project_results(&results, false).unwrap();

        assert_eq!(result, json!({ "id": "1" }));
    }

    #[test]
    fn test_wrap_in_data_envelope() {
        let result = json!([{ "id": "1" }]);
        let wrapped = ResultProjector::wrap_in_data_envelope(result, "users");

        assert_eq!(wrapped, json!({ "data": { "users": [{ "id": "1" }] } }));
    }

    #[test]
    fn test_wrap_error() {
        let error = FraiseQLError::Validation {
            message: "Invalid query".to_string(),
            path:    None,
        };

        let wrapped = ResultProjector::wrap_error(&error);

        assert!(wrapped.get("errors").is_some());
        assert_eq!(wrapped.get("data"), None);
    }

    #[test]
    fn test_add_typename_only_object() {
        let projector = ResultProjector::new(vec!["id".to_string()]);

        let data = json!({ "id": "123", "name": "Alice" });
        let jsonb = JsonbValue::new(data);
        let result = projector.add_typename_only(&jsonb, "User").unwrap();

        assert_eq!(result, json!({ "id": "123", "name": "Alice", "__typename": "User" }));
    }

    #[test]
    fn test_add_typename_only_array() {
        let projector = ResultProjector::new(vec!["id".to_string()]);

        let data = json!([
            { "id": "1", "name": "Alice" },
            { "id": "2", "name": "Bob" }
        ]);
        let jsonb = JsonbValue::new(data);
        let result = projector.add_typename_only(&jsonb, "User").unwrap();

        assert_eq!(
            result,
            json!([
                { "id": "1", "name": "Alice", "__typename": "User" },
                { "id": "2", "name": "Bob", "__typename": "User" }
            ])
        );
    }

    #[test]
    fn test_add_typename_only_primitive() {
        let projector = ResultProjector::new(vec![]);

        let jsonb = JsonbValue::new(json!("string_value"));
        let result = projector.add_typename_only(&jsonb, "String").unwrap();

        // Primitive values are returned unchanged (cannot add __typename to string)
        assert_eq!(result, json!("string_value"));
    }

    // ========================================================================
    // Alias tests
    // ========================================================================

    #[test]
    fn test_field_mapping_simple() {
        let mapping = FieldMapping::simple("name");
        assert_eq!(mapping.source, "name");
        assert_eq!(mapping.output, "name");
    }

    #[test]
    fn test_field_mapping_aliased() {
        let mapping = FieldMapping::aliased("author", "writer");
        assert_eq!(mapping.source, "author");
        assert_eq!(mapping.output, "writer");
    }

    #[test]
    fn test_project_with_alias() {
        let mapper = ProjectionMapper::with_mappings(vec![
            FieldMapping::simple("id"),
            FieldMapping::aliased("author", "writer"),
        ]);

        let data = json!({
            "id": "123",
            "author": { "name": "Alice" },
            "title": "Hello World"
        });

        let jsonb = JsonbValue::new(data);
        let result = mapper.project(&jsonb).unwrap();

        // "author" should be output as "writer"
        assert_eq!(
            result,
            json!({
                "id": "123",
                "writer": { "name": "Alice" }
            })
        );
    }

    #[test]
    fn test_project_with_typename() {
        let mapper =
            ProjectionMapper::new(vec!["id".to_string(), "name".to_string()]).with_typename("User");

        let data = json!({
            "id": "123",
            "name": "Alice",
            "email": "alice@example.com"
        });

        let jsonb = JsonbValue::new(data);
        let result = mapper.project(&jsonb).unwrap();

        assert_eq!(
            result,
            json!({
                "__typename": "User",
                "id": "123",
                "name": "Alice"
            })
        );
    }

    #[test]
    fn test_project_with_alias_and_typename() {
        let mapper = ProjectionMapper::with_mappings(vec![
            FieldMapping::simple("id"),
            FieldMapping::aliased("author", "writer"),
        ])
        .with_typename("Post");

        let data = json!({
            "id": "post-1",
            "author": { "name": "Alice" },
            "title": "Hello"
        });

        let jsonb = JsonbValue::new(data);
        let result = mapper.project(&jsonb).unwrap();

        assert_eq!(
            result,
            json!({
                "__typename": "Post",
                "id": "post-1",
                "writer": { "name": "Alice" }
            })
        );
    }

    #[test]
    fn test_result_projector_with_typename() {
        let projector =
            ResultProjector::new(vec!["id".to_string(), "name".to_string()]).with_typename("User");

        let data = json!({ "id": "1", "name": "Alice", "email": "alice@example.com" });
        let results = vec![JsonbValue::new(data)];
        let result = projector.project_results(&results, false).unwrap();

        assert_eq!(
            result,
            json!({
                "__typename": "User",
                "id": "1",
                "name": "Alice"
            })
        );
    }

    #[test]
    fn test_result_projector_list_with_typename() {
        let projector = ResultProjector::new(vec!["id".to_string()]).with_typename("User");

        let results = vec![
            JsonbValue::new(json!({ "id": "1", "name": "Alice" })),
            JsonbValue::new(json!({ "id": "2", "name": "Bob" })),
        ];
        let result = projector.project_results(&results, true).unwrap();

        assert_eq!(
            result,
            json!([
                { "__typename": "User", "id": "1" },
                { "__typename": "User", "id": "2" }
            ])
        );
    }

    #[test]
    fn test_result_projector_with_mappings() {
        let projector = ResultProjector::with_mappings(vec![
            FieldMapping::simple("id"),
            FieldMapping::aliased("full_name", "name"),
        ]);

        let data = json!({ "id": "1", "full_name": "Alice Smith", "email": "alice@example.com" });
        let results = vec![JsonbValue::new(data)];
        let result = projector.project_results(&results, false).unwrap();

        // "full_name" should be output as "name"
        assert_eq!(
            result,
            json!({
                "id": "1",
                "name": "Alice Smith"
            })
        );
    }

    // ========================================================================
    // Nested typename tests
    // ========================================================================

    #[test]
    fn test_nested_object_typename() {
        let mapper = ProjectionMapper::with_mappings(vec![
            FieldMapping::simple("id"),
            FieldMapping::simple("title"),
            FieldMapping::nested_object(
                "author",
                "User",
                vec![FieldMapping::simple("id"), FieldMapping::simple("name")],
            ),
        ])
        .with_typename("Post");

        let data = json!({
            "id": "post-1",
            "title": "Hello World",
            "author": {
                "id": "user-1",
                "name": "Alice",
                "email": "alice@example.com"
            }
        });

        let jsonb = JsonbValue::new(data);
        let result = mapper.project(&jsonb).unwrap();

        assert_eq!(
            result,
            json!({
                "__typename": "Post",
                "id": "post-1",
                "title": "Hello World",
                "author": {
                    "__typename": "User",
                    "id": "user-1",
                    "name": "Alice"
                }
            })
        );
    }

    #[test]
    fn test_nested_array_typename() {
        let mapper = ProjectionMapper::with_mappings(vec![
            FieldMapping::simple("id"),
            FieldMapping::simple("name"),
            FieldMapping::nested_object(
                "posts",
                "Post",
                vec![FieldMapping::simple("id"), FieldMapping::simple("title")],
            ),
        ])
        .with_typename("User");

        let data = json!({
            "id": "user-1",
            "name": "Alice",
            "posts": [
                { "id": "post-1", "title": "First Post", "views": 100 },
                { "id": "post-2", "title": "Second Post", "views": 200 }
            ]
        });

        let jsonb = JsonbValue::new(data);
        let result = mapper.project(&jsonb).unwrap();

        assert_eq!(
            result,
            json!({
                "__typename": "User",
                "id": "user-1",
                "name": "Alice",
                "posts": [
                    { "__typename": "Post", "id": "post-1", "title": "First Post" },
                    { "__typename": "Post", "id": "post-2", "title": "Second Post" }
                ]
            })
        );
    }

    #[test]
    fn test_deeply_nested_typename() {
        // Post -> author (User) -> company (Company)
        let mapper = ProjectionMapper::with_mappings(vec![
            FieldMapping::simple("id"),
            FieldMapping::nested_object(
                "author",
                "User",
                vec![
                    FieldMapping::simple("name"),
                    FieldMapping::nested_object(
                        "company",
                        "Company",
                        vec![FieldMapping::simple("name")],
                    ),
                ],
            ),
        ])
        .with_typename("Post");

        let data = json!({
            "id": "post-1",
            "author": {
                "name": "Alice",
                "company": {
                    "name": "Acme Corp",
                    "revenue": 1_000_000
                }
            }
        });

        let jsonb = JsonbValue::new(data);
        let result = mapper.project(&jsonb).unwrap();

        assert_eq!(
            result,
            json!({
                "__typename": "Post",
                "id": "post-1",
                "author": {
                    "__typename": "User",
                    "name": "Alice",
                    "company": {
                        "__typename": "Company",
                        "name": "Acme Corp"
                    }
                }
            })
        );
    }

    #[test]
    fn test_nested_object_with_alias_and_typename() {
        let mapper = ProjectionMapper::with_mappings(vec![
            FieldMapping::simple("id"),
            FieldMapping::nested_object_aliased(
                "author",
                "writer",
                "User",
                vec![FieldMapping::simple("id"), FieldMapping::simple("name")],
            ),
        ])
        .with_typename("Post");

        let data = json!({
            "id": "post-1",
            "author": {
                "id": "user-1",
                "name": "Alice"
            }
        });

        let jsonb = JsonbValue::new(data);
        let result = mapper.project(&jsonb).unwrap();

        // "author" should be output as "writer" with typename
        assert_eq!(
            result,
            json!({
                "__typename": "Post",
                "id": "post-1",
                "writer": {
                    "__typename": "User",
                    "id": "user-1",
                    "name": "Alice"
                }
            })
        );
    }

    // ========================================================================
    // Issue #27: Nested objects returned as JSON strings
    // ========================================================================

    #[test]
    fn test_nested_object_as_json_string_is_re_parsed() {
        // Reproduces Issue #27: when the database extracts a nested JSONB field
        // using ->>'field' (text operator), it arrives as a JSON string rather
        // than a proper Object. The projector must re-parse it.
        let mapper = ProjectionMapper::with_mappings(vec![
            FieldMapping::simple("id"),
            FieldMapping::nested_object(
                "author",
                "User",
                vec![FieldMapping::simple("id"), FieldMapping::simple("name")],
            ),
        ])
        .with_typename("Post");

        // "author" is a raw JSON string, not a parsed object
        let data = json!({
            "id": "post-1",
            "author": "{\"id\":\"user-2\",\"name\":\"Bob\"}"
        });

        let jsonb = JsonbValue::new(data);
        let result = mapper.project(&jsonb).unwrap();

        // author must be an object, not a string
        let author = result.get("author").expect("author field missing");
        assert!(author.is_object(), "author should be a JSON object, got: {:?}", author);
        assert_eq!(author.get("id"), Some(&json!("user-2")));
        assert_eq!(author.get("name"), Some(&json!("Bob")));
    }

    // ========================================================================
    // configure_typename_from_selections tests
    // ========================================================================

    fn make_selections_with_typename() -> Vec<FieldSelection> {
        vec![FieldSelection {
            name:          "users".to_string(),
            alias:         None,
            arguments:     vec![],
            nested_fields: vec![
                FieldSelection {
                    name:          "id".to_string(),
                    alias:         None,
                    arguments:     vec![],
                    nested_fields: vec![],
                    directives:    vec![],
                },
                FieldSelection {
                    name:          "__typename".to_string(),
                    alias:         None,
                    arguments:     vec![],
                    nested_fields: vec![],
                    directives:    vec![],
                },
            ],
            directives:    vec![],
        }]
    }

    fn make_selections_without_typename() -> Vec<FieldSelection> {
        vec![FieldSelection {
            name:          "users".to_string(),
            alias:         None,
            arguments:     vec![],
            nested_fields: vec![FieldSelection {
                name:          "id".to_string(),
                alias:         None,
                arguments:     vec![],
                nested_fields: vec![],
                directives:    vec![],
            }],
            directives:    vec![],
        }]
    }

    #[test]
    fn test_configure_typename_from_selections_present() {
        let projector = ResultProjector::new(vec!["id".to_string()])
            .configure_typename_from_selections(&make_selections_with_typename(), "User");

        let data = json!({ "id": "1", "name": "Alice" });
        let results = vec![JsonbValue::new(data)];
        let result = projector.project_results(&results, false).unwrap();

        assert_eq!(result, json!({ "__typename": "User", "id": "1" }));
    }

    #[test]
    fn test_configure_typename_from_selections_absent() {
        let projector = ResultProjector::new(vec!["id".to_string()])
            .configure_typename_from_selections(&make_selections_without_typename(), "User");

        let data = json!({ "id": "1", "name": "Alice" });
        let results = vec![JsonbValue::new(data)];
        let result = projector.project_results(&results, false).unwrap();

        // No __typename because selection set didn't request it
        assert_eq!(result, json!({ "id": "1" }));
    }

    #[test]
    fn test_configure_typename_from_selections_list() {
        let projector = ResultProjector::new(vec!["id".to_string()])
            .configure_typename_from_selections(&make_selections_with_typename(), "User");

        let results = vec![
            JsonbValue::new(json!({ "id": "1" })),
            JsonbValue::new(json!({ "id": "2" })),
        ];
        let result = projector.project_results(&results, true).unwrap();

        assert_eq!(
            result,
            json!([
                { "__typename": "User", "id": "1" },
                { "__typename": "User", "id": "2" }
            ])
        );
    }

    #[test]
    fn test_configure_typename_empty_selections() {
        // Empty selections → no typename
        let projector = ResultProjector::new(vec!["id".to_string()])
            .configure_typename_from_selections(&[], "User");

        let data = json!({ "id": "1" });
        let results = vec![JsonbValue::new(data)];
        let result = projector.project_results(&results, false).unwrap();

        assert_eq!(result, json!({ "id": "1" }));
    }

    // ========================================================================
    // Federation mode tests
    // ========================================================================

    #[test]
    fn test_federation_mode_injects_typename() {
        let projector = ResultProjector::new(vec!["id".to_string()])
            .with_typename("User")
            .with_federation_mode(true);

        let data = json!({ "id": "1", "name": "Alice" });
        let results = vec![JsonbValue::new(data)];
        let result = projector.project_results(&results, false).unwrap();

        assert_eq!(result, json!({ "__typename": "User", "id": "1" }));
    }

    #[test]
    fn test_federation_mode_flag_propagates() {
        let mapper = ProjectionMapper::new(vec!["id".to_string()]).with_federation_mode(true);
        assert!(mapper.federation_mode);

        let mapper2 = ProjectionMapper::new(vec!["id".to_string()]).with_federation_mode(false);
        assert!(!mapper2.federation_mode);
    }

    // ========================================================================

    #[test]
    fn test_nested_without_specific_fields() {
        // When nested_fields is None, all source fields are copied
        let mapper = ProjectionMapper::with_mappings(vec![
            FieldMapping::simple("id"),
            FieldMapping::simple("author").with_nested_typename("User"),
        ])
        .with_typename("Post");

        let data = json!({
            "id": "post-1",
            "author": {
                "id": "user-1",
                "name": "Alice",
                "email": "alice@example.com"
            }
        });

        let jsonb = JsonbValue::new(data);
        let result = mapper.project(&jsonb).unwrap();

        // All author fields should be copied, plus __typename
        assert_eq!(
            result,
            json!({
                "__typename": "Post",
                "id": "post-1",
                "author": {
                    "__typename": "User",
                    "id": "user-1",
                    "name": "Alice",
                    "email": "alice@example.com"
                }
            })
        );
    }
}

mod query_tracing_tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
    use crate::runtime::query_tracing::*;

    #[test]
    fn test_trace_builder_new() {
        let builder = QueryTraceBuilder::new("query_1", "{ user { id } }");
        assert_eq!(builder.query_id, "query_1");
        assert_eq!(builder.query, "{ user { id } }");
        assert!(builder.phases.is_empty());
    }

    #[test]
    fn test_trace_builder_truncate_long_query() {
        let long_query = "a".repeat(600);
        let builder = QueryTraceBuilder::new("query_1", &long_query);
        assert!(builder.query.len() < 600);
        assert!(builder.query.ends_with("..."));
    }

    #[test]
    fn test_record_phase_success() {
        let mut builder = QueryTraceBuilder::new("query_1", "{ user { id } }");
        builder.record_phase_success("parse", 100);
        builder.record_phase_success("validate", 50);

        assert_eq!(builder.phases.len(), 2);
        assert_eq!(builder.phases[0].phase, "parse");
        assert_eq!(builder.phases[0].duration_us, 100);
        assert!(builder.phases[0].success);
    }

    #[test]
    fn test_record_phase_error() {
        let mut builder = QueryTraceBuilder::new("query_1", "{ user { id } }");
        builder.record_phase_error("parse", 100, "Invalid syntax");

        assert_eq!(builder.phases.len(), 1);
        assert_eq!(builder.phases[0].phase, "parse");
        assert!(!builder.phases[0].success);
        assert_eq!(builder.phases[0].error, Some("Invalid syntax".to_string()));
    }

    #[test]
    fn test_finish_success() {
        let mut builder = QueryTraceBuilder::new("query_1", "{ user { id } }");
        builder.record_phase_success("parse", 100);
        builder.record_phase_success("execute", 500);

        let trace = builder.finish(true, None, Some(10)).unwrap();
        assert!(trace.success);
        assert_eq!(trace.query_id, "query_1");
        assert_eq!(trace.phases.len(), 2);
        assert_eq!(trace.result_count, Some(10));
        // total_duration_us is wall-clock time, may vary depending on system speed
    }

    #[test]
    fn test_finish_error() {
        let mut builder = QueryTraceBuilder::new("query_1", "{ user { id } }");
        builder.record_phase_success("parse", 100);
        builder.record_phase_error("execute", 50, "Database connection failed");

        let trace = builder.finish(false, Some("Database connection failed"), None).unwrap();
        assert!(!trace.success);
        assert_eq!(trace.error, Some("Database connection failed".to_string()));
    }

    #[test]
    fn test_average_phase_duration() {
        let mut builder = QueryTraceBuilder::new("query_1", "{ user { id } }");
        builder.record_phase_success("parse", 100);
        builder.record_phase_success("validate", 200);
        builder.record_phase_success("execute", 300);

        let trace = builder.finish(true, None, None).unwrap();
        assert_eq!(trace.average_phase_duration_us(), 200);
    }

    #[test]
    fn test_slowest_phase() {
        let mut builder = QueryTraceBuilder::new("query_1", "{ user { id } }");
        builder.record_phase_success("parse", 100);
        builder.record_phase_success("execute", 500);
        builder.record_phase_success("cache_check", 50);

        let trace = builder.finish(true, None, None).unwrap();
        let slowest = trace.slowest_phase().unwrap();
        assert_eq!(slowest.phase, "execute");
        assert_eq!(slowest.duration_us, 500);
    }

    #[test]
    fn test_to_log_string_success() {
        let mut builder = QueryTraceBuilder::new("query_1", "{ user { id } }");
        builder.record_phase_success("parse", 100);
        builder.record_phase_success("execute", 500);

        let trace = builder.finish(true, None, Some(5)).unwrap();
        let log_str = trace.to_log_string();
        assert!(log_str.contains("query_id=query_1"));
        assert!(log_str.contains("status=success"));
        assert!(log_str.contains("parse=100us"));
        assert!(log_str.contains("execute=500us"));
    }

    #[test]
    fn test_to_log_string_error() {
        let mut builder = QueryTraceBuilder::new("query_1", "{ user { id } }");
        builder.record_phase_success("parse", 100);
        builder.record_phase_error("validate", 50, "Type mismatch");

        let trace = builder.finish(false, Some("Type mismatch"), None).unwrap();
        let log_str = trace.to_log_string();
        assert!(log_str.contains("status=error"));
        assert!(log_str.contains("error=Type mismatch"));
    }

    #[test]
    fn test_average_phase_duration_empty() {
        let builder = QueryTraceBuilder::new("query_1", "{ user { id } }");
        let trace = builder.finish(true, None, None).unwrap();
        assert_eq!(trace.average_phase_duration_us(), 0);
    }

    #[test]
    fn test_elapsed_us() {
        let builder = QueryTraceBuilder::new("query_1", "{ user { id } }");
        let elapsed = builder.elapsed_us();
        // Elapsed time should be non-negative (u64 is always >= 0)
        let _ = elapsed;
    }

    #[test]
    fn test_trace_serialization() {
        let mut builder = QueryTraceBuilder::new("query_1", "{ user { id } }");
        builder.record_phase_success("parse", 100);

        let trace = builder.finish(true, None, Some(5)).unwrap();
        let json = serde_json::to_string(&trace).expect("serialize should work");
        let restored: QueryExecutionTrace =
            serde_json::from_str(&json).expect("deserialize should work");

        assert_eq!(restored.query_id, trace.query_id);
        assert_eq!(restored.phases.len(), trace.phases.len());
    }

    #[test]
    fn test_query_phase_span_serialize() {
        let span = QueryPhaseSpan {
            phase:       "parse".to_string(),
            duration_us: 100,
            success:     true,
            error:       None,
        };

        let json = serde_json::to_string(&span).expect("serialize should work");
        let restored: QueryPhaseSpan =
            serde_json::from_str(&json).expect("deserialize should work");

        assert_eq!(restored.phase, span.phase);
        assert_eq!(restored.duration_us, span.duration_us);
    }

    #[test]
    fn test_truncate_query_helper() {
        assert_eq!(truncate_query("hello", 100), "hello");
        assert!(truncate_query(&"a".repeat(200), 50).ends_with("..."));
    }
}

mod relay_tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
    use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};

    use crate::runtime::relay::*;

    #[test]
    fn test_edge_cursor_roundtrip() {
        for pk in [0_i64, 1, 42, 999_999, i64::MAX] {
            let cursor = encode_edge_cursor(pk);
            assert_eq!(decode_edge_cursor(&cursor), Some(pk));
        }
    }

    #[test]
    fn test_edge_cursor_negative_pk() {
        // Negative pks are unusual but still encodable.
        let cursor = encode_edge_cursor(-1);
        assert_eq!(decode_edge_cursor(&cursor), Some(-1));
    }

    #[test]
    fn test_edge_cursor_i64_min_roundtrips() {
        // Guards the sign-flip mutation: decode(encode(i64::MIN)) must equal i64::MIN.
        let cursor = encode_edge_cursor(i64::MIN);
        assert_eq!(
            decode_edge_cursor(&cursor),
            Some(i64::MIN),
            "i64::MIN must roundtrip through encode/decode"
        );
    }

    #[test]
    fn test_edge_cursor_negative_max_roundtrips() {
        // Guards -(i64::MAX): distinct from i64::MIN, covers the full negative range.
        let cursor = encode_edge_cursor(-i64::MAX);
        assert_eq!(decode_edge_cursor(&cursor), Some(-i64::MAX));
    }

    #[test]
    fn test_edge_cursor_invalid() {
        assert_eq!(decode_edge_cursor("!!!not-base64"), None);
        assert_eq!(decode_edge_cursor(""), None);
        // Valid base64 but not an integer.
        let bad = BASE64.encode("not-a-number");
        assert_eq!(decode_edge_cursor(&bad), None);
    }

    #[test]
    fn test_node_id_roundtrip() {
        let uuid = "550e8400-e29b-41d4-a716-446655440000";
        let id = encode_node_id("User", uuid);
        let decoded = decode_node_id(&id);
        assert_eq!(decoded, Some(("User".to_string(), uuid.to_string())));
    }

    #[test]
    fn test_node_id_various_types() {
        for type_name in ["User", "BlogPost", "OrderItem"] {
            let uuid = "00000000-0000-0000-0000-000000000001";
            let id = encode_node_id(type_name, uuid);
            let decoded = decode_node_id(&id);
            assert_eq!(decoded.as_ref().map(|(t, _)| t.as_str()), Some(type_name));
            assert_eq!(decoded.as_ref().map(|(_, u)| u.as_str()), Some(uuid));
        }
    }

    #[test]
    fn test_node_id_invalid() {
        assert_eq!(decode_node_id("!!!not-base64"), None);
        assert_eq!(decode_node_id(""), None);
        // Valid base64 but no colon separator.
        let no_colon = BASE64.encode("UserMissingColon");
        assert_eq!(decode_node_id(&no_colon), None);
    }

    #[test]
    fn test_edge_cursor_is_base64() {
        let cursor = encode_edge_cursor(42);
        // Verify it's valid base64 by decoding.
        BASE64
            .decode(&cursor)
            .unwrap_or_else(|e| panic!("expected valid base64 edge cursor: {e}"));
    }

    #[test]
    fn test_node_id_is_base64() {
        let id = encode_node_id("User", "some-uuid");
        BASE64
            .decode(&id)
            .unwrap_or_else(|e| panic!("expected valid base64 node ID: {e}"));
    }
}

mod sql_logger_tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
    use std::{thread, time::Duration};

    use crate::runtime::sql_logger::*;

    #[test]
    fn test_sql_operation_detection() {
        assert_eq!(SqlOperation::from_sql("SELECT * FROM users"), SqlOperation::Select);
        assert_eq!(SqlOperation::from_sql("  select id from users"), SqlOperation::Select);
        assert_eq!(SqlOperation::from_sql("INSERT INTO users VALUES (1)"), SqlOperation::Insert);
        assert_eq!(SqlOperation::from_sql("UPDATE users SET id=1"), SqlOperation::Update);
        assert_eq!(SqlOperation::from_sql("DELETE FROM users"), SqlOperation::Delete);
        assert_eq!(SqlOperation::from_sql("CREATE TABLE users (id INT)"), SqlOperation::Other);
    }

    #[test]
    fn test_sql_operation_display() {
        assert_eq!(SqlOperation::Select.to_string(), "SELECT");
        assert_eq!(SqlOperation::Insert.to_string(), "INSERT");
        assert_eq!(SqlOperation::Update.to_string(), "UPDATE");
        assert_eq!(SqlOperation::Delete.to_string(), "DELETE");
        assert_eq!(SqlOperation::Other.to_string(), "OTHER");
    }

    #[test]
    fn test_builder_success() {
        let builder = SqlQueryLogBuilder::new("query_1", "SELECT * FROM users", 0);
        let log = builder.finish_success(Some(10));

        assert!(log.success);
        assert_eq!(log.query_id, "query_1");
        assert_eq!(log.operation, SqlOperation::Select);
        assert_eq!(log.rows_affected, Some(10));
        assert!(log.error.is_none());
        // duration_us is wall-clock time, may vary depending on system speed
    }

    #[test]
    fn test_builder_error() {
        let builder = SqlQueryLogBuilder::new("query_1", "SELECT * FROM nonexistent", 0);
        let log = builder.finish_error("Table not found");

        assert!(!log.success);
        assert_eq!(log.error, Some("Table not found".to_string()));
        assert!(log.rows_affected.is_none());
    }

    #[test]
    fn test_query_truncation() {
        let long_query = "a".repeat(3000);
        let builder = SqlQueryLogBuilder::new("query_1", &long_query, 0);
        let log = builder.finish_success(None);

        assert!(log.sql.len() < 3000);
        assert!(log.sql.ends_with("..."));
    }

    #[test]
    fn test_slow_query_detection() {
        let builder =
            SqlQueryLogBuilder::new("query_1", "SELECT * FROM users", 0).with_slow_threshold(100);

        let log = builder.finish_success(Some(5));

        // Query should be considered fast (runs much faster than 100 us typically)
        assert!(!log.was_slow);
    }

    #[test]
    fn test_slow_query_warning() {
        let builder =
            SqlQueryLogBuilder::new("query_1", "SELECT * FROM users", 0).with_slow_threshold(1);

        // Simulate slow query by sleeping
        thread::sleep(Duration::from_micros(100));
        let log = builder.finish_success(Some(5));

        assert!(log.was_slow);
    }

    #[test]
    fn test_log_string_success() {
        let builder = SqlQueryLogBuilder::new("query_1", "SELECT * FROM users", 5);
        let log = builder.finish_success(Some(10));

        let log_str = log.to_log_string();
        assert!(log_str.contains("SELECT"));
        assert!(log_str.contains("query_1"));
        assert!(log_str.contains("params=5"));
        assert!(log_str.contains("rows=Some(10)"));
    }

    #[test]
    fn test_log_string_error() {
        let builder = SqlQueryLogBuilder::new("query_1", "SELECT * FROM users", 0);
        let log = builder.finish_error("Connection timeout");

        let log_str = log.to_log_string();
        assert!(log_str.contains("FAILED"));
        assert!(log_str.contains("Connection timeout"));
    }

    #[test]
    fn test_duration_ms() {
        let builder = SqlQueryLogBuilder::new("query_1", "SELECT * FROM users", 0);
        thread::sleep(Duration::from_millis(10));
        let log = builder.finish_success(None);

        let ms = log.duration_ms();
        assert!(ms >= 10.0);
    }

    #[test]
    fn test_serialization() {
        let builder = SqlQueryLogBuilder::new("query_1", "SELECT * FROM users", 3);
        let log = builder.finish_success(Some(25));

        let json = serde_json::to_string(&log).expect("serialize should work");
        let restored: SqlQueryLog = serde_json::from_str(&json).expect("deserialize should work");

        assert_eq!(restored.query_id, log.query_id);
        assert_eq!(restored.operation, log.operation);
        assert_eq!(restored.rows_affected, log.rows_affected);
    }

    #[test]
    fn test_all_operations() {
        let operations = vec![
            SqlOperation::Select,
            SqlOperation::Insert,
            SqlOperation::Update,
            SqlOperation::Delete,
            SqlOperation::Other,
        ];

        for op in operations {
            let builder = SqlQueryLogBuilder::new("query_1", "SELECT 1", 0);
            let mut log = builder.finish_success(None);
            log.operation = op;

            assert_eq!(log.operation, op);
            let log_str = log.to_log_string();
            assert!(log_str.contains(&op.to_string()));
        }
    }

    #[test]
    fn test_param_count() {
        let builder =
            SqlQueryLogBuilder::new("query_1", "SELECT * FROM users WHERE id = ? AND name = ?", 2);
        let log = builder.finish_success(Some(1));

        assert_eq!(log.param_count, 2);
        assert!(log.to_log_string().contains("params=2"));
    }
}

mod tenant_enforcer_tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
    use serde_json::json;

    use crate::{
        db::where_clause::{WhereClause, WhereOperator},
        runtime::tenant_enforcer::*,
    };

    #[test]
    fn test_tenant_enforcer_with_org_id() {
        let enforcer = TenantEnforcer::new(Some("org-123".to_string()));
        assert!(enforcer.is_tenant_scoped());
        assert_eq!(enforcer.get_org_id(), Some("org-123"));
    }

    #[test]
    fn test_tenant_enforcer_without_org_id() {
        let enforcer = TenantEnforcer::new(None);
        assert!(!enforcer.is_tenant_scoped());
        assert_eq!(enforcer.get_org_id(), None);
    }

    #[test]
    fn test_enforce_tenant_scope_with_no_where_clause() {
        let enforcer = TenantEnforcer::new(Some("org-123".to_string()));
        let result = enforcer.enforce_tenant_scope(None);

        let enforced =
            result.unwrap_or_else(|e| panic!("expected Ok for enforce_tenant_scope: {e}"));
        assert!(enforced.is_some());

        // Check that it created an org_id = 'org-123' filter
        if let Some(WhereClause::Field {
            path,
            operator,
            value,
        }) = enforced
        {
            assert_eq!(path, vec!["org_id".to_string()]);
            assert_eq!(operator, WhereOperator::Eq);
            assert_eq!(value, json!("org-123"));
        } else {
            panic!("Expected Field clause");
        }
    }

    #[test]
    fn test_enforce_tenant_scope_with_existing_where_clause() {
        let enforcer = TenantEnforcer::new(Some("org-123".to_string()));

        let user_clause = WhereClause::Field {
            path:     vec!["status".to_string()],
            operator: WhereOperator::Eq,
            value:    json!("active"),
        };

        let result = enforcer.enforce_tenant_scope(Some(&user_clause));

        let enforced = result
            .unwrap_or_else(|e| panic!("expected Ok for enforce_tenant_scope with clause: {e}"));
        assert!(enforced.is_some());

        // Check that it created an AND clause combining both filters
        if let Some(WhereClause::And(clauses)) = enforced {
            assert_eq!(clauses.len(), 2);
        } else {
            panic!("Expected And clause");
        }
    }

    #[test]
    fn test_enforce_tenant_scope_without_org_id() {
        let enforcer = TenantEnforcer::new(None);
        let user_clause = WhereClause::Field {
            path:     vec!["status".to_string()],
            operator: WhereOperator::Eq,
            value:    json!("active"),
        };

        let result = enforcer.enforce_tenant_scope(Some(&user_clause));

        // Should return original clause unchanged
        let enforced = result
            .unwrap_or_else(|e| panic!("expected Ok for enforce_tenant_scope without org_id: {e}"));
        assert!(matches!(enforced, Some(WhereClause::Field { .. })));
    }

    #[test]
    fn test_require_tenant_fails_without_org_id() {
        let enforcer = TenantEnforcer::with_requirement(None, true);
        let result = enforcer.enforce_tenant_scope(None);

        let err = result.expect_err("expected Err when tenant required but org_id absent");
        assert_eq!(err, "Request must be tenant-scoped (missing org_id)");
    }

    #[test]
    fn test_require_tenant_succeeds_with_org_id() {
        let enforcer = TenantEnforcer::with_requirement(Some("org-123".to_string()), true);
        let result = enforcer.enforce_tenant_scope(None);

        result.unwrap_or_else(|e| {
            panic!("expected Ok for enforce_tenant_scope with org_id present: {e}")
        });
    }
}

mod window_tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
    use crate::{
        compiler::{
            aggregation::{OrderByClause, OrderDirection},
            window_functions::{
                FrameBoundary, FrameType, SelectColumn, WindowExecutionPlan, WindowFrame,
                WindowFunction, WindowFunctionType,
            },
        },
        db::{WhereClause, WhereOperator, types::DatabaseType},
        runtime::window::*,
    };

    #[test]
    fn test_generate_row_number() {
        let generator = WindowSqlGenerator::new(DatabaseType::PostgreSQL);

        let plan = WindowExecutionPlan {
            table:        "tf_sales".to_string(),
            select:       vec![SelectColumn {
                expression: "revenue".to_string(),
                alias:      "revenue".to_string(),
            }],
            windows:      vec![WindowFunction {
                function:     WindowFunctionType::RowNumber,
                alias:        "rank".to_string(),
                partition_by: vec!["data->>'category'".to_string()],
                order_by:     vec![OrderByClause::new(
                    "revenue".to_string(),
                    OrderDirection::Desc,
                )],
                frame:        None,
            }],
            where_clause: None,
            order_by:     vec![],
            limit:        None,
            offset:       None,
        };

        let sql = generator.generate(&plan).unwrap();

        assert!(sql.raw_sql.contains("ROW_NUMBER()"));
        assert!(sql.raw_sql.contains("PARTITION BY data->>'category'"));
        assert!(sql.raw_sql.contains("ORDER BY revenue DESC"));
    }

    #[test]
    fn test_generate_running_total() {
        let generator = WindowSqlGenerator::new(DatabaseType::PostgreSQL);

        let plan = WindowExecutionPlan {
            table:        "tf_sales".to_string(),
            select:       vec![
                SelectColumn {
                    expression: "occurred_at".to_string(),
                    alias:      "date".to_string(),
                },
                SelectColumn {
                    expression: "revenue".to_string(),
                    alias:      "revenue".to_string(),
                },
            ],
            windows:      vec![WindowFunction {
                function:     WindowFunctionType::Sum {
                    field: "revenue".to_string(),
                },
                alias:        "running_total".to_string(),
                partition_by: vec![],
                order_by:     vec![OrderByClause::new(
                    "occurred_at".to_string(),
                    OrderDirection::Asc,
                )],
                frame:        Some(WindowFrame {
                    frame_type: FrameType::Rows,
                    start:      FrameBoundary::UnboundedPreceding,
                    end:        FrameBoundary::CurrentRow,
                    exclusion:  None,
                }),
            }],
            where_clause: None,
            order_by:     vec![],
            limit:        None,
            offset:       None,
        };

        let sql = generator.generate(&plan).unwrap();

        assert!(sql.raw_sql.contains("SUM(revenue) OVER"));
        assert!(sql.raw_sql.contains("ROWS BETWEEN UNBOUNDED PRECEDING AND CURRENT ROW"));
    }

    #[test]
    fn test_generate_lag_lead() {
        let generator = WindowSqlGenerator::new(DatabaseType::PostgreSQL);

        let plan = WindowExecutionPlan {
            table:        "tf_sales".to_string(),
            select:       vec![],
            windows:      vec![
                WindowFunction {
                    function:     WindowFunctionType::Lag {
                        field:   "revenue".to_string(),
                        offset:  1,
                        default: Some(serde_json::json!(0)),
                    },
                    alias:        "prev_revenue".to_string(),
                    partition_by: vec![],
                    order_by:     vec![OrderByClause::new(
                        "occurred_at".to_string(),
                        OrderDirection::Asc,
                    )],
                    frame:        None,
                },
                WindowFunction {
                    function:     WindowFunctionType::Lead {
                        field:   "revenue".to_string(),
                        offset:  1,
                        default: None,
                    },
                    alias:        "next_revenue".to_string(),
                    partition_by: vec![],
                    order_by:     vec![OrderByClause::new(
                        "occurred_at".to_string(),
                        OrderDirection::Asc,
                    )],
                    frame:        None,
                },
            ],
            where_clause: None,
            order_by:     vec![],
            limit:        None,
            offset:       None,
        };

        let sql = generator.generate(&plan).unwrap();

        assert!(sql.raw_sql.contains("LAG(revenue, 1, 0)"));
        assert!(sql.raw_sql.contains("LEAD(revenue, 1)"));
    }

    #[test]
    fn test_frame_boundary_formatting() {
        let generator = WindowSqlGenerator::new(DatabaseType::PostgreSQL);

        assert_eq!(
            generator.format_frame_boundary(&FrameBoundary::UnboundedPreceding),
            "UNBOUNDED PRECEDING"
        );
        assert_eq!(
            generator.format_frame_boundary(&FrameBoundary::NPreceding { n: 5 }),
            "5 PRECEDING"
        );
        assert_eq!(generator.format_frame_boundary(&FrameBoundary::CurrentRow), "CURRENT ROW");
        assert_eq!(
            generator.format_frame_boundary(&FrameBoundary::NFollowing { n: 3 }),
            "3 FOLLOWING"
        );
        assert_eq!(
            generator.format_frame_boundary(&FrameBoundary::UnboundedFollowing),
            "UNBOUNDED FOLLOWING"
        );
    }

    #[test]
    fn test_moving_average() {
        let generator = WindowSqlGenerator::new(DatabaseType::PostgreSQL);

        let plan = WindowExecutionPlan {
            table:        "tf_sales".to_string(),
            select:       vec![],
            windows:      vec![WindowFunction {
                function:     WindowFunctionType::Avg {
                    field: "revenue".to_string(),
                },
                alias:        "moving_avg_7d".to_string(),
                partition_by: vec![],
                order_by:     vec![OrderByClause::new(
                    "occurred_at".to_string(),
                    OrderDirection::Asc,
                )],
                frame:        Some(WindowFrame {
                    frame_type: FrameType::Rows,
                    start:      FrameBoundary::NPreceding { n: 6 },
                    end:        FrameBoundary::CurrentRow,
                    exclusion:  None,
                }),
            }],
            where_clause: None,
            order_by:     vec![],
            limit:        None,
            offset:       None,
        };

        let sql = generator.generate(&plan).unwrap();

        assert!(sql.raw_sql.contains("AVG(revenue) OVER"));
        assert!(sql.raw_sql.contains("ROWS BETWEEN 6 PRECEDING AND CURRENT ROW"));
    }

    #[test]
    fn test_sqlserver_stddev_variance() {
        let generator = WindowSqlGenerator::new(DatabaseType::SQLServer);

        let plan = WindowExecutionPlan {
            table:        "tf_sales".to_string(),
            select:       vec![],
            windows:      vec![
                WindowFunction {
                    function:     WindowFunctionType::Stddev {
                        field: "revenue".to_string(),
                    },
                    alias:        "stddev".to_string(),
                    partition_by: vec![],
                    order_by:     vec![],
                    frame:        None,
                },
                WindowFunction {
                    function:     WindowFunctionType::Variance {
                        field: "revenue".to_string(),
                    },
                    alias:        "variance".to_string(),
                    partition_by: vec![],
                    order_by:     vec![],
                    frame:        None,
                },
            ],
            where_clause: None,
            order_by:     vec![],
            limit:        None,
            offset:       None,
        };

        let sql = generator.generate(&plan).unwrap();

        // SQL Server uses STDEV/VAR instead of STDDEV/VARIANCE
        assert!(sql.raw_sql.contains("STDEV(revenue)"));
        assert!(sql.raw_sql.contains("VAR(revenue)"));
    }

    #[test]
    fn test_where_clause_uses_bind_parameters() {
        // Ensures WHERE clause is rendered with $N bind parameters (not literal values).
        // Literals would require escaping and are vulnerable to injection edge-cases;
        // bind parameters are always safe.
        let generator = WindowSqlGenerator::new(DatabaseType::PostgreSQL);

        let plan = WindowExecutionPlan {
            table:        "tf_sales".to_string(),
            select:       vec![SelectColumn {
                expression: "revenue".to_string(),
                alias:      "revenue".to_string(),
            }],
            windows:      vec![WindowFunction {
                function:     WindowFunctionType::RowNumber,
                alias:        "rank".to_string(),
                partition_by: vec![],
                order_by:     vec![],
                frame:        None,
            }],
            where_clause: Some(WhereClause::Field {
                path:     vec!["status".to_string()],
                operator: WhereOperator::Eq,
                value:    serde_json::json!("active"),
            }),
            order_by:     vec![],
            limit:        None,
            offset:       None,
        };

        let sql = generator.generate(&plan).unwrap();

        // WHERE clause must use bind parameter ($1), not a literal string value.
        assert!(
            sql.raw_sql.contains("WHERE data->>'status' = $1"),
            "expected bind parameter $1, got: {}",
            sql.raw_sql
        );
        assert!(!sql.raw_sql.contains("WHERE 1=1"));
        assert_eq!(sql.parameters, vec![serde_json::json!("active")]);
    }

    #[test]
    fn test_where_clause_applied() {
        let generator = WindowSqlGenerator::new(DatabaseType::PostgreSQL);

        let plan = WindowExecutionPlan {
            table:        "tf_sales".to_string(),
            select:       vec![SelectColumn {
                expression: "revenue".to_string(),
                alias:      "revenue".to_string(),
            }],
            windows:      vec![WindowFunction {
                function:     WindowFunctionType::RowNumber,
                alias:        "rank".to_string(),
                partition_by: vec![],
                order_by:     vec![],
                frame:        None,
            }],
            where_clause: Some(WhereClause::Field {
                path:     vec!["status".to_string()],
                operator: WhereOperator::Eq,
                value:    serde_json::json!("active"),
            }),
            order_by:     vec![],
            limit:        None,
            offset:       None,
        };

        let sql = generator.generate(&plan).unwrap();

        // WHERE clause is rendered (not 1=1), value is a bind parameter.
        assert!(sql.raw_sql.contains("WHERE"), "WHERE clause must appear in SQL");
        assert!(!sql.raw_sql.contains("WHERE 1=1"));
    }

    #[test]
    fn test_no_where_clause_omitted() {
        let generator = WindowSqlGenerator::new(DatabaseType::PostgreSQL);

        let plan = WindowExecutionPlan {
            table:        "tf_sales".to_string(),
            select:       vec![],
            windows:      vec![WindowFunction {
                function:     WindowFunctionType::RowNumber,
                alias:        "rank".to_string(),
                partition_by: vec![],
                order_by:     vec![],
                frame:        None,
            }],
            where_clause: None,
            order_by:     vec![],
            limit:        None,
            offset:       None,
        };

        let sql = generator.generate(&plan).unwrap();

        // No WHERE clause in output
        assert!(!sql.raw_sql.contains("WHERE"));
    }
}

mod window_parser_tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
    use serde_json::json;

    use crate::{
        compiler::{
            aggregation::OrderDirection,
            fact_table::{
                DimensionColumn, FactTableMetadata, FilterColumn, MeasureColumn, SqlType,
            },
            window_functions::{PartitionByColumn, WindowFunctionSpec, WindowSelectColumn},
        },
        runtime::window_parser::*,
    };

    fn create_test_metadata() -> FactTableMetadata {
        FactTableMetadata {
            table_name:               "tf_sales".to_string(),
            measures:                 vec![
                MeasureColumn {
                    name:     "revenue".to_string(),
                    sql_type: SqlType::Decimal,
                    nullable: false,
                },
                MeasureColumn {
                    name:     "quantity".to_string(),
                    sql_type: SqlType::Int,
                    nullable: false,
                },
            ],
            dimensions:               DimensionColumn {
                name:  "dimensions".to_string(),
                paths: vec![],
            },
            denormalized_filters:     vec![
                FilterColumn {
                    name:     "customer_id".to_string(),
                    sql_type: SqlType::Uuid,
                    indexed:  true,
                },
                FilterColumn {
                    name:     "occurred_at".to_string(),
                    sql_type: SqlType::Timestamp,
                    indexed:  true,
                },
            ],
            calendar_dimensions:      vec![],
            partial_period:           None,
            native_measures:          std::collections::HashMap::new(),
            native_dimension_mapping: std::collections::HashMap::new(),
        }
    }

    #[test]
    fn test_parse_simple_window_query() {
        let metadata = create_test_metadata();
        let query = json!({
            "table": "tf_sales",
            "select": [
                {"type": "measure", "name": "revenue", "alias": "revenue"}
            ],
            "windows": [
                {
                    "function": {"type": "row_number"},
                    "alias": "rank",
                    "partitionBy": [{"type": "dimension", "path": "category"}],
                    "orderBy": [{"field": "revenue", "direction": "DESC"}]
                }
            ]
        });

        let request = WindowQueryParser::parse(&query, &metadata).unwrap();

        assert_eq!(request.table_name, "tf_sales");
        assert_eq!(request.select.len(), 1);
        assert_eq!(request.windows.len(), 1);
        assert_eq!(request.windows[0].alias, "rank");
        assert!(matches!(request.windows[0].function, WindowFunctionSpec::RowNumber));
    }

    #[test]
    fn test_parse_running_sum() {
        let metadata = create_test_metadata();
        let query = json!({
            "table": "tf_sales",
            "select": [],
            "windows": [
                {
                    "function": {"type": "running_sum", "measure": "revenue"},
                    "alias": "running_total",
                    "orderBy": [{"field": "occurred_at", "direction": "ASC"}],
                    "frame": {
                        "frame_type": "ROWS",
                        "start": {"type": "unbounded_preceding"},
                        "end": {"type": "current_row"}
                    }
                }
            ]
        });

        let request = WindowQueryParser::parse(&query, &metadata).unwrap();

        assert_eq!(request.windows.len(), 1);
        match &request.windows[0].function {
            WindowFunctionSpec::RunningSum { measure } => {
                assert_eq!(measure, "revenue");
            },
            _ => panic!("Expected RunningSum function"),
        }
        assert!(request.windows[0].frame.is_some());
    }

    #[test]
    fn test_parse_lag_function() {
        let metadata = create_test_metadata();
        let query = json!({
            "table": "tf_sales",
            "windows": [
                {
                    "function": {"type": "lag", "field": "revenue", "offset": 1, "default": 0},
                    "alias": "prev_revenue",
                    "orderBy": [{"field": "occurred_at"}]
                }
            ]
        });

        let request = WindowQueryParser::parse(&query, &metadata).unwrap();

        match &request.windows[0].function {
            WindowFunctionSpec::Lag {
                field,
                offset,
                default,
            } => {
                assert_eq!(field, "revenue");
                assert_eq!(*offset, 1);
                assert!(default.is_some());
            },
            _ => panic!("Expected Lag function"),
        }
    }

    #[test]
    fn test_parse_ntile_function() {
        let metadata = create_test_metadata();
        let query = json!({
            "table": "tf_sales",
            "windows": [
                {
                    "function": {"type": "ntile", "n": 4},
                    "alias": "quartile",
                    "orderBy": [{"field": "revenue", "direction": "DESC"}]
                }
            ]
        });

        let request = WindowQueryParser::parse(&query, &metadata).unwrap();

        match &request.windows[0].function {
            WindowFunctionSpec::Ntile { n } => {
                assert_eq!(*n, 4);
            },
            _ => panic!("Expected Ntile function"),
        }
    }

    #[test]
    fn test_parse_select_columns() {
        let metadata = create_test_metadata();
        let query = json!({
            "table": "tf_sales",
            "select": [
                {"type": "measure", "name": "revenue", "alias": "rev"},
                {"type": "dimension", "path": "category", "alias": "cat"},
                {"type": "filter", "name": "occurred_at", "alias": "date"}
            ]
        });

        let request = WindowQueryParser::parse(&query, &metadata).unwrap();

        assert_eq!(request.select.len(), 3);
        assert!(matches!(
            &request.select[0],
            WindowSelectColumn::Measure { name, alias } if name == "revenue" && alias == "rev"
        ));
        assert!(matches!(
            &request.select[1],
            WindowSelectColumn::Dimension { path, alias } if path == "category" && alias == "cat"
        ));
        assert!(matches!(
            &request.select[2],
            WindowSelectColumn::Filter { name, alias } if name == "occurred_at" && alias == "date"
        ));
    }

    #[test]
    fn test_parse_partition_by() {
        let metadata = create_test_metadata();
        let query = json!({
            "table": "tf_sales",
            "windows": [
                {
                    "function": {"type": "row_number"},
                    "alias": "rank",
                    "partitionBy": [
                        {"type": "dimension", "path": "category"},
                        {"type": "filter", "name": "customer_id"}
                    ],
                    "orderBy": []
                }
            ]
        });

        let request = WindowQueryParser::parse(&query, &metadata).unwrap();

        assert_eq!(request.windows[0].partition_by.len(), 2);
        assert!(matches!(
            &request.windows[0].partition_by[0],
            PartitionByColumn::Dimension { path } if path == "category"
        ));
        assert!(matches!(
            &request.windows[0].partition_by[1],
            PartitionByColumn::Filter { name } if name == "customer_id"
        ));
    }

    #[test]
    fn test_parse_limit_offset() {
        let metadata = create_test_metadata();
        let query = json!({
            "table": "tf_sales",
            "limit": 100,
            "offset": 50
        });

        let request = WindowQueryParser::parse(&query, &metadata).unwrap();

        assert_eq!(request.limit, Some(100));
        assert_eq!(request.offset, Some(50));
    }

    #[test]
    fn test_parse_final_order_by() {
        let metadata = create_test_metadata();
        let query = json!({
            "table": "tf_sales",
            "orderBy": [
                {"field": "revenue", "direction": "DESC"},
                {"field": "occurred_at", "direction": "ASC"}
            ]
        });

        let request = WindowQueryParser::parse(&query, &metadata).unwrap();

        assert_eq!(request.order_by.len(), 2);
        assert_eq!(request.order_by[0].field, "revenue");
        assert_eq!(request.order_by[0].direction, OrderDirection::Desc);
        assert_eq!(request.order_by[1].field, "occurred_at");
        assert_eq!(request.order_by[1].direction, OrderDirection::Asc);
    }

    #[test]
    fn test_parse_complex_window_query() {
        let metadata = create_test_metadata();
        let query = json!({
            "table": "tf_sales",
            "select": [
                {"type": "measure", "name": "revenue", "alias": "revenue"},
                {"type": "dimension", "path": "category", "alias": "category"}
            ],
            "windows": [
                {
                    "function": {"type": "row_number"},
                    "alias": "rank",
                    "partitionBy": [{"type": "dimension", "path": "category"}],
                    "orderBy": [{"field": "revenue", "direction": "DESC"}]
                },
                {
                    "function": {"type": "running_sum", "measure": "revenue"},
                    "alias": "running_total",
                    "partitionBy": [{"type": "dimension", "path": "category"}],
                    "orderBy": [{"field": "occurred_at", "direction": "ASC"}],
                    "frame": {
                        "frame_type": "ROWS",
                        "start": {"type": "unbounded_preceding"},
                        "end": {"type": "current_row"}
                    }
                },
                {
                    "function": {"type": "lag", "field": "revenue", "offset": 1},
                    "alias": "prev_revenue",
                    "partitionBy": [{"type": "dimension", "path": "category"}],
                    "orderBy": [{"field": "occurred_at", "direction": "ASC"}]
                }
            ],
            "orderBy": [
                {"field": "category", "direction": "ASC"},
                {"field": "revenue", "direction": "DESC"}
            ],
            "limit": 100
        });

        let request = WindowQueryParser::parse(&query, &metadata).unwrap();

        assert_eq!(request.table_name, "tf_sales");
        assert_eq!(request.select.len(), 2);
        assert_eq!(request.windows.len(), 3);
        assert_eq!(request.order_by.len(), 2);
        assert_eq!(request.limit, Some(100));
    }

    #[test]
    fn test_parse_error_missing_table() {
        let metadata = create_test_metadata();
        let query = json!({
            "select": [],
            "windows": []
        });

        let result = WindowQueryParser::parse(&query, &metadata);
        let err = result.expect_err("expected Err for missing table field");
        assert!(err.to_string().contains("table"), "unexpected error message: {err}");
    }

    #[test]
    fn test_parse_error_invalid_function_type() {
        let metadata = create_test_metadata();
        let query = json!({
            "table": "tf_sales",
            "windows": [
                {
                    "function": {"type": "invalid_function"},
                    "alias": "test"
                }
            ]
        });

        let result = WindowQueryParser::parse(&query, &metadata);
        let err = result.expect_err("expected Err for invalid window function type");
        assert!(err.to_string().contains("Unknown"), "unexpected error message: {err}");
    }
}

mod window_projector_tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
    use std::collections::HashMap;

    use serde_json::{Value, json};

    use crate::{
        compiler::window_functions::{
            SelectColumn, WindowExecutionPlan, WindowFunction, WindowFunctionType,
        },
        runtime::window_projector::*,
    };

    fn create_test_plan() -> WindowExecutionPlan {
        WindowExecutionPlan {
            table:        "tf_sales".to_string(),
            select:       vec![
                SelectColumn {
                    expression: "revenue".to_string(),
                    alias:      "revenue".to_string(),
                },
                SelectColumn {
                    expression: "category".to_string(),
                    alias:      "category".to_string(),
                },
            ],
            windows:      vec![WindowFunction {
                function:     WindowFunctionType::RowNumber,
                alias:        "rank".to_string(),
                partition_by: vec!["category".to_string()],
                order_by:     vec![],
                frame:        None,
            }],
            where_clause: None,
            order_by:     vec![],
            limit:        None,
            offset:       None,
        }
    }

    #[test]
    fn test_project_empty_results() {
        let plan = create_test_plan();
        let rows: Vec<HashMap<String, Value>> = vec![];

        let result = WindowProjector::project(rows, &plan).unwrap();
        assert_eq!(result, json!([]));
    }

    #[test]
    fn test_project_single_row() {
        let plan = create_test_plan();
        let mut row = HashMap::new();
        row.insert("revenue".to_string(), json!(100.00));
        row.insert("category".to_string(), json!("Electronics"));
        row.insert("rank".to_string(), json!(1));

        let rows = vec![row];
        let result = WindowProjector::project(rows, &plan).unwrap();

        let expected = json!([
            {"revenue": 100.00, "category": "Electronics", "rank": 1}
        ]);
        assert_eq!(result, expected);
    }

    #[test]
    fn test_project_multiple_rows() {
        let plan = create_test_plan();

        let mut row1 = HashMap::new();
        row1.insert("revenue".to_string(), json!(100.00));
        row1.insert("category".to_string(), json!("Electronics"));
        row1.insert("rank".to_string(), json!(1));

        let mut row2 = HashMap::new();
        row2.insert("revenue".to_string(), json!(150.00));
        row2.insert("category".to_string(), json!("Electronics"));
        row2.insert("rank".to_string(), json!(2));

        let mut row3 = HashMap::new();
        row3.insert("revenue".to_string(), json!(50.00));
        row3.insert("category".to_string(), json!("Books"));
        row3.insert("rank".to_string(), json!(1));

        let rows = vec![row1, row2, row3];
        let result = WindowProjector::project(rows, &plan).unwrap();

        let expected = json!([
            {"revenue": 100.00, "category": "Electronics", "rank": 1},
            {"revenue": 150.00, "category": "Electronics", "rank": 2},
            {"revenue": 50.00, "category": "Books", "rank": 1}
        ]);
        assert_eq!(result, expected);
    }

    #[test]
    fn test_wrap_in_data_envelope() {
        let projected = json!([{"rank": 1}, {"rank": 2}]);
        let response = WindowProjector::wrap_in_data_envelope(projected, "sales_window");

        let expected = json!({
            "data": {
                "sales_window": [{"rank": 1}, {"rank": 2}]
            }
        });
        assert_eq!(response, expected);
    }

    #[test]
    fn test_project_with_null_values() {
        let plan = create_test_plan();

        let mut row = HashMap::new();
        row.insert("revenue".to_string(), json!(null));
        row.insert("category".to_string(), json!("Unknown"));
        row.insert("rank".to_string(), json!(1));

        let rows = vec![row];
        let result = WindowProjector::project(rows, &plan).unwrap();

        let expected = json!([
            {"revenue": null, "category": "Unknown", "rank": 1}
        ]);
        assert_eq!(result, expected);
    }

    #[test]
    fn test_project_with_numeric_types() {
        let plan = create_test_plan();

        let mut row = HashMap::new();
        row.insert("revenue".to_string(), json!(1234.56));
        row.insert("category".to_string(), json!("Electronics"));
        row.insert("rank".to_string(), json!(1));
        row.insert("running_total".to_string(), json!(5000.00));
        row.insert("row_count".to_string(), json!(42));

        let rows = vec![row];
        let result = WindowProjector::project(rows, &plan).unwrap();

        // Verify numeric values are preserved
        let arr = result.as_array().unwrap();
        let first_row = &arr[0];
        assert_eq!(first_row["revenue"], json!(1234.56));
        assert_eq!(first_row["rank"], json!(1));
        assert_eq!(first_row["running_total"], json!(5000.00));
        assert_eq!(first_row["row_count"], json!(42));
    }
}

// ── RuntimeConfig::from_compiled_schema — the H16 single constructor seam ──────
mod runtime_config_from_schema_tests {
    use serde_json::json;

    use crate::{
        runtime::{RuntimeConfig, page_size_precedence},
        schema::{
            CURRENT_SCHEMA_FORMAT_VERSION, ChangelogConfig, CompiledSchema, SecurityConfig,
            ValidationConfig,
        },
    };

    #[test]
    fn rejects_incompatible_format_version() {
        let mut schema = CompiledSchema::new();
        schema.schema_format_version = Some(CURRENT_SCHEMA_FORMAT_VERSION + 1);
        let result = RuntimeConfig::from_compiled_schema(&schema);
        assert!(result.is_err(), "an incompatible schema version must refuse to build a config");
        assert!(result.unwrap_err().contains("mismatch"));
    }

    #[test]
    fn accepts_legacy_and_current_versions() {
        // No version (pre-v2.1): warns but builds.
        let legacy = CompiledSchema::new();
        assert!(RuntimeConfig::from_compiled_schema(&legacy).is_ok());

        let mut current = CompiledSchema::new();
        current.schema_format_version = Some(CURRENT_SCHEMA_FORMAT_VERSION);
        assert!(RuntimeConfig::from_compiled_schema(&current).is_ok());
    }

    #[test]
    fn reads_audit_logging_flag_from_enterprise_config() {
        let mut schema = CompiledSchema::new();
        let mut security = SecurityConfig::default();
        security
            .additional
            .insert("enterprise".to_string(), json!({ "audit_logging_enabled": true }));
        schema.security = Some(security);

        let config = RuntimeConfig::from_compiled_schema(&schema).unwrap();
        assert!(config.audit_mutations, "audit_logging_enabled must flow into audit_mutations");
    }

    #[test]
    fn audit_logging_defaults_off_when_absent() {
        let schema = CompiledSchema::new();
        let config = RuntimeConfig::from_compiled_schema(&schema).unwrap();
        assert!(!config.audit_mutations);
    }

    #[test]
    fn threads_compiled_max_page_size() {
        let mut schema = CompiledSchema::new();
        schema.validation_config = Some(ValidationConfig {
            max_page_size: Some(250),
            ..ValidationConfig::default()
        });

        // Note: assumes FRAISEQL_MAX_PAGE_SIZE is unset in the test env (it is in CI);
        // the env-override precedence itself is covered by `page_size_*` below.
        let config = RuntimeConfig::from_compiled_schema(&schema).unwrap();
        assert_eq!(config.max_page_size, Some(250));
    }

    #[test]
    fn reads_changelog_write_enabled() {
        let mut schema = CompiledSchema::new();
        let changelog = ChangelogConfig {
            write_enabled: false,
            ..ChangelogConfig::default()
        };
        schema.changelog = Some(changelog);

        let config = RuntimeConfig::from_compiled_schema(&schema).unwrap();
        assert!(
            !config.changelog_enabled,
            "compiled write_enabled=false must disable the outbox"
        );
    }

    #[test]
    fn changelog_defaults_on_when_absent() {
        let schema = CompiledSchema::new();
        let config = RuntimeConfig::from_compiled_schema(&schema).unwrap();
        assert!(config.changelog_enabled);
    }

    // ── page_size_precedence (moved here with the #421 logic it implements) ──
    #[test]
    fn page_size_default_when_nothing_set() {
        assert_eq!(page_size_precedence(None, None), Some(1000));
    }

    #[test]
    fn page_size_compiled_overrides_default() {
        assert_eq!(page_size_precedence(None, Some(250)), Some(250));
    }

    #[test]
    fn page_size_env_overrides_compiled() {
        assert_eq!(page_size_precedence(Some("500"), Some(250)), Some(500));
    }

    #[test]
    fn page_size_env_disables_ceiling() {
        assert_eq!(page_size_precedence(Some("none"), Some(250)), None);
        assert_eq!(page_size_precedence(Some("0"), Some(250)), None);
    }

    #[test]
    fn page_size_unparseable_env_falls_through_to_compiled() {
        assert_eq!(page_size_precedence(Some("lots"), Some(250)), Some(250));
    }
}
