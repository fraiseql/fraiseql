//! Mutation-targeted tests for `compiler/aggregation.rs`.
//!
//! These tests kill surviving mutants that the existing inline tests do not cover.
//!
//! ## Targeted mutations
//!
//! | Mutant | Location | What cargo-mutants changes | Killed by |
//! |--------|----------|---------------------------|-----------|
//! | B1 | line 122-128 | `GroupBySelection::alias()` replaced with `""` | `group_by_alias_returns_correct_value_*` |
//! | B2 | line 169-175 | `AggregateSelection::alias()` replaced with `""` | `aggregate_alias_returns_correct_value_*` |
//! | B3 | line 425 | `measure_exists` negated | `count_distinct_rejects_unknown_field` |
//! | B4 | line 453 | triple-guard `&&` becomes `||` | `measure_aggregate_accepts_dimension_path_as_measure` |
//! | B5 | line 475 | `==` becomes `!=` for StringAgg delimiter | `string_agg_gets_comma_delimiter` |
//! | B6 | line 475 | `Some` becomes `None` for StringAgg delimiter | `non_string_agg_gets_no_delimiter` |
//! | B7 | line 496 | `||` becomes `&&` for bool field check | `bool_aggregate_accepts_filter_column` |
//!
//! **Do not merge tests** — each test targets exactly one mutation.

#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#![allow(missing_docs)] // Reason: test code

use fraiseql_core::compiler::aggregation::{
    AggregateExpression, AggregateSelection, AggregationPlanner, AggregationRequest,
    GroupByExpression, GroupBySelection,
};
use fraiseql_core::compiler::aggregate_types::{
    AggregateFunction, BoolAggregateFunction, TemporalBucket,
};
use fraiseql_core::compiler::fact_table::{
    DimensionColumn, DimensionPath, FactTableMetadata, FilterColumn, MeasureColumn, SqlType,
};

// ── Shared fixtures ───────────────────────────────────────────────────────────

fn base_metadata() -> FactTableMetadata {
    FactTableMetadata {
        table_name:           "tf_orders".to_string(),
        measures:             vec![MeasureColumn {
            name:     "amount".to_string(),
            sql_type: SqlType::Decimal,
            nullable: false,
        }],
        dimensions:           DimensionColumn {
            name:  "dims".to_string(),
            paths: vec![DimensionPath {
                name:      "region".to_string(),
                json_path: "dims->>'region'".to_string(),
                data_type: "text".to_string(),
            }],
        },
        denormalized_filters: vec![
            FilterColumn {
                name:     "created_at".to_string(),
                sql_type: SqlType::Timestamp,
                indexed:  true,
            },
            FilterColumn {
                name:     "is_paid".to_string(),
                sql_type: SqlType::Boolean,
                indexed:  false,
            },
        ],
        calendar_dimensions:  vec![],
    }
}

fn simple_request_with_group_by(group_by: Vec<GroupBySelection>) -> AggregationRequest {
    AggregationRequest {
        table_name:   "tf_orders".to_string(),
        where_clause: None,
        group_by,
        aggregates:   vec![AggregateSelection::Count {
            alias: "count".to_string(),
        }],
        having:       vec![],
        order_by:     vec![],
        limit:        None,
        offset:       None,
    }
}

fn simple_request_with_aggregates(aggregates: Vec<AggregateSelection>) -> AggregationRequest {
    AggregationRequest {
        table_name:   "tf_orders".to_string(),
        where_clause: None,
        group_by:     vec![],
        aggregates,
        having:       vec![],
        order_by:     vec![],
        limit:        None,
        offset:       None,
    }
}

// ── B1: GroupBySelection::alias() ─────────────────────────────────────────────

/// B1a: alias() on Dimension variant must return the declared alias, not "".
///
/// Mutation "replace with `\"\"` " would make `alias()` always return "" for all variants.
#[test]
fn group_by_alias_dimension_returns_correct_value() {
    let sel = GroupBySelection::Dimension {
        path:  "region".to_string(),
        alias: "sale_region".to_string(),
    };
    assert_eq!(sel.alias(), "sale_region", "B1a: Dimension alias must match declared value");
    assert_ne!(sel.alias(), "", "B1a: alias must not be empty string");
}

