#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable
use super::*;
use crate::compiler::fact_table::{DimensionColumn, FilterColumn, MeasureColumn, SqlType};

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
            paths: vec![crate::compiler::fact_table::DimensionPath {
                name:      "category".to_string(),
                json_path: "dimensions->>'category'".to_string(),
                data_type: "text".to_string(),
            }],
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

    let plan = WindowFunctionPlanner::plan(&query, &metadata).expect("window plan should succeed");

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

    let plan = WindowFunctionPlanner::plan(&query, &metadata).expect("window plan should succeed");

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
    WindowFunctionPlanner::validate(&plan, &metadata, DatabaseType::PostgreSQL)
        .unwrap_or_else(|e| panic!("expected Ok for PostgreSQL GROUPS frame: {e}"));
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

    let plan = WindowPlanner::plan(request, &metadata).expect("window plan should succeed");

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

    let plan = WindowPlanner::plan(request, &metadata).expect("window plan should succeed");

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

    let plan = WindowPlanner::plan(request, &metadata).expect("window plan should succeed");

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

    let err =
        WindowPlanner::plan(request, &metadata).expect_err("expected Err for invalid measure name");
    assert!(err.to_string().contains("not found"), "unexpected error: {err}");
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

    let err =
        WindowPlanner::plan(request, &metadata).expect_err("expected Err for invalid filter name");
    assert!(err.to_string().contains("not found"), "unexpected error: {err}");
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

    let plan = WindowPlanner::plan(request, &metadata).expect("window plan should succeed");

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

    let plan = WindowPlanner::plan(request, &metadata).expect("window plan should succeed");

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

    let plan = WindowPlanner::plan(request, &metadata).expect("window plan should succeed");

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

    let plan = WindowPlanner::plan(request, &metadata).expect("window plan should succeed");

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
    assert!(json2.contains('4'));
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

    let result = WindowPlanner::plan(request, &metadata);
    assert!(result.is_err(), "injection attempt in orderBy field must be rejected");
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("invalid characters"),
        "error should mention invalid characters: {msg}"
    );
}

// =============================================================================
// #794 / #795 — SQL injection on the LIVE window path
//
// These drive the same three calls the shipped binary makes —
// `WindowQueryParser::parse` -> `WindowPlanner::plan` -> `WindowSqlGenerator::generate`
// — with the payloads verbatim from the issue bodies. They are deliberately *not*
// written against `WindowFunctionPlanner`, the planner that consults `WindowAllowlist`,
// because nothing in the shipped binary calls it: testing that one is what let this ship.
// =============================================================================

#[cfg(test)]
mod live_path_injection {
    use super::*;
    use crate::runtime::{WindowQueryParser, window::WindowSqlGenerator};

    /// Everything a client controls arrives as the raw `variables` JSON object.
    fn plan_from_variables(variables: &serde_json::Value) -> Result<WindowExecutionPlan> {
        let metadata = create_test_metadata();
        let request = WindowQueryParser::parse(variables, &metadata)?;
        WindowPlanner::plan(request, &metadata)
    }

    fn sql_from_variables(variables: &serde_json::Value) -> Result<String> {
        let plan = plan_from_variables(variables)?;
        Ok(WindowSqlGenerator::new(crate::db::DatabaseType::PostgreSQL)
            .generate(&plan)?
            .raw_sql)
    }

    /// #794 sink 3: `alias` on a measure select is cloned through untouched and emitted
    /// by `write!(sql, "{} AS {}", …)`. The issue's payload appends a whole scalar
    /// subquery against `pg_authid`.
    #[test]
    fn measure_alias_cannot_smuggle_a_subquery() {
        let variables = serde_json::json!({
            "table": "tf_sales",
            "select": [{
                "type":  "measure",
                "name":  "revenue",
                "alias": "c, (SELECT string_agg(rolname, ',') FROM pg_authid) AS leak"
            }]
        });

        let result = sql_from_variables(&variables);

        assert!(
            result.is_err(),
            "a measure alias carrying a subquery must be rejected, got SQL: {:?}",
            result.ok()
        );
    }

