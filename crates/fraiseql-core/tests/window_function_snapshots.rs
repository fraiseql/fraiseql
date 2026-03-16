#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

//! Snapshot tests for window function SQL generation.
//!
//! These tests verify that changes to the window function planner do not silently
//! alter the generated SQL. Snapshots are stored in `snapshots/window_function_snapshots__*.snap`.
//!
//! To generate or update snapshots:
//! ```bash
//! INSTA_UPDATE=always cargo test --test window_function_snapshots -p fraiseql-core
//! ```

mod common;

use common::create_sales_metadata;
use fraiseql_core::{
    compiler::window_functions::WindowFunctionPlanner, db::types::DatabaseType,
    runtime::WindowSqlGenerator,
};
use insta::assert_snapshot;
use serde_json::json;

// =============================================================================
// Helper
// =============================================================================

fn plan_and_generate(query: &serde_json::Value, db_type: DatabaseType) -> String {
    let metadata = create_sales_metadata();
    let generator = WindowSqlGenerator::new(db_type);
    let plan = WindowFunctionPlanner::plan(query, &metadata).unwrap();
    generator.generate(&plan).unwrap().raw_sql
}

// =============================================================================
// ROW_NUMBER — all 4 dialects
// =============================================================================

mod row_number {
    use super::*;

    fn query() -> serde_json::Value {
        json!({
            "table": "tf_sales",
            "select": ["revenue", "data->>'category' as category"],
            "windows": [{
                "function": {"type": "row_number"},
                "alias": "rank",
                "partitionBy": ["data->>'category'"],
                "orderBy": [{"field": "revenue", "direction": "DESC"}]
            }]
        })
    }

    #[test]
    fn postgres() {
        assert_snapshot!(plan_and_generate(&query(), DatabaseType::PostgreSQL));
    }

    #[test]
    fn mysql() {
        assert_snapshot!(plan_and_generate(&query(), DatabaseType::MySQL));
    }

    #[test]
    fn sqlite() {
        assert_snapshot!(plan_and_generate(&query(), DatabaseType::SQLite));
    }

    #[test]
    fn sqlserver() {
        assert_snapshot!(plan_and_generate(&query(), DatabaseType::SQLServer));
    }
}

// =============================================================================
// RANK — no PARTITION BY, all 4 dialects
// =============================================================================

mod rank {
    use super::*;

