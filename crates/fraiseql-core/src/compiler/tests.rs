//! Tests for the `compiler` module.

#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable
// ---------------------------------------------------------------------------
// mod.rs tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod compiler_tests {
    use crate::{
        compiler::fact_table::{DimensionColumn, FactTableMetadata},
        schema::CompiledSchema,
    };

    #[test]
    fn test_compiled_schema_fact_table_operations() {
        let mut schema = CompiledSchema::new();

        let metadata = FactTableMetadata {
            table_name:               "tf_sales".to_string(),
            type_name:                None,
            measures:                 vec![],
            dimensions:               DimensionColumn {
                name:  "data".to_string(),
                paths: vec![],
            },
            denormalized_filters:     vec![],
            calendar_dimensions:      vec![],
            native_measures:          std::collections::HashMap::new(),
            native_dimension_mapping: std::collections::HashMap::new(),
        };

        schema.add_fact_table("tf_sales".to_string(), metadata.clone());

        assert!(schema.has_fact_tables());

        let tables = schema.list_fact_tables();
        assert_eq!(tables.len(), 1);
        assert!(tables.contains(&"tf_sales"));

        let retrieved = schema.get_fact_table("tf_sales");
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap(), &metadata);

        assert!(schema.get_fact_table("tf_nonexistent").is_none());
    }
}

// ---------------------------------------------------------------------------
// ir.rs tests
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// parser.rs tests
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// validator.rs tests
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// enum_validator.rs tests
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// aggregate_types.rs tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod window_allowlist_tests {
    use super::super::window_allowlist::WindowAllowlist;
    use crate::{
        compiler::fact_table::{
            DimensionColumn, DimensionPath, FactTableMetadata, FilterColumn, MeasureColumn, SqlType,
        },
        error::FraiseQLError,
    };

    fn test_metadata() -> FactTableMetadata {
        FactTableMetadata {
            table_name:               "tf_sales".to_string(),
            type_name:                None,
            measures:                 vec![
                MeasureColumn {
                    name:       "revenue".to_string(),
                    sql_type:   SqlType::Decimal,
                    nullable:   false,
                    additivity: crate::compiler::fact_table::Additivity::Additive,
                },
                MeasureColumn {
                    name:       "units".to_string(),
                    sql_type:   SqlType::Int,
                    nullable:   false,
                    additivity: crate::compiler::fact_table::Additivity::Additive,
                },
            ],
            dimensions:               DimensionColumn {
                name:  "dimensions".to_string(),
                paths: vec![DimensionPath {
                    name:      "category".to_string(),
                    json_path: "dimensions->>'category'".to_string(),
                    data_type: "text".to_string(),
                }],
            },
            denormalized_filters:     vec![FilterColumn {
                name:      "occurred_at".to_string(),
                sql_type:  SqlType::Timestamp,
                indexed:   true,
                hierarchy: None,
            }],
            calendar_dimensions:      vec![],
            native_measures:          std::collections::HashMap::new(),
            native_dimension_mapping: std::collections::HashMap::new(),
        }
    }

    #[test]
    fn test_measure_name_accepted() {
        let al = WindowAllowlist::from_metadata(&test_metadata());
        al.validate("revenue", "PARTITION BY")
            .unwrap_or_else(|e| panic!("expected Ok: {e}"));
    }

    #[test]
    fn test_filter_name_accepted() {
        let al = WindowAllowlist::from_metadata(&test_metadata());
        al.validate("occurred_at", "ORDER BY")
            .unwrap_or_else(|e| panic!("expected Ok: {e}"));
    }

    #[test]
    fn test_dimension_short_name_accepted() {
        let al = WindowAllowlist::from_metadata(&test_metadata());
        al.validate("category", "PARTITION BY")
            .unwrap_or_else(|e| panic!("expected Ok: {e}"));
    }

    #[test]
    fn test_dimension_full_json_path_accepted() {
        let al = WindowAllowlist::from_metadata(&test_metadata());
        al.validate("dimensions->>'category'", "PARTITION BY")
            .unwrap_or_else(|e| panic!("expected Ok: {e}"));
    }

    #[test]
    fn test_unknown_field_rejected() {
        let al = WindowAllowlist::from_metadata(&test_metadata());
        assert!(
            matches!(
                al.validate("secret_column", "PARTITION BY"),
                Err(FraiseQLError::Validation { .. })
            ),
            "expected Validation error for unknown field"
        );
    }

    #[test]
    fn test_sql_injection_payloads_rejected() {
        let al = WindowAllowlist::from_metadata(&test_metadata());
        let payloads = [
            "'; DROP TABLE users; --",
            "1 UNION SELECT * FROM secrets",
            "field; DELETE FROM logs",
            "x\x00y",
            "field' OR '1'='1",
            "revenue--",
            "revenue UNION SELECT password FROM admin",
        ];
        for payload in &payloads {
            assert!(
                al.validate(payload, "PARTITION BY").is_err(),
                "Should reject payload: {payload}"
            );
        }
    }

    #[test]
    fn test_empty_allowlist_accepts_anything() {
        // When metadata has no known fields, allowlist is empty and validation is
        // skipped (character-level validation in the planner still applies).
        let al = WindowAllowlist::default();
        assert!(al.is_empty());
        al.validate("any_field", "PARTITION BY")
            .unwrap_or_else(|e| panic!("expected Ok for empty allowlist: {e}"));
        al.validate("'; DROP TABLE users; --", "ORDER BY")
            .unwrap_or_else(|e| panic!("expected Ok for empty allowlist: {e}"));
    }
}

