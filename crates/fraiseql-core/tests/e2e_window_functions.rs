#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

//! End-to-end window function tests.
//!
//! These drive the exact three calls the shipped binary makes —
//! `WindowQueryParser::parse` -> `WindowPlanner::plan` -> `WindowSqlGenerator::generate`
//! — so what they pin is the SQL a real request produces.
//!
//! They were originally written against `WindowFunctionPlanner`, a second planner that
//! nothing outside tests ever called. That split was the root cause of #794: the
//! identifier allowlist was wired into the planner nobody runs, so every guard test
//! passed while the live path interpolated client strings verbatim. #881 deleted that
//! planner and moved this coverage onto the live request shape.

mod common;

use common::{assert_sql_contains, create_sales_metadata};
use fraiseql_core::{
    compiler::window_functions::WindowPlanner,
    db::types::DatabaseType,
    runtime::{WindowQueryParser, WindowSqlGenerator},
};
use serde_json::json;

// =============================================================================
// Helper Functions
// =============================================================================

/// Parse, plan and generate exactly as the runtime does.
///
/// `query` is the client-supplied `variables` object verbatim — measures, dimensions
/// and filter columns are named semantically and resolved against
/// [`create_sales_metadata`], which declares measures `revenue`/`quantity`, the JSONB
/// dimension column `data` with paths `category`/`region`, and the denormalized filter
/// columns `customer_id`/`occurred_at`.
fn plan_and_generate(query: &serde_json::Value, db_type: DatabaseType) -> String {
    let metadata = create_sales_metadata();
    let request = WindowQueryParser::parse(query, &metadata).unwrap();
    let plan = WindowPlanner::plan(request, &metadata).unwrap();

    WindowSqlGenerator::new(db_type).generate(&plan).unwrap().raw_sql
}

fn plan_and_generate_pg(query: &serde_json::Value) -> String {
    plan_and_generate(query, DatabaseType::PostgreSQL)
}

/// The `select` clause used by most tests: the `revenue` measure and the `category`
/// dimension, which the planner renders as `revenue` and `data->>'category'`.
fn select_revenue_and_category() -> serde_json::Value {
    json!([
        {"type": "measure", "name": "revenue", "alias": "revenue"},
        {"type": "dimension", "path": "category", "alias": "category"}
    ])
}

fn select_revenue() -> serde_json::Value {
    json!([{"type": "measure", "name": "revenue", "alias": "revenue"}])
}

fn select_occurred_at_and_revenue() -> serde_json::Value {
    json!([
        {"type": "filter", "name": "occurred_at", "alias": "occurred_at"},
        {"type": "measure", "name": "revenue", "alias": "revenue"}
    ])
}

// =============================================================================
// Ranking Function Tests
// =============================================================================

#[test]
fn test_row_number_simple() {
    let query = json!({
        "table": "tf_sales",
        "select": select_revenue_and_category(),
        "windows": [{
            "function": {"type": "row_number"},
            "alias": "rank",
            "partitionBy": [{"type": "dimension", "path": "category"}],
            "orderBy": [{"field": "revenue", "direction": "DESC"}]
        }]
    });

    let sql = plan_and_generate_pg(&query);

    assert_sql_contains(
        &sql,
        &[
            "SELECT",
            "revenue",
            "ROW_NUMBER()",
            "OVER",
            "PARTITION BY data->>'category'",
            "ORDER BY revenue DESC",
            "AS rank",
            "FROM tf_sales",
        ],
    );
}

#[test]
fn test_rank_with_gaps() {
    let query = json!({
        "table": "tf_sales",
        "select": select_revenue(),
        "windows": [{
            "function": {"type": "rank"},
            "alias": "revenue_rank",
            "partitionBy": [],
            "orderBy": [{"field": "revenue", "direction": "DESC"}]
        }]
    });

    let sql = plan_and_generate_pg(&query);

    assert_sql_contains(&sql, &["RANK()", "OVER", "ORDER BY revenue DESC", "AS revenue_rank"]);
}

#[test]
fn test_dense_rank_no_gaps() {
    let query = json!({
        "table": "tf_sales",
        "select": select_revenue(),
        "windows": [{
            "function": {"type": "dense_rank"},
            "alias": "dense_rank",
            "partitionBy": [],
            "orderBy": [{"field": "revenue", "direction": "DESC"}]
        }]
    });

    let sql = plan_and_generate_pg(&query);

    assert_sql_contains(&sql, &["DENSE_RANK()", "OVER", "ORDER BY revenue DESC"]);
}

