#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

//! Snapshot tests for window function SQL generation.
//!
//! These pin the exact SQL the live chain emits —
//! `WindowQueryParser::parse` -> `WindowPlanner::plan` -> `WindowSqlGenerator::generate`
//! — so a change to any of the three shows up as a snapshot diff rather than silently
//! altering what reaches PostgreSQL. Snapshots are stored in
//! `snapshots/window_function_snapshots__*.snap` and registered in
//! `tests/snapshot-pairs.md`.
//!
//! Ported from `WindowFunctionPlanner` in #881; see the header of
//! `e2e_window_functions.rs` for why that planner is gone.
//!
//! To generate or update snapshots:
//! ```bash
//! INSTA_UPDATE=always cargo test --test window_function_snapshots -p fraiseql-core
//! ```

mod common;

use common::create_sales_metadata;
use fraiseql_core::{
    compiler::window_functions::WindowPlanner,
    db::types::DatabaseType,
    runtime::{WindowQueryParser, WindowSqlGenerator},
};
use insta::assert_snapshot;
use serde_json::json;

// =============================================================================
// Helper
// =============================================================================

fn plan_and_generate(query: &serde_json::Value, db_type: DatabaseType) -> String {
    let metadata = create_sales_metadata();
    let request = WindowQueryParser::parse(query, &metadata).unwrap();
    let plan = WindowPlanner::plan(request, &metadata).unwrap();

    WindowSqlGenerator::new(db_type).generate(&plan).unwrap().raw_sql
}

// =============================================================================
// ROW_NUMBER — partitioned by a JSONB dimension
// =============================================================================

mod row_number {
    use super::*;

    fn query() -> serde_json::Value {
        json!({
            "table": "tf_sales",
            "select": [
                {"type": "measure", "name": "revenue", "alias": "revenue"},
                {"type": "dimension", "path": "category", "alias": "category"}
            ],
            "windows": [{
                "function": {"type": "row_number"},
                "alias": "rank",
                "partitionBy": [{"type": "dimension", "path": "category"}],
                "orderBy": [{"field": "revenue", "direction": "DESC"}]
            }]
        })
    }

    #[test]
    fn postgres() {
        assert_snapshot!(plan_and_generate(&query(), DatabaseType::PostgreSQL));
    }
}

// =============================================================================
// RANK — no PARTITION BY
// =============================================================================

mod rank {
    use super::*;

    fn query() -> serde_json::Value {
        json!({
            "table": "tf_sales",
            "select": [{"type": "measure", "name": "revenue", "alias": "revenue"}],
            "windows": [{
                "function": {"type": "rank"},
                "alias": "revenue_rank",
                "partitionBy": [],
                "orderBy": [{"field": "revenue", "direction": "DESC"}]
            }]
        })
    }

    #[test]
    fn postgres() {
        assert_snapshot!(plan_and_generate(&query(), DatabaseType::PostgreSQL));
    }
}

// =============================================================================
// DENSE_RANK — partitioned
// =============================================================================

mod dense_rank {
    use super::*;

    fn query() -> serde_json::Value {
        json!({
            "table": "tf_sales",
            "select": [{"type": "measure", "name": "revenue", "alias": "revenue"}],
            "windows": [{
                "function": {"type": "dense_rank"},
                "alias": "dense_rank",
                "partitionBy": [{"type": "dimension", "path": "category"}],
                "orderBy": [{"field": "revenue", "direction": "DESC"}]
            }]
        })
    }

    #[test]
    fn postgres() {
        assert_snapshot!(plan_and_generate(&query(), DatabaseType::PostgreSQL));
    }
}

// =============================================================================
// LAG — value function
// =============================================================================

mod lag {
    use super::*;

