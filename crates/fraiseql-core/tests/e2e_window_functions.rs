//! End-to-end window function tests
//!
//! These tests exercise the full window function pipeline from planning to SQL generation

mod common;

use common::{assert_sql_contains, create_sales_metadata};
use fraiseql_core::compiler::window_functions::WindowFunctionPlanner;
use fraiseql_core::db::types::DatabaseType;
use fraiseql_core::runtime::WindowSqlGenerator;
use serde_json::json;

// =============================================================================
// Helper Functions
// =============================================================================

fn plan_and_generate(query: &serde_json::Value, db_type: DatabaseType) -> String {
    let metadata = create_sales_metadata();
    let generator = WindowSqlGenerator::new(db_type);

    let plan = WindowFunctionPlanner::plan(query, &metadata).unwrap();
    let sql_result = generator.generate(&plan).unwrap();

    sql_result.complete_sql
}

fn plan_and_generate_pg(query: &serde_json::Value) -> String {
    plan_and_generate(query, DatabaseType::PostgreSQL)
}

// =============================================================================
// Ranking Function Tests
// =============================================================================

#[test]
fn test_row_number_simple() {
    let query = json!({
        "table": "tf_sales",
        "select": ["revenue", "data->>'category' as category"],
        "windows": [{
            "function": {"row_number": {}},
            "alias": "rank",
            "partitionBy": ["data->>'category'"],
            "orderBy": [{"field": "revenue", "direction": "DESC"}]
        }]
    });

    let sql = plan_and_generate_pg(&query);

    assert_sql_contains(&sql, &[
        "SELECT",
        "revenue",
        "ROW_NUMBER()",
        "OVER",
        "PARTITION BY data->>'category'",
        "ORDER BY revenue DESC",
        "AS rank",
        "FROM tf_sales",
    ]);
}

#[test]
fn test_rank_with_gaps() {
    let query = json!({
        "table": "tf_sales",
        "select": ["revenue"],
        "windows": [{
            "function": {"rank": {}},
            "alias": "revenue_rank",
            "partitionBy": [],
            "orderBy": [{"field": "revenue", "direction": "DESC"}]
        }]
    });

    let sql = plan_and_generate_pg(&query);

    assert_sql_contains(&sql, &[
        "RANK()",
        "OVER",
        "ORDER BY revenue DESC",
        "AS revenue_rank",
    ]);
}

#[test]
fn test_dense_rank_no_gaps() {
    let query = json!({
        "table": "tf_sales",
        "select": ["revenue"],
        "windows": [{
            "function": {"dense_rank": {}},
            "alias": "dense_rank",
            "partitionBy": [],
            "orderBy": [{"field": "revenue", "direction": "DESC"}]
        }]
    });

    let sql = plan_and_generate_pg(&query);

    assert_sql_contains(&sql, &[
        "DENSE_RANK()",
        "OVER",
        "ORDER BY revenue DESC",
    ]);
}

#[test]
fn test_ntile_quartiles() {
    let query = json!({
        "table": "tf_sales",
        "select": ["revenue"],
        "windows": [{
            "function": {"ntile": {"n": 4}},
            "alias": "quartile",
            "partitionBy": [],
            "orderBy": [{"field": "revenue", "direction": "ASC"}]
        }]
    });

    let sql = plan_and_generate_pg(&query);

    assert_sql_contains(&sql, &[
        "NTILE(4)",
        "OVER",
        "ORDER BY revenue ASC",
        "AS quartile",
    ]);
}

#[test]
fn test_percent_rank() {
    let query = json!({
        "table": "tf_sales",
        "select": ["revenue"],
        "windows": [{
            "function": {"percent_rank": {}},
            "alias": "pct_rank",
            "partitionBy": ["data->>'category'"],
            "orderBy": [{"field": "revenue", "direction": "DESC"}]
        }]
    });

    let sql = plan_and_generate_pg(&query);

    assert_sql_contains(&sql, &[
        "PERCENT_RANK()",
        "OVER",
        "PARTITION BY data->>'category'",
        "ORDER BY revenue DESC",
    ]);
}

#[test]
fn test_cume_dist() {
    let query = json!({
        "table": "tf_sales",
        "select": ["revenue"],
        "windows": [{
            "function": {"cume_dist": {}},
            "alias": "cumulative_distribution",
            "partitionBy": [],
            "orderBy": [{"field": "revenue", "direction": "ASC"}]
        }]
    });

    let sql = plan_and_generate_pg(&query);

    assert_sql_contains(&sql, &[
        "CUME_DIST()",
        "OVER",
        "ORDER BY revenue ASC",
    ]);
}