#[test]
fn test_ntile_quartiles() {
    let query = json!({
        "table": "tf_sales",
        "select": select_revenue(),
        "windows": [{
            "function": {"type": "ntile", "n": 4},
            "alias": "quartile",
            "partitionBy": [],
            "orderBy": [{"field": "revenue", "direction": "ASC"}]
        }]
    });

    let sql = plan_and_generate_pg(&query);

    assert_sql_contains(&sql, &["NTILE(4)", "OVER", "ORDER BY revenue ASC", "AS quartile"]);
}

#[test]
fn test_percent_rank() {
    let query = json!({
        "table": "tf_sales",
        "select": select_revenue(),
        "windows": [{
            "function": {"type": "percent_rank"},
            "alias": "pct_rank",
            "partitionBy": [{"type": "dimension", "path": "category"}],
            "orderBy": [{"field": "revenue", "direction": "DESC"}]
        }]
    });

    let sql = plan_and_generate_pg(&query);

    assert_sql_contains(
        &sql,
        &[
            "PERCENT_RANK()",
            "OVER",
            "PARTITION BY data->>'category'",
            "ORDER BY revenue DESC",
        ],
    );
}

#[test]
fn test_cume_dist() {
    let query = json!({
        "table": "tf_sales",
        "select": select_revenue(),
        "windows": [{
            "function": {"type": "cume_dist"},
            "alias": "cumulative_distribution",
            "partitionBy": [],
            "orderBy": [{"field": "revenue", "direction": "ASC"}]
        }]
    });

    let sql = plan_and_generate_pg(&query);

    assert_sql_contains(&sql, &["CUME_DIST()", "OVER", "ORDER BY revenue ASC"]);
}

// =============================================================================
// Value Function Tests (LAG/LEAD/FIRST_VALUE/LAST_VALUE)
// =============================================================================

#[test]
fn test_lag_previous_value() {
    let query = json!({
        "table": "tf_sales",
        "select": select_occurred_at_and_revenue(),
        "windows": [{
            "function": {
                "type": "lag",
                "field": "revenue",
                "offset": 1,
                "default": 0
            },
            "alias": "prev_revenue",
            "partitionBy": [],
            "orderBy": [{"field": "occurred_at", "direction": "ASC"}]
        }]
    });

    let sql = plan_and_generate_pg(&query);

    assert_sql_contains(
        &sql,
        &[
            "LAG(revenue, 1, 0)",
            "OVER",
            "ORDER BY occurred_at ASC",
            "AS prev_revenue",
        ],
    );
}

#[test]
fn test_lead_next_value() {
    let query = json!({
        "table": "tf_sales",
        "select": select_occurred_at_and_revenue(),
        "windows": [{
            "function": {
                "type": "lead",
                "field": "revenue",
                "offset": 1,
                "default": 0
            },
            "alias": "next_revenue",
            "partitionBy": [],
            "orderBy": [{"field": "occurred_at", "direction": "ASC"}]
        }]
    });

    let sql = plan_and_generate_pg(&query);

    assert_sql_contains(
        &sql,
        &[
            "LEAD(revenue, 1, 0)",
            "OVER",
            "ORDER BY occurred_at ASC",
            "AS next_revenue",
        ],
    );
}

#[test]
fn test_first_value() {
    let query = json!({
        "table": "tf_sales",
        "select": select_occurred_at_and_revenue(),
        "windows": [{
            "function": {
                "type": "first_value",
                "field": "revenue"
            },
            "alias": "first_revenue",
            "partitionBy": [{"type": "dimension", "path": "category"}],
            "orderBy": [{"field": "occurred_at", "direction": "ASC"}]
        }]
    });

    let sql = plan_and_generate_pg(&query);

    assert_sql_contains(
        &sql,
        &[
            "FIRST_VALUE(revenue)",
            "OVER",
            "PARTITION BY data->>'category'",
            "ORDER BY occurred_at ASC",
        ],
    );
}

#[test]
fn test_last_value() {
    let query = json!({
        "table": "tf_sales",
        "select": select_occurred_at_and_revenue(),
        "windows": [{
            "function": {
                "type": "last_value",
                "field": "revenue"
            },
            "alias": "last_revenue",
            "partitionBy": [{"type": "dimension", "path": "category"}],
            "orderBy": [{"field": "occurred_at", "direction": "ASC"}],
            "frame": {
                "frame_type": "ROWS",
                "start": {"type": "unbounded_preceding"},
                "end": {"type": "unbounded_following"}
            }
        }]
    });

    let sql = plan_and_generate_pg(&query);

    assert_sql_contains(
        &sql,
        &[
            "LAST_VALUE(revenue)",
            "OVER",
            "PARTITION BY data->>'category'",
            "ORDER BY occurred_at ASC",
            "ROWS BETWEEN UNBOUNDED PRECEDING AND UNBOUNDED FOLLOWING",
        ],
    );
}