// ---------------------------------------------------------------------------
// aggregation.rs tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod aggregation_tests {
    use super::super::aggregation::{
        AggregateExpression, AggregateSelection, AggregationPlanner, AggregationRequest,
        GroupByExpression, GroupBySelection, HavingCondition, OrderByClause, OrderDirection,
    };
    use crate::{
        compiler::{
            aggregate_types::{AggregateFunction, HavingOperator, TemporalBucket},
            fact_table::{
                DimensionColumn, FactTableMetadata, FilterColumn, MeasureColumn, SqlType,
            },
        },
        error::FraiseQLError,
    };

    fn create_test_metadata() -> FactTableMetadata {
        FactTableMetadata {
            table_name:               "tf_sales".to_string(),
            type_name:                None,
            measures:                 vec![
                MeasureColumn {
                    name:       "revenue".to_string(),
                    sql_type:   SqlType::Decimal,
                    nullable:   false,
                    additivity: crate::compiler::fact_table::Additivity::Additive,
                },
                MeasureColumn {
                    name:       "quantity".to_string(),
                    sql_type:   SqlType::Int,
                    nullable:   false,
                    additivity: crate::compiler::fact_table::Additivity::Additive,
                },
            ],
            dimensions:               DimensionColumn {
                name:  "dimensions".to_string(),
                paths: vec![],
            },
            denormalized_filters:     vec![
                FilterColumn {
                    name:      "customer_id".to_string(),
                    sql_type:  SqlType::Uuid,
                    indexed:   true,
                    hierarchy: None,
                },
                FilterColumn {
                    name:      "occurred_at".to_string(),
                    sql_type:  SqlType::Timestamp,
                    indexed:   true,
                    hierarchy: None,
                },
            ],
            calendar_dimensions:      vec![],
            native_measures:          std::collections::HashMap::new(),
            native_dimension_mapping: std::collections::HashMap::new(),
        }
    }

    #[test]
    fn test_plan_simple_aggregation() {
        let metadata = create_test_metadata();
        let request = AggregationRequest {
            table_name:   "tf_sales".to_string(),
            where_clause: None,
            group_by:     vec![],
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

        let plan = AggregationPlanner::plan(request, metadata).unwrap();

        assert_eq!(plan.aggregate_expressions.len(), 2);
        assert!(matches!(plan.aggregate_expressions[0], AggregateExpression::Count { .. }));
        assert!(matches!(
            plan.aggregate_expressions[1],
            AggregateExpression::MeasureAggregate { .. }
        ));
    }

    #[test]
    fn test_plan_with_group_by() {
        let metadata = create_test_metadata();
        let request = AggregationRequest {
            table_name:   "tf_sales".to_string(),
            where_clause: None,
            group_by:     vec![
                GroupBySelection::Dimension {
                    path:  "category".to_string(),
                    alias: "category".to_string(),
                },
                GroupBySelection::TemporalBucket {
                    column: "occurred_at".to_string(),
                    bucket: TemporalBucket::Day,
                    alias:  "occurred_at_day".to_string(),
                },
            ],
            aggregates:   vec![AggregateSelection::Count {
                alias: "count".to_string(),
            }],
            having:       vec![],
            order_by:     vec![],
            limit:        None,
            offset:       None,
        };

        let plan = AggregationPlanner::plan(request, metadata).unwrap();

        assert_eq!(plan.group_by_expressions.len(), 2);
        assert!(matches!(plan.group_by_expressions[0], GroupByExpression::JsonbPath { .. }));
        assert!(matches!(plan.group_by_expressions[1], GroupByExpression::TemporalBucket { .. }));
    }

    #[test]
    fn test_plan_with_having() {
        let metadata = create_test_metadata();
        let request = AggregationRequest {
            table_name:   "tf_sales".to_string(),
            where_clause: None,
            group_by:     vec![GroupBySelection::Dimension {
                path:  "category".to_string(),
                alias: "category".to_string(),
            }],
            aggregates:   vec![AggregateSelection::MeasureAggregate {
                measure:  "revenue".to_string(),
                function: AggregateFunction::Sum,
                alias:    "revenue_sum".to_string(),
            }],
            having:       vec![HavingCondition {
                aggregate: AggregateSelection::MeasureAggregate {
                    measure:  "revenue".to_string(),
                    function: AggregateFunction::Sum,
                    alias:    "revenue_sum".to_string(),
                },
                operator:  HavingOperator::Gt,
                value:     serde_json::json!(1000),
            }],
            order_by:     vec![],
            limit:        None,
            offset:       None,
        };

        let plan = AggregationPlanner::plan(request, metadata).unwrap();

        assert_eq!(plan.having_conditions.len(), 1);
        assert_eq!(plan.having_conditions[0].operator, HavingOperator::Gt);
    }

    #[test]
    fn test_validate_invalid_measure() {
        let metadata = create_test_metadata();
        let request = AggregationRequest {
            table_name:   "tf_sales".to_string(),
            where_clause: None,
            group_by:     vec![],
            aggregates:   vec![AggregateSelection::MeasureAggregate {
                measure:  "nonexistent".to_string(),
                function: AggregateFunction::Sum,
                alias:    "nonexistent_sum".to_string(),
            }],
            having:       vec![],
            order_by:     vec![],
            limit:        None,
            offset:       None,
        };

        let result = AggregationPlanner::plan(request, metadata);
        assert!(
            matches!(&result, Err(FraiseQLError::Validation { message, .. }) if message.contains("not found")),
            "expected Validation error about measure not found, got: {result:?}"
        );
    }

    #[test]
    fn test_validate_invalid_temporal_column() {
        let metadata = create_test_metadata();
        let request = AggregationRequest {
            table_name:   "tf_sales".to_string(),
            where_clause: None,
            group_by:     vec![GroupBySelection::TemporalBucket {
                column: "nonexistent".to_string(),
                bucket: TemporalBucket::Day,
                alias:  "day".to_string(),
            }],
            aggregates:   vec![AggregateSelection::Count {
                alias: "count".to_string(),
            }],
            having:       vec![],
            order_by:     vec![],
            limit:        None,
            offset:       None,
        };

        let result = AggregationPlanner::plan(request, metadata);
        assert!(
            matches!(&result, Err(FraiseQLError::Validation { message, .. }) if message.contains("not found")),
            "expected Validation error about column not found, got: {result:?}"
        );
    }

    #[test]
    fn test_order_by_from_graphql_json_object_format() {
        let json = serde_json::json!({ "name": "DESC", "created_at": "ASC" });
        let clauses = OrderByClause::from_graphql_json(&json).unwrap();
        assert_eq!(clauses.len(), 2);
        assert!(clauses.iter().any(|c| c.field == "name" && c.direction == OrderDirection::Desc));
        assert!(
            clauses
                .iter()
                .any(|c| c.field == "created_at" && c.direction == OrderDirection::Asc)
        );
    }

    #[test]
    fn test_order_by_from_graphql_json_array_format() {
        let json = serde_json::json!([
            { "field": "name", "direction": "DESC" },
            { "field": "age" }
        ]);
        let clauses = OrderByClause::from_graphql_json(&json).unwrap();
        assert_eq!(clauses.len(), 2);
        assert_eq!(clauses[0].field, "name");
        assert_eq!(clauses[0].direction, OrderDirection::Desc);
        assert_eq!(clauses[1].field, "age");
        assert_eq!(clauses[1].direction, OrderDirection::Asc); // default
    }

    #[test]
    fn test_order_by_from_graphql_json_invalid_direction() {
        let json = serde_json::json!({ "name": "INVALID" });
        let result = OrderByClause::from_graphql_json(&json);
        assert!(
            matches!(result, Err(FraiseQLError::Validation { .. })),
            "expected Validation error for invalid direction, got: {result:?}"
        );
    }

    #[test]
    fn test_order_by_rejects_sql_injection_in_field() {
        let json = serde_json::json!({ "x' || pg_sleep(5) || '": "ASC" });
        let result = OrderByClause::from_graphql_json(&json);
        assert!(
            matches!(result, Err(FraiseQLError::Validation { .. })),
            "expected Validation error for SQL injection in field, got: {result:?}"
        );
    }

    #[test]
    fn test_order_by_rejects_field_with_dot() {
        let json = serde_json::json!({ "a.b": "ASC" });
        let result = OrderByClause::from_graphql_json(&json);
        assert!(
            matches!(result, Err(FraiseQLError::Validation { .. })),
            "expected Validation error for dot in field name, got: {result:?}"
        );
    }

    #[test]
    fn test_order_by_rejects_empty_field() {
        let json = serde_json::json!({ "": "ASC" });
        let result = OrderByClause::from_graphql_json(&json);
        assert!(
            matches!(result, Err(FraiseQLError::Validation { .. })),
            "expected Validation error for empty field name, got: {result:?}"
        );
    }

    #[test]
    fn test_order_by_accepts_valid_identifiers() {
        let json = serde_json::json!({ "created_at": "DESC", "_score": "ASC" });
        let clauses = OrderByClause::from_graphql_json(&json).unwrap();
        assert_eq!(clauses.len(), 2);
    }

    #[test]
    fn test_order_by_array_rejects_injection_field() {
        let json = serde_json::json!([{ "field": "x' OR '1'='1", "direction": "ASC" }]);
        let result = OrderByClause::from_graphql_json(&json);
        assert!(
            matches!(result, Err(FraiseQLError::Validation { .. })),
            "expected Validation error for SQL injection in array field, got: {result:?}"
        );
    }

    /// Helper: metadata with declared dimension paths (for allowlist tests).
    fn create_metadata_with_paths() -> FactTableMetadata {
        use crate::compiler::fact_table::DimensionPath;
        let mut meta = create_test_metadata();
        meta.dimensions.paths = vec![DimensionPath {
            name:      "category".to_string(),
            json_path: "dimensions->>'category'".to_string(),
            data_type: "text".to_string(),
        }];
        meta
    }

    #[test]
    fn test_dimension_allowlist_accepts_declared_path() {
        let metadata = create_metadata_with_paths();
        let request = AggregationRequest {
            table_name:   "tf_sales".to_string(),
            where_clause: None,
            group_by:     vec![GroupBySelection::Dimension {
                path:  "category".to_string(),
                alias: "category".to_string(),
            }],
            aggregates:   vec![AggregateSelection::Count {
                alias: "count".to_string(),
            }],
            having:       vec![],
            order_by:     vec![],
            limit:        None,
            offset:       None,
        };
        AggregationPlanner::plan(request, metadata)
            .unwrap_or_else(|e| panic!("declared dimension path should be accepted: {e}"));
    }

    #[test]
    fn test_dimension_allowlist_rejects_unknown_path() {
        let metadata = create_metadata_with_paths();
        let request = AggregationRequest {
            table_name:   "tf_sales".to_string(),
            where_clause: None,
            group_by:     vec![GroupBySelection::Dimension {
                path:  "undeclared_path".to_string(),
                alias: "x".to_string(),
            }],
            aggregates:   vec![AggregateSelection::Count {
                alias: "count".to_string(),
            }],
            having:       vec![],
            order_by:     vec![],
            limit:        None,
            offset:       None,
        };
        let result = AggregationPlanner::plan(request, metadata);
        assert!(
            matches!(&result, Err(FraiseQLError::Validation { message, .. }) if message.contains("not found")),
            "expected Validation error about undeclared dimension path, got: {result:?}"
        );
    }

    #[test]
    fn test_dimension_allowlist_accepts_any_path_when_paths_empty() {
        // When metadata.dimensions.paths is empty, any path is allowed
        // (schema did not declare a dimension allowlist).
        let metadata = create_test_metadata(); // paths: vec![]
        let request = AggregationRequest {
            table_name:   "tf_sales".to_string(),
            where_clause: None,
            group_by:     vec![GroupBySelection::Dimension {
                path:  "any_undeclared_path".to_string(),
                alias: "x".to_string(),
            }],
            aggregates:   vec![AggregateSelection::Count {
                alias: "count".to_string(),
            }],
            having:       vec![],
            order_by:     vec![],
            limit:        None,
            offset:       None,
        };
        AggregationPlanner::plan(request, metadata)
            .unwrap_or_else(|e| panic!("any path should be accepted when paths empty: {e}"));
    }
}

