#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
use super::*;
use crate::compiler::fact_table::{DimensionColumn, FilterColumn, MeasureColumn, SqlType};

fn create_test_metadata() -> FactTableMetadata {
    FactTableMetadata {
        table_name:           "tf_sales".to_string(),
        measures:             vec![
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
        dimensions:           DimensionColumn {
            name:  "dimensions".to_string(),
            paths: vec![],
        },
        denormalized_filters: vec![
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
        calendar_dimensions:  vec![],
    }
}

// =============================================================================
// Test Helpers
// =============================================================================

/// Helper to serialize test objects without panicking
fn serialize_json<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_string(value).expect("serialization should succeed for test objects")
}

/// Helper to deserialize test JSON without panicking
fn deserialize_json<'a, T: serde::Deserialize<'a>>(json: &'a str) -> T {
    serde_json::from_str(json).expect("deserialization should succeed for valid test JSON")
}

// =============================================================================
// Tests
// =============================================================================

#[test]
fn test_window_function_type_serialization() {
    let func = WindowFunctionType::RowNumber;
    let json = serialize_json(&func);
    assert_eq!(json, r#"{"type":"row_number"}"#);
}

#[test]
fn test_frame_type_serialization() {
    let frame_type = FrameType::Rows;
    let json = serialize_json(&frame_type);
    assert_eq!(json, r#""ROWS""#);
}

#[test]
fn test_frame_boundary_unbounded() {
    let boundary = FrameBoundary::UnboundedPreceding;
    let json = serialize_json(&boundary);
    assert!(json.contains("unbounded_preceding"));
}

#[test]
fn test_frame_boundary_n_preceding() {
    let boundary = FrameBoundary::NPreceding { n: 5 };
    let json = serialize_json(&boundary);
    assert!(json.contains("n_preceding"));
    assert!(json.contains("\"n\":5"));
}

#[test]
fn test_parse_row_number_query() {
    let metadata = create_test_metadata();
    let query = serde_json::json!({
        "table": "tf_sales",
        "select": ["revenue"],
        "windows": [{
            "function": {"type": "row_number"},
            "alias": "rank",
            "partitionBy": ["category"],
            "orderBy": [{"field": "revenue", "direction": "DESC"}]
        }]
    });

    let plan =
        WindowFunctionPlanner::plan(&query, &metadata).expect("window plan should succeed");

    assert_eq!(plan.table, "tf_sales");
    assert_eq!(plan.windows.len(), 1);
    assert_eq!(plan.windows[0].alias, "rank");
    assert!(matches!(plan.windows[0].function, WindowFunctionType::RowNumber));
}

#[test]
fn test_parse_lag_function() {
    let metadata = create_test_metadata();
    let query = serde_json::json!({
        "table": "tf_sales",
        "windows": [{
            "function": {
                "type": "lag",
                "field": "revenue",
                "offset": 1,
                "default": 0
            },
            "alias": "prev_revenue",
            "orderBy": [{"field": "occurred_at"}]
        }]
    });

    let plan =
        WindowFunctionPlanner::plan(&query, &metadata).expect("window plan should succeed");

    match &plan.windows[0].function {
        WindowFunctionType::Lag {
            field,
            offset,
            default,
        } => {
            assert_eq!(field, "revenue");
            assert_eq!(*offset, 1);
            assert!(default.is_some());
        },
        _ => panic!("Expected LAG function"),
    }
}

#[test]
fn test_validate_groups_frame_postgres_only() {
    use crate::db::types::DatabaseType;

    let metadata = create_test_metadata();
    let plan = WindowExecutionPlan {
        table:        "tf_sales".to_string(),
        select:       vec![],
        windows:      vec![WindowFunction {
            function:     WindowFunctionType::RowNumber,
            alias:        "rank".to_string(),
            partition_by: vec![],
            order_by:     vec![],
            frame:        Some(WindowFrame {
                frame_type: FrameType::Groups,
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

    // Should pass for PostgreSQL
    assert!(
        WindowFunctionPlanner::validate(&plan, &metadata, DatabaseType::PostgreSQL).is_ok()
    );

    // Should fail for MySQL
    assert!(WindowFunctionPlanner::validate(&plan, &metadata, DatabaseType::MySQL).is_err());
}

// =============================================================================
// WindowPlanner Tests (High-Level -> Low-Level conversion)
// =============================================================================

#[test]
fn test_window_planner_basic_request() {
    let metadata = create_test_metadata();
    let request = WindowRequest {
        table_name:   "tf_sales".to_string(),
        select:       vec![
            WindowSelectColumn::Measure {
                name:  "revenue".to_string(),
                alias: "revenue".to_string(),
            },
            WindowSelectColumn::Dimension {
                path:  "category".to_string(),
                alias: "category".to_string(),
            },
        ],
        windows:      vec![WindowFunctionRequest {
            function:     WindowFunctionSpec::RowNumber,
            alias:        "rank".to_string(),
            partition_by: vec![PartitionByColumn::Dimension {
                path: "category".to_string(),
            }],
            order_by:     vec![WindowOrderBy {
                field:     "revenue".to_string(),
                direction: OrderDirection::Desc,
            }],
            frame:        None,
        }],
        where_clause: None,
        order_by:     vec![],
        limit:        Some(100),
        offset:       None,
    };

    let plan = WindowPlanner::plan(request, metadata).expect("window plan should succeed");

    assert_eq!(plan.table, "tf_sales");
    assert_eq!(plan.select.len(), 2);
    assert_eq!(plan.select[0].expression, "revenue");
    assert_eq!(plan.select[0].alias, "revenue");
    assert_eq!(plan.select[1].expression, "dimensions->>'category'");
    assert_eq!(plan.select[1].alias, "category");

    assert_eq!(plan.windows.len(), 1);
    assert_eq!(plan.windows[0].alias, "rank");
    assert!(matches!(plan.windows[0].function, WindowFunctionType::RowNumber));
    assert_eq!(plan.windows[0].partition_by, vec!["dimensions->>'category'"]);
    assert_eq!(plan.windows[0].order_by.len(), 1);
    assert_eq!(plan.windows[0].order_by[0].field, "revenue");
    assert_eq!(plan.windows[0].order_by[0].direction, OrderDirection::Desc);

    assert_eq!(plan.limit, Some(100));
}

#[test]
fn test_window_planner_running_sum() {
    let metadata = create_test_metadata();
    let request = WindowRequest {
        table_name:   "tf_sales".to_string(),
        select:       vec![WindowSelectColumn::Measure {
            name:  "revenue".to_string(),
            alias: "revenue".to_string(),
        }],
        windows:      vec![WindowFunctionRequest {
            function:     WindowFunctionSpec::RunningSum {
                measure: "revenue".to_string(),
            },
            alias:        "running_total".to_string(),
            partition_by: vec![],
            order_by:     vec![WindowOrderBy {
                field:     "occurred_at".to_string(),
                direction: OrderDirection::Asc,
            }],
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

    let plan = WindowPlanner::plan(request, metadata).expect("window plan should succeed");

    assert_eq!(plan.windows.len(), 1);
    match &plan.windows[0].function {
        WindowFunctionType::Sum { field } => {
            assert_eq!(field, "revenue");
        },
        _ => panic!("Expected Sum function"),
    }
    assert_eq!(plan.windows[0].alias, "running_total");
    assert!(plan.windows[0].frame.is_some());
}

#[test]
fn test_window_planner_filter_column() {
    let metadata = create_test_metadata();
    let request = WindowRequest {
        table_name:   "tf_sales".to_string(),
        select:       vec![WindowSelectColumn::Filter {
            name:  "occurred_at".to_string(),
            alias: "date".to_string(),
        }],
        windows:      vec![],
        where_clause: None,
        order_by:     vec![],
        limit:        None,
        offset:       None,
    };

    let plan = WindowPlanner::plan(request, metadata).expect("window plan should succeed");

    assert_eq!(plan.select.len(), 1);
    assert_eq!(plan.select[0].expression, "occurred_at");
    assert_eq!(plan.select[0].alias, "date");
}

#[test]
fn test_window_planner_invalid_measure() {
    let metadata = create_test_metadata();
    let request = WindowRequest {
        table_name:   "tf_sales".to_string(),
        select:       vec![WindowSelectColumn::Measure {
            name:  "nonexistent".to_string(),
            alias: "alias".to_string(),
        }],
        windows:      vec![],
        where_clause: None,
        order_by:     vec![],
        limit:        None,
        offset:       None,
    };

    let result = WindowPlanner::plan(request, metadata);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("not found"));
}

#[test]
fn test_window_planner_invalid_filter() {
    let metadata = create_test_metadata();
    let request = WindowRequest {
        table_name:   "tf_sales".to_string(),
        select:       vec![WindowSelectColumn::Filter {
            name:  "nonexistent_filter".to_string(),
            alias: "alias".to_string(),
        }],
        windows:      vec![],
        where_clause: None,
        order_by:     vec![],
        limit:        None,
        offset:       None,
    };

    let result = WindowPlanner::plan(request, metadata);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("not found"));
}

#[test]
fn test_window_planner_lag_function() {
    let metadata = create_test_metadata();
    let request = WindowRequest {
        table_name:   "tf_sales".to_string(),
        select:       vec![],
        windows:      vec![WindowFunctionRequest {
            function:     WindowFunctionSpec::Lag {
                field:   "revenue".to_string(),
                offset:  1,
                default: Some(serde_json::json!(0)),
            },
            alias:        "prev_revenue".to_string(),
            partition_by: vec![],
            order_by:     vec![WindowOrderBy {
                field:     "occurred_at".to_string(),
                direction: OrderDirection::Asc,
            }],
            frame:        None,
        }],
        where_clause: None,
        order_by:     vec![],
        limit:        None,
        offset:       None,
    };

    let plan = WindowPlanner::plan(request, metadata).expect("window plan should succeed");

    match &plan.windows[0].function {
        WindowFunctionType::Lag {
            field,
            offset,
            default,
        } => {
            assert_eq!(field, "revenue"); // measure stays as-is
            assert_eq!(*offset, 1);
            assert!(default.is_some());
        },
        _ => panic!("Expected Lag function"),
    }
}

#[test]
fn test_window_planner_dimension_field_in_lag() {
    let metadata = create_test_metadata();
    let request = WindowRequest {
        table_name:   "tf_sales".to_string(),
        select:       vec![],
        windows:      vec![WindowFunctionRequest {
            function:     WindowFunctionSpec::Lag {
                field:   "category".to_string(), // dimension path
                offset:  1,
                default: None,
            },
            alias:        "prev_category".to_string(),
            partition_by: vec![],
            order_by:     vec![WindowOrderBy {
                field:     "occurred_at".to_string(),
                direction: OrderDirection::Asc,
            }],
            frame:        None,
        }],
        where_clause: None,
        order_by:     vec![],
        limit:        None,
        offset:       None,
    };

    let plan = WindowPlanner::plan(request, metadata).expect("window plan should succeed");

    match &plan.windows[0].function {
        WindowFunctionType::Lag { field, .. } => {
            // dimension gets converted to JSONB extraction
            assert_eq!(field, "dimensions->>'category'");
        },
        _ => panic!("Expected Lag function"),
    }
}

#[test]
fn test_window_planner_partition_by_filter() {
    let metadata = create_test_metadata();
    let request = WindowRequest {
        table_name:   "tf_sales".to_string(),
        select:       vec![],
        windows:      vec![WindowFunctionRequest {
            function:     WindowFunctionSpec::RowNumber,
            alias:        "rank".to_string(),
            partition_by: vec![PartitionByColumn::Filter {
                name: "customer_id".to_string(),
            }],
            order_by:     vec![],
            frame:        None,
        }],
        where_clause: None,
        order_by:     vec![],
        limit:        None,
        offset:       None,
    };

    let plan = WindowPlanner::plan(request, metadata).expect("window plan should succeed");

    assert_eq!(plan.windows[0].partition_by, vec!["customer_id"]);
}

#[test]
fn test_window_planner_final_order_by() {
    let metadata = create_test_metadata();
    let request = WindowRequest {
        table_name:   "tf_sales".to_string(),
        select:       vec![],
        windows:      vec![],
        where_clause: None,
        order_by:     vec![
            WindowOrderBy {
                field:     "revenue".to_string(),
                direction: OrderDirection::Desc,
            },
            WindowOrderBy {
                field:     "category".to_string(), // dimension
                direction: OrderDirection::Asc,
            },
        ],
        limit:        None,
        offset:       None,
    };

    let plan = WindowPlanner::plan(request, metadata).expect("window plan should succeed");

    assert_eq!(plan.order_by.len(), 2);
    assert_eq!(plan.order_by[0].field, "revenue");
    assert_eq!(plan.order_by[0].direction, OrderDirection::Desc);
    assert_eq!(plan.order_by[1].field, "dimensions->>'category'");
    assert_eq!(plan.order_by[1].direction, OrderDirection::Asc);
}

#[test]
fn test_window_request_serialization() {
    let request = WindowRequest {
        table_name:   "tf_sales".to_string(),
        select:       vec![WindowSelectColumn::Measure {
            name:  "revenue".to_string(),
            alias: "revenue".to_string(),
        }],
        windows:      vec![WindowFunctionRequest {
            function:     WindowFunctionSpec::RowNumber,
            alias:        "rank".to_string(),
            partition_by: vec![],
            order_by:     vec![],
            frame:        None,
        }],
        where_clause: None,
        order_by:     vec![],
        limit:        Some(10),
        offset:       None,
    };

    // Should serialize without panic
    let json = serialize_json(&request);
    assert!(json.contains("tf_sales"));
    assert!(json.contains("revenue"));
    assert!(json.contains("row_number"));

    // Should deserialize back
    let deserialized: WindowRequest = deserialize_json(&json);
    assert_eq!(deserialized.table_name, "tf_sales");
    assert_eq!(deserialized.limit, Some(10));
}

#[test]
fn test_window_function_spec_serialization() {
    let spec = WindowFunctionSpec::RunningSum {
        measure: "revenue".to_string(),
    };
    let json = serialize_json(&spec);
    assert!(json.contains("running_sum"));
    assert!(json.contains("revenue"));

    let spec2 = WindowFunctionSpec::Ntile { n: 4 };
    let json2 = serialize_json(&spec2);
    assert!(json2.contains("ntile"));
    assert!(json2.contains("4"));
}

/// `resolve_field_to_sql` must reject fields whose names contain characters outside the
/// GraphQL identifier set (`[_A-Za-z][_0-9A-Za-z]*`).  Such names are embedded as
/// single-quoted JSONB keys and would break the SQL structure if accepted.
#[test]
fn test_resolve_field_rejects_injection_in_order_by() {
    let metadata = create_test_metadata();
    let request = WindowRequest {
        table_name:   "tf_sales".to_string(),
        select:       vec![],
        windows:      vec![],
        where_clause: None,
        order_by:     vec![WindowOrderBy {
            // Contains a single quote — must be rejected.
            field:     "x'; DROP TABLE t; --".to_string(),
            direction: OrderDirection::Asc,
        }],
        limit:        None,
        offset:       None,
    };

    let result = WindowPlanner::plan(request, metadata);
    assert!(result.is_err(), "injection attempt in orderBy field must be rejected");
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("invalid characters"),
        "error should mention invalid characters: {msg}"
    );
}