#[test]
fn test_nth_value() {
    let query = json!({
        "table": "tf_sales",
        "select": select_occurred_at_and_revenue(),
        "windows": [{
            "function": {
                "type": "nth_value",
                "field": "revenue",
                "n": 3
            },
            "alias": "third_revenue",
            "partitionBy": [{"type": "dimension", "path": "category"}],
            "orderBy": [{"field": "occurred_at", "direction": "ASC"}]
        }]
    });

    let sql = plan_and_generate_pg(&query);

    assert_sql_contains(
        &sql,
        &[
            "NTH_VALUE(revenue, 3)",
            "OVER",
            "PARTITION BY data->>'category'",
            "ORDER BY occurred_at ASC",
        ],
    );
}

// =============================================================================
// Aggregate as Window Function Tests
// =============================================================================

#[test]
fn test_running_total_sum() {
    let query = json!({
        "table": "tf_sales",
        "select": select_occurred_at_and_revenue(),
        "windows": [{
            "function": {
                "type": "running_sum",
                "measure": "revenue"
            },
            "alias": "running_total",
            "partitionBy": [],
            "orderBy": [{"field": "occurred_at", "direction": "ASC"}],
            "frame": {
                "frame_type": "ROWS",
                "start": {"type": "unbounded_preceding"},
                "end": {"type": "current_row"}
            }
        }]
    });

    let sql = plan_and_generate_pg(&query);

    assert_sql_contains(
        &sql,
        &[
            "SUM(revenue)",
            "OVER",
            "ORDER BY occurred_at ASC",
            "ROWS BETWEEN UNBOUNDED PRECEDING AND CURRENT ROW",
            "AS running_total",
        ],
    );
}

#[test]
fn test_moving_average() {
    let query = json!({
        "table": "tf_sales",
        "select": select_occurred_at_and_revenue(),
        "windows": [{
            "function": {
                "type": "running_avg",
                "measure": "revenue"
            },
            "alias": "moving_avg_3",
            "partitionBy": [],
            "orderBy": [{"field": "occurred_at", "direction": "ASC"}],
            "frame": {
                "frame_type": "ROWS",
                "start": {"type": "n_preceding", "n": 2},
                "end": {"type": "current_row"}
            }
        }]
    });

    let sql = plan_and_generate_pg(&query);

    assert_sql_contains(
        &sql,
        &[
            "AVG(revenue)",
            "OVER",
            "ORDER BY occurred_at ASC",
            "ROWS BETWEEN 2 PRECEDING AND CURRENT ROW",
            "AS moving_avg_3",
        ],
    );
}

#[test]
fn test_running_count() {
    let query = json!({
        "table": "tf_sales",
        "select": [{"type": "filter", "name": "occurred_at", "alias": "occurred_at"}],
        "windows": [{
            "function": {
                "type": "running_count"
            },
            "alias": "running_count",
            "partitionBy": [],
            "orderBy": [{"field": "occurred_at", "direction": "ASC"}],
            "frame": {
                "frame_type": "ROWS",
                "start": {"type": "unbounded_preceding"},
                "end": {"type": "current_row"}
            }
        }]
    });

    let sql = plan_and_generate_pg(&query);

    assert_sql_contains(
        &sql,
        &[
            "COUNT(*)",
            "OVER",
            "ORDER BY occurred_at ASC",
            "ROWS BETWEEN UNBOUNDED PRECEDING AND CURRENT ROW",
        ],
    );
}

#[test]
fn test_running_count_of_field() {
    // `running_count` with a `field` counts non-null values of that field rather than
    // rows. The dead planner had no spelling for this arm, so nothing exercised it.
    let query = json!({
        "table": "tf_sales",
        "select": select_occurred_at_and_revenue(),
        "windows": [{
            "function": {
                "type": "running_count",
                "field": "revenue"
            },
            "alias": "non_null_revenues",
            "partitionBy": [],
            "orderBy": [{"field": "occurred_at", "direction": "ASC"}]
        }]
    });

    let sql = plan_and_generate_pg(&query);

    assert_sql_contains(&sql, &["COUNT(revenue)", "OVER", "AS non_null_revenues"]);
}