/// #1014 — the two ORDER BY conversions on the window path need *different*
/// allowlists, and getting them the same way round is the whole point.
///
/// | Clause | Runs | Accepts |
/// |---|---|---|
/// | in-window, inside `OVER (…)` | before the window functions produce anything | measures, filter columns, dimension paths |
/// | the final ORDER BY | after the window functions | the above **plus** every window and select alias |
mod window_order_by_allowlist_tests {
    use crate::compiler::{
        aggregation::OrderDirection,
        fact_table::{
            DimensionColumn, DimensionPath, FactTableMetadata, FilterColumn, MeasureColumn, SqlType,
        },
        window_functions::{
            WindowFunctionRequest, WindowFunctionSpec, WindowOrderBy, WindowPlanner, WindowRequest,
            WindowSelectColumn,
        },
    };

    fn metadata() -> FactTableMetadata {
        FactTableMetadata {
            table_name:               "tf_sales".to_string(),
            type_name:                None,
            measures:                 vec![MeasureColumn {
                name:       "revenue".to_string(),
                sql_type:   SqlType::Decimal,
                nullable:   false,
                additivity: crate::compiler::fact_table::Additivity::Additive,
            }],
            dimensions:               DimensionColumn {
                name:  "dimensions".to_string(),
                paths: vec![DimensionPath {
                    name:      "category".to_string(),
                    json_path: "dimensions->>'category'".to_string(),
                    data_type: "text".to_string(),
                }],
            },
            denormalized_filters:     vec![FilterColumn {
                name:      "occurred_at".to_string(),
                sql_type:  SqlType::Timestamp,
                indexed:   true,
                hierarchy: None,
            }],
            calendar_dimensions:      vec![],
            native_measures:          std::collections::HashMap::new(),
            native_dimension_mapping: std::collections::HashMap::new(),
        }
    }