    fn query() -> serde_json::Value {
        json!({
            "table": "tf_sales",
            "select": ["revenue"],
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

    #[test]
    fn mysql() {
        assert_snapshot!(plan_and_generate(&query(), DatabaseType::MySQL));
    }

    #[test]
    fn sqlite() {
        assert_snapshot!(plan_and_generate(&query(), DatabaseType::SQLite));
    }

    #[test]
    fn sqlserver() {
        assert_snapshot!(plan_and_generate(&query(), DatabaseType::SQLServer));
    }
}

// =============================================================================
// DENSE_RANK — partitioned, all 4 dialects
// =============================================================================

mod dense_rank {
    use super::*;

    fn query() -> serde_json::Value {
        json!({
            "table": "tf_sales",
            "select": ["revenue"],
            "windows": [{
                "function": {"type": "dense_rank"},
                "alias": "dense_rank",
                "partitionBy": ["data->>'category'"],
                "orderBy": [{"field": "revenue", "direction": "DESC"}]
            }]
        })
    }

    #[test]
    fn postgres() {
        assert_snapshot!(plan_and_generate(&query(), DatabaseType::PostgreSQL));
    }

    #[test]
    fn mysql() {
        assert_snapshot!(plan_and_generate(&query(), DatabaseType::MySQL));
    }

    #[test]
    fn sqlite() {
        assert_snapshot!(plan_and_generate(&query(), DatabaseType::SQLite));
    }

    #[test]
    fn sqlserver() {
        assert_snapshot!(plan_and_generate(&query(), DatabaseType::SQLServer));
    }
}

// =============================================================================
// LAG — value function, all 4 dialects
// =============================================================================

mod lag {
    use super::*;

    fn query() -> serde_json::Value {
        json!({
            "table": "tf_sales",
            "select": ["occurred_at", "revenue"],
            "windows": [{
                "function": {
                    "type": "lag",
                    "field": "revenue",
                    "offset": 1,
                    "default": 0
                },
                "alias": "prev_revenue",
                "partitionBy": ["data->>'category'"],
                "orderBy": [{"field": "occurred_at", "direction": "ASC"}]
            }]
        })
    }

    #[test]
    fn postgres() {
        assert_snapshot!(plan_and_generate(&query(), DatabaseType::PostgreSQL));
    }

    #[test]
    fn mysql() {
        assert_snapshot!(plan_and_generate(&query(), DatabaseType::MySQL));
    }

    #[test]
    fn sqlite() {
        assert_snapshot!(plan_and_generate(&query(), DatabaseType::SQLite));
    }

    #[test]
    fn sqlserver() {
        assert_snapshot!(plan_and_generate(&query(), DatabaseType::SQLServer));
    }
}

// =============================================================================
// LEAD — value function, all 4 dialects
// =============================================================================

mod lead {
    use super::*;

    fn query() -> serde_json::Value {
        json!({
            "table": "tf_sales",
            "select": ["occurred_at", "revenue"],
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

    #[test]
    fn mysql() {
        assert_snapshot!(plan_and_generate(&query(), DatabaseType::MySQL));
    }

    #[test]
    fn sqlite() {
        assert_snapshot!(plan_and_generate(&query(), DatabaseType::SQLite));
    }

    #[test]
    fn sqlserver() {
        assert_snapshot!(plan_and_generate(&query(), DatabaseType::SQLServer));
    }
}

// =============================================================================
// Cumulative SUM with frame clause — all 4 dialects
// =============================================================================

mod cumulative_sum {
    use super::*;

    fn query() -> serde_json::Value {
        json!({
            "table": "tf_sales",
            "select": ["occurred_at", "revenue"],
            "windows": [{
                "function": {"type": "sum", "field": "revenue"},
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

    #[test]
    fn mysql() {
        assert_snapshot!(plan_and_generate(&query(), DatabaseType::MySQL));
    }

    #[test]
    fn sqlite() {
        assert_snapshot!(plan_and_generate(&query(), DatabaseType::SQLite));
    }

    #[test]
    fn sqlserver() {
        assert_snapshot!(plan_and_generate(&query(), DatabaseType::SQLServer));
    }
}

// =============================================================================
// Moving average with N PRECEDING frame — all 4 dialects
// =============================================================================

mod moving_average {
    use super::*;

    fn query() -> serde_json::Value {
        json!({
            "table": "tf_sales",
            "select": ["occurred_at", "revenue"],
            "windows": [{
                "function": {"type": "avg", "field": "revenue"},
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

    #[test]
    fn mysql() {
        assert_snapshot!(plan_and_generate(&query(), DatabaseType::MySQL));
    }

    #[test]
    fn sqlite() {
        assert_snapshot!(plan_and_generate(&query(), DatabaseType::SQLite));
    }

    #[test]
    fn sqlserver() {
        assert_snapshot!(plan_and_generate(&query(), DatabaseType::SQLServer));
    }
}

// =============================================================================
// Multiple window functions in one query — PostgreSQL
// =============================================================================

mod multiple_windows {
    use super::*;

    #[test]
    fn postgres() {
        let query = json!({
            "table": "tf_sales",
            "select": ["occurred_at", "revenue", "data->>'category' as category"],
            "windows": [
                {
                    "function": {"type": "row_number"},
                    "alias": "row_num",
                    "partitionBy": ["data->>'category'"],
                    "orderBy": [{"field": "revenue", "direction": "DESC"}]
                },
                {
                    "function": {"type": "sum", "field": "revenue"},
                    "alias": "running_total",
                    "partitionBy": ["data->>'category'"],
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
                    "partitionBy": ["data->>'category'"],
                    "orderBy": [{"field": "occurred_at", "direction": "ASC"}]
                }
            ]
        });
        assert_snapshot!(plan_and_generate(&query, DatabaseType::PostgreSQL));
    }
}

// =============================================================================
// LAST_VALUE with UNBOUNDED FOLLOWING frame — PostgreSQL
// =============================================================================

mod last_value {
    use super::*;

    #[test]
    fn postgres() {
        let query = json!({
            "table": "tf_sales",
            "select": ["occurred_at", "revenue"],
            "windows": [{
                "function": {"type": "last_value", "field": "revenue"},
                "alias": "last_revenue",
                "partitionBy": ["data->>'category'"],
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
// NTILE — PostgreSQL
// =============================================================================

mod ntile {
    use super::*;

    #[test]
    fn postgres() {
        let query = json!({
            "table": "tf_sales",
            "select": ["revenue"],
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
// STDDEV / VARIANCE — SQL Server naming difference
// =============================================================================

mod stddev_variance {
    use super::*;

    fn stddev_query() -> serde_json::Value {
        json!({
            "table": "tf_sales",
            "select": ["revenue"],
            "windows": [{
                "function": {"type": "stddev", "field": "revenue"},
                "alias": "stddev_revenue",
                "partitionBy": [],
                "orderBy": [{"field": "occurred_at", "direction": "ASC"}]
            }]
        })
    }

    fn variance_query() -> serde_json::Value {
        json!({
            "table": "tf_sales",
            "select": ["revenue"],
            "windows": [{
                "function": {"type": "variance", "field": "revenue"},
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
    fn stddev_sqlserver() {
        let sql = plan_and_generate(&stddev_query(), DatabaseType::SQLServer);
        assert_snapshot!(sql);
    }

    #[test]
    fn variance_postgres() {
        let sql = plan_and_generate(&variance_query(), DatabaseType::PostgreSQL);
        assert_snapshot!(sql);
    }

    #[test]
    fn variance_sqlserver() {
        let sql = plan_and_generate(&variance_query(), DatabaseType::SQLServer);
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
            "select": ["occurred_at", "revenue"],
            "windows": [{
                "function": {"type": "avg", "field": "revenue"},
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
            "select": ["revenue"],
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