#[test]
fn test_running_min_max() {
    let query = json!({
        "table": "tf_sales",
        "select": select_occurred_at_and_revenue(),
        "windows": [
            {
                "function": {"type": "running_min", "measure": "revenue"},
                "alias": "running_min",
                "partitionBy": [],
                "orderBy": [{"field": "occurred_at", "direction": "ASC"}],
                "frame": {
                    "frame_type": "ROWS",
                    "start": {"type": "unbounded_preceding"},
                    "end": {"type": "current_row"}
                }
            },
            {
                "function": {"type": "running_max", "measure": "revenue"},
                "alias": "running_max",
                "partitionBy": [],
                "orderBy": [{"field": "occurred_at", "direction": "ASC"}],
                "frame": {
                    "frame_type": "ROWS",
                    "start": {"type": "unbounded_preceding"},
                    "end": {"type": "current_row"}
                }
            }
        ]
    });

    let sql = plan_and_generate_pg(&query);

    assert_sql_contains(
        &sql,
        &[
            "MIN(revenue)",
            "MAX(revenue)",
            "OVER",
            "ORDER BY occurred_at ASC",
            "ROWS BETWEEN UNBOUNDED PRECEDING AND CURRENT ROW",
        ],
    );
}

// =============================================================================
// Frame Specification Tests
// =============================================================================

#[test]
fn test_frame_rows_preceding_following() {
    let query = json!({
        "table": "tf_sales",
        "select": select_occurred_at_and_revenue(),
        "windows": [{
            "function": {"type": "running_avg", "measure": "revenue"},
            "alias": "centered_avg",
            "partitionBy": [],
            "orderBy": [{"field": "occurred_at", "direction": "ASC"}],
            "frame": {
                "frame_type": "ROWS",
                "start": {"type": "n_preceding", "n": 3},
                "end": {"type": "n_following", "n": 3}
            }
        }]
    });

    let sql = plan_and_generate_pg(&query);

    assert_sql_contains(
        &sql,
        &[
            "AVG(revenue)",
            "OVER",
            "ROWS BETWEEN 3 PRECEDING AND 3 FOLLOWING",
        ],
    );
}

#[test]
fn test_frame_range() {
    let query = json!({
        "table": "tf_sales",
        "select": select_occurred_at_and_revenue(),
        "windows": [{
            "function": {"type": "running_sum", "measure": "revenue"},
            "alias": "range_sum",
            "partitionBy": [],
            "orderBy": [{"field": "occurred_at", "direction": "ASC"}],
            "frame": {
                "frame_type": "RANGE",
                "start": {"type": "unbounded_preceding"},
                "end": {"type": "current_row"}
            }
        }]
    });

    let sql = plan_and_generate_pg(&query);

    assert_sql_contains(
        &sql,
        &[
            "SUM(revenue)",
            "OVER",
            "RANGE BETWEEN UNBOUNDED PRECEDING AND CURRENT ROW",
        ],
    );
}

#[test]
fn test_frame_groups_postgres_only() {
    let query = json!({
        "table": "tf_sales",
        "select": select_occurred_at_and_revenue(),
        "windows": [{
            "function": {"type": "running_sum", "measure": "revenue"},
            "alias": "groups_sum",
            "partitionBy": [],
            "orderBy": [{"field": "revenue", "direction": "ASC"}],
            "frame": {
                "frame_type": "GROUPS",
                "start": {"type": "unbounded_preceding"},
                "end": {"type": "current_row"}
            }
        }]
    });

    let sql = plan_and_generate_pg(&query);

    assert_sql_contains(
        &sql,
        &[
            "SUM(revenue)",
            "OVER",
            "GROUPS BETWEEN UNBOUNDED PRECEDING AND CURRENT ROW",
        ],
    );
}

#[test]
fn test_frame_exclusion_postgres() {
    let query = json!({
        "table": "tf_sales",
        "select": select_occurred_at_and_revenue(),
        "windows": [{
            "function": {"type": "running_avg", "measure": "revenue"},
            "alias": "avg_excluding_current",
            "partitionBy": [],
            "orderBy": [{"field": "occurred_at", "direction": "ASC"}],
            "frame": {
                "frame_type": "ROWS",
                "start": {"type": "unbounded_preceding"},
                "end": {"type": "unbounded_following"},
                "exclusion": "current_row"
            }
        }]
    });

    let sql = plan_and_generate_pg(&query);

    assert_sql_contains(
        &sql,
        &[
            "AVG(revenue)",
            "OVER",
            "ROWS BETWEEN UNBOUNDED PRECEDING AND UNBOUNDED FOLLOWING",
            "EXCLUDE CURRENT ROW",
        ],
    );
}