    fn query() -> serde_json::Value {
        json!({
            "table": "tf_sales",
            "select": [
                {"type": "filter", "name": "occurred_at", "alias": "occurred_at"},
                {"type": "measure", "name": "revenue", "alias": "revenue"}
            ],
            "windows": [{
                "function": {
                    "type": "lag",
                    "field": "revenue",
                    "offset": 1,
                    "default": 0
                },
                "alias": "prev_revenue",
                "partitionBy": [{"type": "dimension", "path": "category"}],
                "orderBy": [{"field": "occurred_at", "direction": "ASC"}]
            }]
        })
    }

    #[test]
    fn postgres() {
        assert_snapshot!(plan_and_generate(&query(), DatabaseType::PostgreSQL));
    }
}

// =============================================================================
// LEAD — value function
// =============================================================================

mod lead {
    use super::*;

    fn query() -> serde_json::Value {
        json!({
            "table": "tf_sales",
            "select": [
                {"type": "filter", "name": "occurred_at", "alias": "occurred_at"},
                {"type": "measure", "name": "revenue", "alias": "revenue"}
            ],
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
        })
    }

    #[test]
    fn postgres() {
        assert_snapshot!(plan_and_generate(&query(), DatabaseType::PostgreSQL));
    }
}

// =============================================================================
// Cumulative SUM with frame clause
// =============================================================================

mod cumulative_sum {
    use super::*;

    fn query() -> serde_json::Value {
        json!({
            "table": "tf_sales",
            "select": [
                {"type": "filter", "name": "occurred_at", "alias": "occurred_at"},
                {"type": "measure", "name": "revenue", "alias": "revenue"}
            ],
            "windows": [{
                "function": {"type": "running_sum", "measure": "revenue"},
                "alias": "running_total",
                "partitionBy": [],
                "orderBy": [{"field": "occurred_at", "direction": "ASC"}],
                "frame": {
                    "frame_type": "ROWS",
                    "start": {"type": "unbounded_preceding"},
                    "end": {"type": "current_row"}
                }
            }]
        })
    }

    #[test]
    fn postgres() {
        assert_snapshot!(plan_and_generate(&query(), DatabaseType::PostgreSQL));
    }
}

// =============================================================================
// Moving average with N PRECEDING frame
// =============================================================================

mod moving_average {
    use super::*;

    fn query() -> serde_json::Value {
        json!({
            "table": "tf_sales",
            "select": [
                {"type": "filter", "name": "occurred_at", "alias": "occurred_at"},
                {"type": "measure", "name": "revenue", "alias": "revenue"}
            ],
            "windows": [{
                "function": {"type": "running_avg", "measure": "revenue"},
                "alias": "moving_avg_3",
                "partitionBy": [],
                "orderBy": [{"field": "occurred_at", "direction": "ASC"}],
                "frame": {
                    "frame_type": "ROWS",
                    "start": {"type": "n_preceding", "n": 2},
                    "end": {"type": "current_row"}
                }
            }]
        })
    }

    #[test]
    fn postgres() {
        assert_snapshot!(plan_and_generate(&query(), DatabaseType::PostgreSQL));
    }
}

// =============================================================================
// Multiple window functions in one query
// =============================================================================

mod multiple_windows {
    use super::*;

    #[test]
    fn postgres() {
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
        assert_snapshot!(plan_and_generate(&query, DatabaseType::PostgreSQL));
    }
}

// =============================================================================
// LAST_VALUE with UNBOUNDED FOLLOWING frame
// =============================================================================

mod last_value {
    use super::*;