    /// A request with one `rank()` window aliased `rank`, one selected measure
    /// aliased `total`, and a caller-chosen final ORDER BY.
    fn request_ordering_by(field: &str) -> WindowRequest {
        WindowRequest {
            table_name:   "tf_sales".to_string(),
            select:       vec![WindowSelectColumn::Measure {
                name:  "revenue".to_string(),
                alias: "total".to_string(),
            }],
            windows:      vec![WindowFunctionRequest {
                function:     WindowFunctionSpec::Rank,
                alias:        "rank".to_string(),
                partition_by: vec![],
                order_by:     vec![],
                frame:        None,
            }],
            where_clause: None,
            order_by:     vec![WindowOrderBy {
                field:     field.to_string(),
                direction: OrderDirection::Desc,
            }],
            limit:        None,
            offset:       None,
        }
    }

    /// **#1014's literal ask.** The final ORDER BY runs after the window
    /// functions, so `rank` is a real output column and must be emitted as one: the quoted
    /// alias the SELECT list declares (#1516).
    /// Asserting on the generated expression rather than on absence of an error
    /// is load-bearing: before this, the clause "succeeded" as
    /// `dimensions->>'rank'`, which is NULL, which sorted nothing.
    #[test]
    fn the_final_order_by_accepts_a_window_alias_and_emits_it_as_a_column() {
        let plan = WindowPlanner::plan(request_ordering_by("rank"), &metadata())
            .expect("ordering by a window alias is #1014's ask");
        assert_eq!(
            plan.order_by[0].field, r#""rank""#,
            "a window alias must be a column, not a JSONB dimension read"
        );
    }