/// B1b: alias() on TemporalBucket variant.
#[test]
fn group_by_alias_temporal_bucket_returns_correct_value() {
    let sel = GroupBySelection::TemporalBucket {
        column: "created_at".to_string(),
        bucket: TemporalBucket::Month,
        alias:  "created_month".to_string(),
    };
    assert_eq!(sel.alias(), "created_month", "B1b: TemporalBucket alias must match");
    assert_ne!(sel.alias(), "", "B1b: alias must not be empty string");
}

/// B1c: alias() on CalendarDimension variant.
#[test]
fn group_by_alias_calendar_dimension_returns_correct_value() {
    let sel = GroupBySelection::CalendarDimension {
        source_column:   "created_at".to_string(),
        calendar_column: "event_date".to_string(),
        json_key:        "quarter".to_string(),
        bucket:          TemporalBucket::Month,
        alias:           "event_quarter".to_string(),
    };
    assert_eq!(sel.alias(), "event_quarter", "B1c: CalendarDimension alias must match");
    assert_ne!(sel.alias(), "", "B1c: alias must not be empty string");
}

/// B1d: Different aliases must produce different return values (not just non-empty).
#[test]
fn group_by_alias_distinguishes_different_aliases() {
    let a = GroupBySelection::Dimension {
        path:  "x".to_string(),
        alias: "alias_a".to_string(),
    };
    let b = GroupBySelection::Dimension {
        path:  "x".to_string(),
        alias: "alias_b".to_string(),
    };
    assert_ne!(a.alias(), b.alias(), "B1d: different aliases must differ");
}

// ── B2: AggregateSelection::alias() ──────────────────────────────────────────

/// B2a: alias() on Count variant.
#[test]
fn aggregate_alias_count_returns_correct_value() {
    let sel = AggregateSelection::Count {
        alias: "order_count".to_string(),
    };
    assert_eq!(sel.alias(), "order_count", "B2a: Count alias must match");
    assert_ne!(sel.alias(), "", "B2a: alias must not be empty string");
}

/// B2b: alias() on CountDistinct variant.
#[test]
fn aggregate_alias_count_distinct_returns_correct_value() {
    let sel = AggregateSelection::CountDistinct {
        field: "amount".to_string(),
        alias: "unique_amounts".to_string(),
    };
    assert_eq!(sel.alias(), "unique_amounts", "B2b: CountDistinct alias must match");
}

/// B2c: alias() on MeasureAggregate variant.
#[test]
fn aggregate_alias_measure_aggregate_returns_correct_value() {
    let sel = AggregateSelection::MeasureAggregate {
        measure:  "amount".to_string(),
        function: AggregateFunction::Sum,
        alias:    "total_amount".to_string(),
    };
    assert_eq!(sel.alias(), "total_amount", "B2c: MeasureAggregate alias must match");
}

/// B2d: alias() on BoolAggregate variant.
#[test]
fn aggregate_alias_bool_aggregate_returns_correct_value() {
    let sel = AggregateSelection::BoolAggregate {
        field:    "is_paid".to_string(),
        function: BoolAggregateFunction::And,
        alias:    "all_paid".to_string(),
    };
    assert_eq!(sel.alias(), "all_paid", "B2d: BoolAggregate alias must match");
}

// ── B3: CountDistinct measure validation (line 425) ──────────────────────────

/// B3: CountDistinct must reject a field that is not a measure.
///
/// Mutation that negates `measure_exists` would accept any field, including invented ones.
/// This test ensures the validation gate actually fires.
#[test]
fn count_distinct_rejects_unknown_field() {
    let metadata = base_metadata();
    let request = simple_request_with_aggregates(vec![AggregateSelection::CountDistinct {
        field: "completely_unknown".to_string(),
        alias: "x".to_string(),
    }]);
    let result = AggregationPlanner::plan(request, metadata);
    assert!(result.is_err(), "B3: unknown field in CountDistinct must fail");
    let msg = result.unwrap_err().to_string();
    assert!(msg.contains("not found"), "B3: error must mention 'not found', got: {msg}");
}