    /// #794 sink 1: the Dimension select arm builds `format!("{}->>'{}'", …, path)` with
    /// no charset check, so a quote in `path` breaks out of the JSONB key literal.
    #[test]
    fn dimension_path_cannot_break_out_of_the_jsonb_key() {
        let variables = serde_json::json!({
            "table": "tf_sales",
            "select": [{
                "type":  "dimension",
                "path":  "category'||(SELECT rolname FROM pg_authid LIMIT 1)||'",
                "alias": "d"
            }]
        });

        let result = sql_from_variables(&variables);

        assert!(
            result.is_err(),
            "a dimension path containing a quote must be rejected, got SQL: {:?}",
            result.ok()
        );
    }

    /// #794 sink 3: the window-function `alias` reaches `write!(sql, " AS {}", …)`.
    #[test]
    fn window_function_alias_cannot_smuggle_a_subquery() {
        let variables = serde_json::json!({
            "table":   "tf_sales",
            "select":  [],
            "windows": [{
                "function": {"type": "row_number"},
                "alias":    "rank, (SELECT current_user) AS whoami"
            }]
        });

        let result = sql_from_variables(&variables);

        assert!(
            result.is_err(),
            "a window alias carrying a subquery must be rejected, got SQL: {:?}",
            result.ok()
        );
    }

    /// #794 sink 2: `PartitionByColumn::Dimension` repeats the unvalidated `format!`.
    #[test]
    fn partition_by_dimension_path_cannot_break_out_of_the_jsonb_key() {
        let variables = serde_json::json!({
            "table":   "tf_sales",
            "select":  [],
            "windows": [{
                "function":    {"type": "row_number"},
                "alias":       "rank",
                "partitionBy": [{"type": "dimension", "path": "x'||(SELECT version())||'"}]
            }]
        });

        let result = sql_from_variables(&variables);

        assert!(
            result.is_err(),
            "a partitionBy dimension path containing a quote must be rejected, got SQL: {:?}",
            result.ok()
        );
    }

    /// #795: the FROM target comes from the client's `table` key and is never reconciled
    /// against the fact table the root field already resolved. A subquery substitutes the
    /// relation outright; naming a different table also drops the RLS policy, which is
    /// looked up by that same unvalidated name.
    #[test]
    fn table_cannot_substitute_the_relation_with_a_subquery() {
        let variables = serde_json::json!({
            "table": "(SELECT jsonb_build_object('category', rolname) AS dimensions \
                      FROM pg_authid) AS x",
            "select": [{"type": "measure", "name": "revenue", "alias": "r"}]
        });

        let result = sql_from_variables(&variables);

        assert!(
            result.is_err(),
            "a subquery in `table` must be rejected, got SQL: {:?}",
            result.ok()
        );
    }

    /// #795: even a plain, well-formed identifier must be refused when it is not the fact
    /// table the root field resolved — that is the RLS-bypass half of the issue.
    #[test]
    fn table_cannot_name_a_different_relation() {
        let variables = serde_json::json!({
            "table":  "pg_authid",
            "select": [{"type": "measure", "name": "revenue", "alias": "r"}]
        });

        let result = sql_from_variables(&variables);

        assert!(
            result.is_err(),
            "a `table` other than the resolved fact table must be rejected, got SQL: {:?}",
            result.ok()
        );
    }

    /// The guard must not break the legitimate query shape it protects.
    #[test]
    fn a_well_formed_window_query_still_plans_and_generates() {
        let variables = serde_json::json!({
            "table":   "tf_sales",
            "select":  [
                {"type": "measure",   "name": "revenue",  "alias": "revenue"},
                {"type": "dimension", "path": "category", "alias": "category"}
            ],
            "windows": [{
                "function":    {"type": "row_number"},
                "alias":       "rank",
                "partitionBy": [{"type": "dimension", "path": "category"}]
            }]
        });

        let sql =
            sql_from_variables(&variables).expect("a legitimate window query must still work");

        assert!(sql.contains("FROM tf_sales"), "expected the resolved fact table: {sql}");
        assert!(sql.contains("ROW_NUMBER()"), "expected the window function: {sql}");
    }
}