    #[test]
    fn the_final_order_by_accepts_a_select_alias_and_emits_it_as_a_column() {
        let plan = WindowPlanner::plan(request_ordering_by("total"), &metadata())
            .expect("ordering by a select alias must work");
        assert_eq!(plan.order_by[0].field, r#""total""#);
    }

    #[test]
    fn the_final_order_by_still_accepts_a_measure_and_a_filter_column() {
        for (field, expected) in [("revenue", "revenue"), ("occurred_at", "occurred_at")] {
            let plan = WindowPlanner::plan(request_ordering_by(field), &metadata())
                .unwrap_or_else(|e| panic!("'{field}' must remain a valid sort key: {e}"));
            assert_eq!(plan.order_by[0].field, expected);
        }
    }

    #[test]
    fn the_final_order_by_still_accepts_a_dimension_path() {
        let plan = WindowPlanner::plan(request_ordering_by("category"), &metadata())
            .expect("a declared dimension is a valid sort key");
        assert_eq!(plan.order_by[0].field, "dimensions->>'category'");
    }

    /// The generalisation beyond #1014: a name that is neither a schema field
    /// nor an output alias used to become `dimensions->>'nope'` — NULL, sorting
    /// nothing, silently.
    #[test]
    fn the_final_order_by_refuses_a_name_that_is_neither_field_nor_alias() {
        let err = WindowPlanner::plan(request_ordering_by("totallyBogusField"), &metadata())
            .expect_err("an unknown sort key must not silently sort nothing");
        assert!(
            err.to_string().contains("totallyBogusField"),
            "the error must name the field: {err}"
        );
    }

    /// The asymmetry that is the whole phase: inside `OVER (…)` the window
    /// columns do not exist yet, so a window function cannot order by its own
    /// sibling's alias.
    #[test]
    fn the_in_window_order_by_refuses_a_window_alias() {
        let mut request = request_ordering_by("revenue");
        request.windows[0].order_by = vec![WindowOrderBy {
            field:     "rank".to_string(),
            direction: OrderDirection::Desc,
        }];
        let err = WindowPlanner::plan(request, &metadata())
            .expect_err("`rank` is not a column inside the OVER(...) that produces it");
        assert!(err.to_string().contains("rank"), "got: {err}");
    }

    #[test]
    fn the_in_window_order_by_still_accepts_a_measure_and_a_dimension() {
        for field in ["revenue", "occurred_at", "category"] {
            let mut request = request_ordering_by("revenue");
            request.windows[0].order_by = vec![WindowOrderBy {
                field:     field.to_string(),
                direction: OrderDirection::Desc,
            }];
            WindowPlanner::plan(request, &metadata())
                .unwrap_or_else(|e| panic!("'{field}' is valid inside OVER(...): {e}"));
        }
    }

    #[test]
    fn the_in_window_order_by_refuses_an_unknown_name() {
        let mut request = request_ordering_by("revenue");
        request.windows[0].order_by = vec![WindowOrderBy {
            field:     "totallyBogusField".to_string(),
            direction: OrderDirection::Desc,
        }];
        WindowPlanner::plan(request, &metadata())
            .expect_err("an unknown in-window sort key must be refused");
    }

    /// **Control** — empty metadata declares no schema constraints, so the
    /// allowlist is a no-op and both paths still execute. That "cannot
    /// adjudicate → pass" property is inherited from `WindowAllowlist::validate`
    /// and is what keeps this from breaking schemas with no fact-table metadata.
    #[test]
    fn empty_metadata_cannot_adjudicate_and_still_executes() {
        let empty = FactTableMetadata {
            table_name:               "tf_sales".to_string(),
            type_name:                None,
            measures:                 vec![],
            dimensions:               DimensionColumn {
                name:  "dimensions".to_string(),
                paths: vec![],
            },
            denormalized_filters:     vec![],
            calendar_dimensions:      vec![],
            native_measures:          std::collections::HashMap::new(),
            native_dimension_mapping: std::collections::HashMap::new(),
        };
        let mut request = request_ordering_by("anythingAtAll");
        request.select = vec![];
        request.windows = vec![];
        WindowPlanner::plan(request, &empty)
            .expect("no schema constraints declared — a rejection cannot be justified");
    }
}