/// B3b: CountDistinct must accept a known measure.
#[test]
fn count_distinct_accepts_known_measure() {
    let metadata = base_metadata();
    let request = simple_request_with_aggregates(vec![AggregateSelection::CountDistinct {
        field: "amount".to_string(),
        alias: "distinct_amounts".to_string(),
    }]);
    assert!(
        AggregationPlanner::plan(request, metadata).is_ok(),
        "B3b: known measure in CountDistinct must succeed"
    );
}

// ── B4: MeasureAggregate triple-guard (lines 448-453) ────────────────────────

/// B4a: MeasureAggregate should accept a declared dimension path as the measure field.
///
/// The triple guard `!measure_exists && !is_dimension && !is_filter` allows dimension
/// paths to be used in aggregates. Mutating `&&` → `||` would reject valid dimension paths.
#[test]
fn measure_aggregate_accepts_dimension_path_as_measure() {
    let metadata = base_metadata(); // "region" is declared as a dimension path
    let request = simple_request_with_aggregates(vec![AggregateSelection::MeasureAggregate {
        measure:  "region".to_string(), // dimension path, not a measure
        function: AggregateFunction::Count,
        alias:    "region_count".to_string(),
    }]);
    let result = AggregationPlanner::plan(request, metadata);
    assert!(result.is_ok(), "B4a: declared dimension path must be accepted as measure aggregate");
}

/// B4b: MeasureAggregate should accept a denormalized filter column as measure.
#[test]
fn measure_aggregate_accepts_filter_column_as_measure() {
    let metadata = base_metadata(); // "created_at" is a denormalized filter
    let request = simple_request_with_aggregates(vec![AggregateSelection::MeasureAggregate {
        measure:  "created_at".to_string(),
        function: AggregateFunction::Max,
        alias:    "last_created".to_string(),
    }]);
    let result = AggregationPlanner::plan(request, metadata);
    assert!(
        result.is_ok(),
        "B4b: denormalized filter column must be accepted as measure aggregate"
    );
}

/// B4c: MeasureAggregate must still reject a completely unknown field.
#[test]
fn measure_aggregate_rejects_completely_unknown_field() {
    let metadata = base_metadata();
    let request = simple_request_with_aggregates(vec![AggregateSelection::MeasureAggregate {
        measure:  "invented_field".to_string(),
        function: AggregateFunction::Sum,
        alias:    "x".to_string(),
    }]);
    let result = AggregationPlanner::plan(request, metadata);
    assert!(result.is_err(), "B4c: completely unknown field must fail");
}

// ── B5/B6: StringAgg delimiter logic (lines 464-479) ─────────────────────────

/// B5: StringAgg must produce an AdvancedAggregate with a delimiter.
///
/// Mutation `== becomes !=` would give StringAgg `None` and give other functions `Some(", ")`.
#[test]
fn string_agg_produces_advanced_aggregate_with_delimiter() {
    let metadata = base_metadata();
    let request = simple_request_with_aggregates(vec![AggregateSelection::MeasureAggregate {
        measure:  "amount".to_string(),
        function: AggregateFunction::StringAgg,
        alias:    "amounts_joined".to_string(),
    }]);
    let plan = AggregationPlanner::plan(request, metadata).unwrap();
    let expr = &plan.aggregate_expressions[0];
    match expr {
        AggregateExpression::AdvancedAggregate { delimiter, .. } => {
            assert!(
                delimiter.is_some(),
                "B5: StringAgg must have a delimiter, got None"
            );
            assert_eq!(
                delimiter.as_deref(),
                Some(", "),
                "B5: StringAgg delimiter must be \", \""
            );
        },
        other => panic!("B5: StringAgg must produce AdvancedAggregate, got: {other:?}"),
    }
}