// =============================================================================
// Value Function Tests (LAG/LEAD/FIRST_VALUE/LAST_VALUE)
// =============================================================================

#[test]
fn test_lag_previous_value() {
    let query = json!({
        "table": "tf_sales",
        "select": ["occurred_at", "revenue"],
        "windows": [{
            "function": {
                "lag": {
                    "field": "revenue",
                    "offset": 1,
                    "default": 0
                }
            },
            "alias": "prev_revenue",
            "partitionBy": [],
            "orderBy": [{"field": "occurred_at", "direction": "ASC"}]
        }]
    });

    let sql = plan_and_generate_pg(&query);

    assert_sql_contains(&sql, &[
        "LAG(revenue, 1, 0)",
        "OVER",
        "ORDER BY occurred_at ASC",
        "AS prev_revenue",
    ]);
}

#[test]
fn test_lead_next_value() {
    let query = json!({
        "table": "tf_sales",
        "select": ["occurred_at", "revenue"],
        "windows": [{
            "function": {
                "lead": {
                    "field": "revenue",
                    "offset": 1,
                    "default": 0
                }
            },
            "alias": "next_revenue",
            "partitionBy": [],
            "orderBy": [{"field": "occurred_at", "direction": "ASC"}]
        }]
    });

    let sql = plan_and_generate_pg(&query);

    assert_sql_contains(&sql, &[
        "LEAD(revenue, 1, 0)",
        "OVER",
        "ORDER BY occurred_at ASC",
        "AS next_revenue",
    ]);
}

#[test]
fn test_first_value() {
    let query = json!({
        "table": "tf_sales",
        "select": ["occurred_at", "revenue"],
        "windows": [{
            "function": {
                "first_value": {
                    "field": "revenue"
                }
            },
            "alias": "first_revenue",
            "partitionBy": ["data->>'category'"],
            "orderBy": [{"field": "occurred_at", "direction": "ASC"}]
        }]
    });

    let sql = plan_and_generate_pg(&query);

    assert_sql_contains(&sql, &[
        "FIRST_VALUE(revenue)",
        "OVER",
        "PARTITION BY data->>'category'",
        "ORDER BY occurred_at ASC",
    ]);
}