// =============================================================================
// Composition Tests
// =============================================================================

#[test]
fn test_multiple_window_functions() {
    let query = json!({
        "table": "tf_sales",
        "select": [
            {"type": "filter", "name": "occurred_at", "alias": "occurred_at"},
            {"type": "measure", "name": "revenue", "alias": "revenue"},
            {"type": "dimension", "path": "category", "alias": "category"}
        ],
        "windows": [
            {
                "function": {"type": "row_number"},
                "alias": "row_num",
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
                "function": {"type": "lag", "field": "revenue", "offset": 1, "default": 0},
                "alias": "prev_revenue",
                "partitionBy": [{"type": "dimension", "path": "category"}],
                "orderBy": [{"field": "occurred_at", "direction": "ASC"}]
            }
        ]
    });

    let sql = plan_and_generate_pg(&query);

    assert_sql_contains(
        &sql,
        &[
            "ROW_NUMBER()",
            "SUM(revenue)",
            "LAG(revenue, 1, 0)",
            "PARTITION BY data->>'category'",
            "AS row_num",
            "AS running_total",
            "AS prev_revenue",
        ],
    );
}

#[test]
fn test_window_with_limit_offset() {
    let query = json!({
        "table": "tf_sales",
        "select": select_revenue(),
        "windows": [{
            "function": {"type": "row_number"},
            "alias": "rank",
            "partitionBy": [],
            "orderBy": [{"field": "revenue", "direction": "DESC"}]
        }],
        "limit": 10,
        "offset": 5
    });

    let sql = plan_and_generate_pg(&query);

    assert_sql_contains(
        &sql,
        &[
            "ROW_NUMBER()",
            "OVER",
            "ORDER BY revenue DESC",
            "LIMIT 10",
            "OFFSET 5",
        ],
    );
}

#[test]
fn test_window_with_final_order_by() {
    let query = json!({
        "table": "tf_sales",
        "select": select_occurred_at_and_revenue(),
        "windows": [{
            "function": {"type": "row_number"},
            "alias": "rank",
            "partitionBy": [{"type": "dimension", "path": "category"}],
            "orderBy": [{"field": "revenue", "direction": "DESC"}]
        }],
        "orderBy": [
            {"field": "category", "direction": "ASC"},
            {"field": "occurred_at", "direction": "DESC"}
        ]
    });

    let sql = plan_and_generate_pg(&query);

    // The window's own ORDER BY stays inside OVER (...); the final ORDER BY is a
    // separate trailing clause over the post-window result.
    assert!(
        sql.contains("ROW_NUMBER() OVER (PARTITION BY data->>'category' ORDER BY revenue DESC)")
    );
    assert!(sql.ends_with("ORDER BY data->>'category' ASC, occurred_at DESC"), "got: {sql}");
}

// =============================================================================
// Semantic-name resolution — what the request shape buys over raw SQL strings
// =============================================================================

#[test]
fn measure_not_in_metadata_is_rejected() {
    let query = json!({
        "table": "tf_sales",
        "select": [{"type": "measure", "name": "profit", "alias": "profit"}],
        "windows": [{
            "function": {"type": "running_sum", "measure": "profit"},
            "alias": "running_profit",
            "orderBy": [{"field": "occurred_at", "direction": "ASC"}]
        }]
    });

    let metadata = create_sales_metadata();
    let request = WindowQueryParser::parse(&query, &metadata).unwrap();
    let err = WindowPlanner::plan(request, &metadata).unwrap_err().to_string();

    assert!(err.contains("profit"), "error should name the unknown measure: {err}");
}

#[test]
fn undeclared_dimension_path_is_rejected() {
    // `create_sales_metadata` enumerates `category` and `region`, so the allowlist is
    // live and an undeclared path must not reach `data->>'…'`.
    let query = json!({
        "table": "tf_sales",
        "select": [{"type": "dimension", "path": "secret", "alias": "secret"}],
        "windows": [{
            "function": {"type": "row_number"},
            "alias": "rank",
            "orderBy": [{"field": "revenue", "direction": "DESC"}]
        }]
    });

    let metadata = create_sales_metadata();
    let request = WindowQueryParser::parse(&query, &metadata).unwrap();

    assert!(
        WindowPlanner::plan(request, &metadata).is_err(),
        "an undeclared dimension path must be refused"
    );
}