/// B6: Non-StringAgg advanced functions (e.g. ArrayAgg) must have no delimiter.
///
/// Mutation `Some becomes None` would strip the StringAgg delimiter, but equally
/// a mutation `None becomes Some(", ")` would add a spurious delimiter to non-string aggs.
#[test]
fn array_agg_produces_advanced_aggregate_without_delimiter() {
    let metadata = base_metadata();
    let request = simple_request_with_aggregates(vec![AggregateSelection::MeasureAggregate {
        measure:  "amount".to_string(),
        function: AggregateFunction::ArrayAgg,
        alias:    "amounts_array".to_string(),
    }]);
    let plan = AggregationPlanner::plan(request, metadata).unwrap();
    let expr = &plan.aggregate_expressions[0];
    match expr {
        AggregateExpression::AdvancedAggregate { delimiter, .. } => {
            assert!(
                delimiter.is_none(),
                "B6: ArrayAgg must have no delimiter, got: {delimiter:?}"
            );
        },
        other => panic!("B6: ArrayAgg must produce AdvancedAggregate, got: {other:?}"),
    }
}

// ── B7: BoolAggregate field validation (lines 496-499) ───────────────────────

/// B7a: BoolAggregate accepts a filter column (denormalized_filters path).
///
/// Mutation `||` → `&&` would require the field to be in BOTH dimensions AND filters,
/// causing this valid request to fail.
#[test]
fn bool_aggregate_accepts_filter_column() {
    let metadata = base_metadata(); // "is_paid" is a filter column, not a dimension path
    let request = simple_request_with_aggregates(vec![AggregateSelection::BoolAggregate {
        field:    "is_paid".to_string(),
        function: BoolAggregateFunction::Or,
        alias:    "any_paid".to_string(),
    }]);
    let result = AggregationPlanner::plan(request, metadata);
    assert!(result.is_ok(), "B7a: filter column must be accepted in BoolAggregate");
}

/// B7b: BoolAggregate accepts a declared dimension path.
#[test]
fn bool_aggregate_accepts_dimension_path() {
    let metadata = base_metadata(); // "region" is a declared dimension path
    let request = simple_request_with_aggregates(vec![AggregateSelection::BoolAggregate {
        field:    "region".to_string(),
        function: BoolAggregateFunction::And,
        alias:    "all_regions".to_string(),
    }]);
    let result = AggregationPlanner::plan(request, metadata);
    assert!(result.is_ok(), "B7b: declared dimension path must be accepted in BoolAggregate");
}

/// B7c: BoolAggregate must reject a completely unknown field.
#[test]
fn bool_aggregate_rejects_unknown_field() {
    let metadata = base_metadata();
    let request = simple_request_with_aggregates(vec![AggregateSelection::BoolAggregate {
        field:    "totally_unknown".to_string(),
        function: BoolAggregateFunction::And,
        alias:    "x".to_string(),
    }]);
    let result = AggregationPlanner::plan(request, metadata);
    assert!(result.is_err(), "B7c: unknown field must fail in BoolAggregate");
    let msg = result.unwrap_err().to_string();
    assert!(msg.contains("not found"), "B7c: error must mention 'not found', got: {msg}");
}

// ── Plan structure integrity ──────────────────────────────────────────────────

/// Verify that the plan output properly propagates the alias from GroupBySelection.
///
/// This catches mutations that strip the alias before storing in the plan.
#[test]
fn plan_group_by_alias_is_propagated_into_expression() {
    let metadata = base_metadata();
    let request = simple_request_with_group_by(vec![GroupBySelection::Dimension {
        path:  "region".to_string(),
        alias: "sale_region".to_string(),
    }]);
    let plan = AggregationPlanner::plan(request, metadata).unwrap();
    match &plan.group_by_expressions[0] {
        GroupByExpression::JsonbPath { alias, .. } => {
            assert_eq!(alias, "sale_region", "alias must be propagated into plan");
        },
        other => panic!("expected JsonbPath, got: {other:?}"),
    }
}

/// Verify the aggregate alias is propagated into AggregateExpression::Count.
#[test]
fn plan_aggregate_alias_is_propagated_into_count_expression() {
    let metadata = base_metadata();
    let request = simple_request_with_aggregates(vec![AggregateSelection::Count {
        alias: "total_orders".to_string(),
    }]);
    let plan = AggregationPlanner::plan(request, metadata).unwrap();
    match &plan.aggregate_expressions[0] {
        AggregateExpression::Count { alias } => {
            assert_eq!(alias, "total_orders", "alias must be propagated into Count expression");
        },
        other => panic!("expected Count, got: {other:?}"),
    }
}