#[test]
fn test_last_value() {
    let query = json!({
        "table": "tf_sales",
        "select": ["occurred_at", "revenue"],
        "windows": [{
            "function": {
                "last_value": {
                    "field": "revenue"
                }
            },
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

    let sql = plan_and_generate_pg(&query);

    assert_sql_contains(&sql, &[
        "LAST_VALUE(revenue)",
        "OVER",
        "PARTITION BY data->>'category'",
        "ORDER BY occurred_at ASC",
        "ROWS BETWEEN UNBOUNDED PRECEDING AND UNBOUNDED FOLLOWING",
    ]);
}

#[test]
fn test_nth_value() {
    let query = json!({
        "table": "tf_sales",
        "select": ["occurred_at", "revenue"],
        "windows": [{
            "function": {
                "nth_value": {
                    "field": "revenue",
                    "n": 3
                }
            },
            "alias": "third_revenue",
            "partitionBy": ["data->>'category'"],
            "orderBy": [{"field": "occurred_at", "direction": "ASC"}]
        }]
    });

    let sql = plan_and_generate_pg(&query);

    assert_sql_contains(&sql, &[
        "NTH_VALUE(revenue, 3)",
        "OVER",
        "PARTITION BY data->>'category'",
        "ORDER BY occurred_at ASC",
    ]);
}

// =============================================================================
// Aggregate as Window Function Tests
// =============================================================================

#[test]
fn test_running_total_sum() {
    let query = json!({
        "table": "tf_sales",
        "select": ["occurred_at", "revenue"],
        "windows": [{
            "function": {
                "sum": {
                    "field": "revenue"
                }
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

    assert_sql_contains(&sql, &[
        "SUM(revenue)",
        "OVER",
        "ORDER BY occurred_at ASC",
        "ROWS BETWEEN UNBOUNDED PRECEDING AND CURRENT ROW",
        "AS running_total",
    ]);
}

#[test]
fn test_moving_average() {
    let query = json!({
        "table": "tf_sales",
        "select": ["occurred_at", "revenue"],
        "windows": [{
            "function": {
                "avg": {
                    "field": "revenue"
                }
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

    assert_sql_contains(&sql, &[
        "AVG(revenue)",
        "OVER",
        "ORDER BY occurred_at ASC",
        "ROWS BETWEEN 2 PRECEDING AND CURRENT ROW",
        "AS moving_avg_3",
    ]);
}

#[test]
fn test_running_count() {
    let query = json!({
        "table": "tf_sales",
        "select": ["occurred_at"],
        "windows": [{
            "function": {
                "count": {}
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

    assert_sql_contains(&sql, &[
        "COUNT(*)",
        "OVER",
        "ORDER BY occurred_at ASC",
        "ROWS BETWEEN UNBOUNDED PRECEDING AND CURRENT ROW",
    ]);
}

#[test]
fn test_running_min_max() {
    let query = json!({
        "table": "tf_sales",
        "select": ["occurred_at", "revenue"],
        "windows": [
            {
                "function": {"min": {"field": "revenue"}},
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
                "function": {"max": {"field": "revenue"}},
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

    assert_sql_contains(&sql, &[
        "MIN(revenue)",
        "MAX(revenue)",
        "OVER",
        "ORDER BY occurred_at ASC",
        "ROWS BETWEEN UNBOUNDED PRECEDING AND CURRENT ROW",
    ]);
}

// =============================================================================
// Frame Specification Tests
// =============================================================================

#[test]
fn test_frame_rows_preceding_following() {
    let query = json!({
        "table": "tf_sales",
        "select": ["occurred_at", "revenue"],
        "windows": [{
            "function": {"avg": {"field": "revenue"}},
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

    assert_sql_contains(&sql, &[
        "AVG(revenue)",
        "OVER",
        "ROWS BETWEEN 3 PRECEDING AND 3 FOLLOWING",
    ]);
}

#[test]
fn test_frame_range() {
    let query = json!({
        "table": "tf_sales",
        "select": ["occurred_at", "revenue"],
        "windows": [{
            "function": {"sum": {"field": "revenue"}},
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

    assert_sql_contains(&sql, &[
        "SUM(revenue)",
        "OVER",
        "RANGE BETWEEN UNBOUNDED PRECEDING AND CURRENT ROW",
    ]);
}

#[test]
fn test_frame_groups_postgres_only() {
    let query = json!({
        "table": "tf_sales",
        "select": ["occurred_at", "revenue"],
        "windows": [{
            "function": {"sum": {"field": "revenue"}},
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

    assert_sql_contains(&sql, &[
        "SUM(revenue)",
        "OVER",
        "GROUPS BETWEEN UNBOUNDED PRECEDING AND CURRENT ROW",
    ]);
}

#[test]
fn test_frame_exclusion_postgres() {
    let query = json!({
        "table": "tf_sales",
        "select": ["occurred_at", "revenue"],
        "windows": [{
            "function": {"avg": {"field": "revenue"}},
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

    assert_sql_contains(&sql, &[
        "AVG(revenue)",
        "OVER",
        "ROWS BETWEEN UNBOUNDED PRECEDING AND UNBOUNDED FOLLOWING",
        "EXCLUDE CURRENT ROW",
    ]);
}

// =============================================================================
// Multi-Database Tests
// =============================================================================

#[test]
fn test_window_function_multi_database() {
    let query = json!({
        "table": "tf_sales",
        "select": ["revenue"],
        "windows": [{
            "function": {"row_number": {}},
            "alias": "rank",
            "partitionBy": ["data->>'category'"],
            "orderBy": [{"field": "revenue", "direction": "DESC"}]
        }]
    });

    // PostgreSQL
    let pg_sql = plan_and_generate(&query, DatabaseType::PostgreSQL);
    assert!(pg_sql.contains("ROW_NUMBER()"));
    assert!(pg_sql.contains("PARTITION BY data->>'category'"));

    // MySQL
    let mysql_sql = plan_and_generate(&query, DatabaseType::MySQL);
    assert!(mysql_sql.contains("ROW_NUMBER()"));
    assert!(mysql_sql.contains("PARTITION BY data->>'category'"));

    // SQLite
    let sqlite_sql = plan_and_generate(&query, DatabaseType::SQLite);
    assert!(sqlite_sql.contains("ROW_NUMBER()"));
    assert!(sqlite_sql.contains("PARTITION BY data->>'category'"));

    // SQL Server
    let mssql_sql = plan_and_generate(&query, DatabaseType::SQLServer);
    assert!(mssql_sql.contains("ROW_NUMBER()"));
    assert!(mssql_sql.contains("PARTITION BY data->>'category'"));
}

#[test]
fn test_stddev_variance_sqlserver_naming() {
    let query_stddev = json!({
        "table": "tf_sales",
        "select": ["revenue"],
        "windows": [{
            "function": {"stddev": {"field": "revenue"}},
            "alias": "stddev_revenue",
            "partitionBy": [],
            "orderBy": [{"field": "occurred_at", "direction": "ASC"}]
        }]
    });

    let query_variance = json!({
        "table": "tf_sales",
        "select": ["revenue"],
        "windows": [{
            "function": {"variance": {"field": "revenue"}},
            "alias": "var_revenue",
            "partitionBy": [],
            "orderBy": [{"field": "occurred_at", "direction": "ASC"}]
        }]
    });

    // PostgreSQL uses STDDEV/VARIANCE
    let pg_stddev = plan_and_generate(&query_stddev, DatabaseType::PostgreSQL);
    assert!(pg_stddev.contains("STDDEV(revenue)"));

    let pg_variance = plan_and_generate(&query_variance, DatabaseType::PostgreSQL);
    assert!(pg_variance.contains("VARIANCE(revenue)"));

    // SQL Server uses STDEV/VAR
    let mssql_stddev = plan_and_generate(&query_stddev, DatabaseType::SQLServer);
    assert!(mssql_stddev.contains("STDEV(revenue)"));

    let mssql_variance = plan_and_generate(&query_variance, DatabaseType::SQLServer);
    assert!(mssql_variance.contains("VAR(revenue)"));
}

// =============================================================================
// Complex Scenarios
// =============================================================================

#[test]
fn test_multiple_window_functions() {
    let query = json!({
        "table": "tf_sales",
        "select": ["occurred_at", "revenue", "data->>'category' as category"],
        "windows": [
            {
                "function": {"row_number": {}},
                "alias": "row_num",
                "partitionBy": ["data->>'category'"],
                "orderBy": [{"field": "revenue", "direction": "DESC"}]
            },
            {
                "function": {"sum": {"field": "revenue"}},
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
                "function": {"lag": {"field": "revenue", "offset": 1, "default": 0}},
                "alias": "prev_revenue",
                "partitionBy": ["data->>'category'"],
                "orderBy": [{"field": "occurred_at", "direction": "ASC"}]
            }
        ]
    });

    let sql = plan_and_generate_pg(&query);

    assert_sql_contains(&sql, &[
        "ROW_NUMBER()",
        "SUM(revenue)",
        "LAG(revenue, 1, 0)",
        "PARTITION BY data->>'category'",
        "AS row_num",
        "AS running_total",
        "AS prev_revenue",
    ]);
}

#[test]
fn test_window_with_limit_offset() {
    let query = json!({
        "table": "tf_sales",
        "select": ["revenue"],
        "windows": [{
            "function": {"row_number": {}},
            "alias": "rank",
            "partitionBy": [],
            "orderBy": [{"field": "revenue", "direction": "DESC"}]
        }],
        "limit": 10,
        "offset": 5
    });

    let sql = plan_and_generate_pg(&query);

    assert_sql_contains(&sql, &[
        "ROW_NUMBER()",
        "OVER",
        "ORDER BY revenue DESC",
        "LIMIT 10",
        "OFFSET 5",
    ]);
}

#[test]
fn test_window_with_final_order_by() {
    let query = json!({
        "table": "tf_sales",
        "select": ["occurred_at", "revenue"],
        "windows": [{
            "function": {"row_number": {}},
            "alias": "rank",
            "partitionBy": ["data->>'category'"],
            "orderBy": [{"field": "revenue", "direction": "DESC"}]
        }],
        "orderBy": [
            {"field": "data->>'category'", "direction": "ASC"},
            {"field": "rank", "direction": "ASC"}
        ]
    });

    let sql = plan_and_generate_pg(&query);

    // Verify window ORDER BY is separate from final ORDER BY
    assert!(sql.contains("ROW_NUMBER()"));
    assert!(sql.contains("ORDER BY data->>'category' ASC"));
    assert!(sql.contains("rank ASC"));
}