    #[test]
    fn postgres() {
        let query = json!({
            "table": "tf_sales",
            "select": [
                {"type": "filter", "name": "occurred_at", "alias": "occurred_at"},
                {"type": "measure", "name": "revenue", "alias": "revenue"}
            ],
            "windows": [{
                "function": {"type": "last_value", "field": "revenue"},
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
        assert_snapshot!(plan_and_generate(&query, DatabaseType::PostgreSQL));
    }
}

// =============================================================================
// NTILE
// =============================================================================

mod ntile {
    use super::*;

    #[test]
    fn postgres() {
        let query = json!({
            "table": "tf_sales",
            "select": [{"type": "measure", "name": "revenue", "alias": "revenue"}],
            "windows": [{
                "function": {"type": "ntile", "n": 4},
                "alias": "quartile",
                "partitionBy": [],
                "orderBy": [{"field": "revenue", "direction": "ASC"}]
            }]
        });
        assert_snapshot!(plan_and_generate(&query, DatabaseType::PostgreSQL));
    }
}

// =============================================================================
// STDDEV / VARIANCE
// =============================================================================

mod stddev_variance {
    use super::*;

    fn stddev_query() -> serde_json::Value {
        json!({
            "table": "tf_sales",
            "select": [{"type": "measure", "name": "revenue", "alias": "revenue"}],
            "windows": [{
                "function": {"type": "running_stddev", "measure": "revenue"},
                "alias": "stddev_revenue",
                "partitionBy": [],
                "orderBy": [{"field": "occurred_at", "direction": "ASC"}]
            }]
        })
    }

    fn variance_query() -> serde_json::Value {
        json!({
            "table": "tf_sales",
            "select": [{"type": "measure", "name": "revenue", "alias": "revenue"}],
            "windows": [{
                "function": {"type": "running_variance", "measure": "revenue"},
                "alias": "var_revenue",
                "partitionBy": [],
                "orderBy": [{"field": "occurred_at", "direction": "ASC"}]
            }]
        })
    }

    #[test]
    fn stddev_postgres() {
        let sql = plan_and_generate(&stddev_query(), DatabaseType::PostgreSQL);
        assert_snapshot!(sql);
    }

    #[test]
    fn variance_postgres() {
        let sql = plan_and_generate(&variance_query(), DatabaseType::PostgreSQL);
        assert_snapshot!(sql);
    }
}

// =============================================================================
// Frame exclusion — PostgreSQL-specific
// =============================================================================

mod frame_exclusion {
    use super::*;

    #[test]
    fn exclude_current_row_postgres() {
        let query = json!({
            "table": "tf_sales",
            "select": [
                {"type": "filter", "name": "occurred_at", "alias": "occurred_at"},
                {"type": "measure", "name": "revenue", "alias": "revenue"}
            ],
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
        assert_snapshot!(plan_and_generate(&query, DatabaseType::PostgreSQL));
    }
}

// =============================================================================
// Window function with LIMIT/OFFSET
// =============================================================================

mod with_limit_offset {
    use super::*;

    #[test]
    fn postgres() {
        let query = json!({
            "table": "tf_sales",
            "select": [{"type": "measure", "name": "revenue", "alias": "revenue"}],
            "windows": [{
                "function": {"type": "row_number"},
                "alias": "rank",
                "partitionBy": [],
                "orderBy": [{"field": "revenue", "direction": "DESC"}]
            }],
            "limit": 10,
            "offset": 5
        });
        assert_snapshot!(plan_and_generate(&query, DatabaseType::PostgreSQL));
    }
}

// =============================================================================
// WHERE clause — the parameterised half of the emitted SQL
//
// The dead planner turned any `where` key into an empty `WhereClause::And(vec![])`,
// so no snapshot ever showed a real predicate or a bind parameter. The live parser
// builds one, and the generator renders placeholders.
// =============================================================================

mod filtered {
    use super::*;

    #[test]
    fn postgres() {
        let query = json!({
            "table": "tf_sales",
            "select": [
                {"type": "filter", "name": "occurred_at", "alias": "occurred_at"},
                {"type": "measure", "name": "revenue", "alias": "revenue"}
            ],
            "windows": [{
                "function": {"type": "running_sum", "measure": "revenue"},
                "alias": "running_total",
                "partitionBy": [],
                "orderBy": [{"field": "occurred_at", "direction": "ASC"}]
            }],
            "where": {"category_eq": "hardware"}
        });
        assert_snapshot!(plan_and_generate(&query, DatabaseType::PostgreSQL));
    }
}
