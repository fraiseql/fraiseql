//! End-to-end aggregate query tests
//!
//! These tests exercise the full analytics pipeline from query parsing to SQL generation

mod common;

use common::{create_sales_metadata, assert_sql_contains};
use fraiseql_core::runtime::AggregateQueryParser;
use fraiseql_core::runtime::AggregationSqlGenerator;
use fraiseql_core::db::types::DatabaseType;
use serde_json::json;

// =============================================================================
// Helper Functions
// =============================================================================

fn parse_plan_generate(query: &serde_json::Value) -> String {
    let metadata = create_sales_metadata();
    let generator = AggregationSqlGenerator::new(DatabaseType::PostgreSQL);

    // Add table field if not present
    let mut query_with_table = query.clone();
    if query_with_table.as_object_mut().unwrap().get("table").is_none() {
        query_with_table.as_object_mut().unwrap().insert("table".to_string(), json!("tf_sales"));
    }

    let parsed = AggregateQueryParser::parse(&query_with_table, &metadata).unwrap();
    let plan = fraiseql_core::compiler::aggregation::AggregationPlanner::plan(parsed, metadata).unwrap();
    let sql_parts = generator.generate(&plan).unwrap();

    // Build complete SQL from parts
    let mut sql = format!("{} {}", sql_parts.select, sql_parts.from);

    if let Some(where_clause) = sql_parts.where_clause {
        sql.push(' ');
        sql.push_str(&where_clause);
    }

    if let Some(group_by) = sql_parts.group_by {
        sql.push(' ');
        sql.push_str(&group_by);
    }

    if let Some(having) = sql_parts.having {
        sql.push(' ');
        sql.push_str(&having);
    }

    if let Some(order_by) = sql_parts.order_by {
        sql.push(' ');
        sql.push_str(&order_by);
    }

    if let Some(limit) = sql_parts.limit {
        sql.push_str(&format!(" LIMIT {}", limit));
    }

    if let Some(offset) = sql_parts.offset {
        sql.push_str(&format!(" OFFSET {}", offset));
    }

    sql
}

// =============================================================================
// Basic Aggregate Tests
// =============================================================================

#[test]
fn test_simple_count_all() {
    let query = json!({
        "aggregates": [{"count": {}}]
    });

    let sql = parse_plan_generate(&query);

    assert_sql_contains(&sql, &[
        "SELECT",
        "COUNT(*)",
        "FROM tf_sales"
    ]);
}

#[test]
fn test_count_with_sum() {
    let query = json!({
        "aggregates": [
            {"count": {}},
            {"revenue_sum": {}}
        ]
    });

    let sql = parse_plan_generate(&query);

    assert_sql_contains(&sql, &[
        "COUNT(*)",
        "SUM(revenue)",
        "FROM tf_sales"
    ]);
}

#[test]
fn test_all_aggregate_functions() {
    let query = json!({
        "aggregates": [
            {"count": {}},
            {"revenue_sum": {}},
            {"revenue_avg": {}},
            {"revenue_min": {}},
            {"revenue_max": {}},
            {"quantity_sum": {}},
            {"quantity_avg": {}}
        ]
    });

    let sql = parse_plan_generate(&query);

    assert_sql_contains(&sql, &[
        "COUNT(*)",
        "SUM(revenue)",
        "AVG(revenue)",
        "MIN(revenue)",
        "MAX(revenue)",
        "SUM(quantity)",
        "AVG(quantity)"
    ]);
}

// =============================================================================
// GROUP BY Tests
// =============================================================================

#[test]
fn test_group_by_single_dimension() {
    let query = json!({
        "groupBy": {"category": true},
        "aggregates": [
            {"count": {}},
            {"revenue_sum": {}}
        ]
    });

    let sql = parse_plan_generate(&query);

    assert_sql_contains(&sql, &[
        "data->>'category'",
        "GROUP BY",
        "COUNT(*)",
        "SUM(revenue)"
    ]);
}

#[test]
fn test_group_by_multiple_dimensions() {
    let query = json!({
        "groupBy": {
            "category": true,
            "region": true
        },
        "aggregates": [
            {"count": {}},
            {"revenue_sum": {}}
        ]
    });

    let sql = parse_plan_generate(&query);

    assert_sql_contains(&sql, &[
        "data->>'category'",
        "data->>'region'",
        "GROUP BY"
    ]);
}

// =============================================================================
// WHERE Clause Tests
// =============================================================================

#[test]
fn test_where_denormalized_filter() {
    let query = json!({
        "where": {
            "customer_id_eq": "cust-001"
        },
        "aggregates": [{"count": {}}]
    });

    let sql = parse_plan_generate(&query);

    assert_sql_contains(&sql, &[
        "WHERE",
        "customer_id",
        "cust-001"
    ]);
}

#[test]
fn test_where_jsonb_dimension() {
    let query = json!({
        "where": {
            "category_eq": "Electronics"
        },
        "aggregates": [{"count": {}}]
    });

    let sql = parse_plan_generate(&query);

    assert_sql_contains(&sql, &[
        "WHERE",
        "data->>'category'",
        "Electronics"
    ]);
}

#[test]
fn test_where_with_comparison_operators() {
    let query = json!({
        "where": {
            "revenue_gt": 100.0,
            "quantity_lte": 10
        },
        "aggregates": [{"count": {}}]
    });

    let sql = parse_plan_generate(&query);

    assert_sql_contains(&sql, &[
        "WHERE",
        "revenue",
        ">",
        "quantity",
        "<="
    ]);
}

#[test]
fn test_where_with_like_operator() {
    let query = json!({
        "where": {
            "category_contains": "electr"
        },
        "aggregates": [{"count": {}}]
    });

    let sql = parse_plan_generate(&query);

    assert_sql_contains(&sql, &[
        "WHERE",
        "data->>'category'",
        "LIKE",
        "%electr%"
    ]);
}

// =============================================================================
// ORDER BY and LIMIT Tests
// =============================================================================

#[test]
fn test_order_by_aggregate_desc() {
    let query = json!({
        "groupBy": {"category": true},
        "aggregates": [
            {"count": {}},
            {"revenue_sum": {}}
        ],
        "orderBy": {
            "revenue_sum": "DESC"
        }
    });

    let sql = parse_plan_generate(&query);

    assert_sql_contains(&sql, &[
        "ORDER BY",
        "DESC"
    ]);
}

#[test]
fn test_order_by_dimension_asc() {
    let query = json!({
        "groupBy": {"category": true},
        "aggregates": [{"count": {}}],
        "orderBy": {
            "category": "ASC"
        }
    });

    let sql = parse_plan_generate(&query);

    assert_sql_contains(&sql, &[
        "ORDER BY",
        "ASC"
    ]);
}

#[test]
fn test_limit_only() {
    let query = json!({
        "groupBy": {"category": true},
        "aggregates": [{"count": {}}],
        "limit": 10
    });

    let sql = parse_plan_generate(&query);

    assert_sql_contains(&sql, &["LIMIT 10"]);
}

#[test]
fn test_limit_and_offset() {
    let query = json!({
        "groupBy": {"category": true},
        "aggregates": [{"count": {}}],
        "limit": 5,
        "offset": 10
    });

    let sql = parse_plan_generate(&query);

    assert_sql_contains(&sql, &[
        "LIMIT 5",
        "OFFSET 10"
    ]);
}

// =============================================================================
// Complex Query Tests
// =============================================================================

#[test]
fn test_complex_query_all_clauses() {
    let query = json!({
        "where": {
            "region_eq": "North",
            "revenue_gt": 100.0
        },
        "groupBy": {
            "category": true
        },
        "aggregates": [
            {"count": {}},
            {"revenue_sum": {}},
            {"revenue_avg": {}}
        ],
        "orderBy": {
            "revenue_sum": "DESC"
        },
        "limit": 10
    });

    let sql = parse_plan_generate(&query);

    // Verify all clauses present
    assert_sql_contains(&sql, &[
        "SELECT",
        "WHERE",
        "GROUP BY",
        "ORDER BY",
        "LIMIT"
    ]);

    // Verify specific content
    assert_sql_contains(&sql, &[
        "data->>'region'",
        "data->>'category'",
        "COUNT(*)",
        "SUM(revenue)",
        "AVG(revenue)",
        "DESC"
    ]);
}

#[test]
fn test_multiple_where_conditions() {
    let query = json!({
        "where": {
            "customer_id_eq": "cust-001",
            "category_eq": "Electronics",
            "revenue_gte": 50.0
        },
        "aggregates": [{"count": {}}]
    });

    let sql = parse_plan_generate(&query);

    assert_sql_contains(&sql, &[
        "WHERE",
        "customer_id",
        "data->>'category'",
        "revenue",
        "AND" // Multiple conditions should be AND-ed
    ]);
}

#[test]
fn test_group_by_with_multiple_aggregates() {
    let query = json!({
        "groupBy": {
            "category": true,
            "region": true
        },
        "aggregates": [
            {"count": {}},
            {"revenue_sum": {}},
            {"revenue_avg": {}},
            {"revenue_min": {}},
            {"revenue_max": {}},
            {"quantity_sum": {}}
        ]
    });

    let sql = parse_plan_generate(&query);

    // Verify GROUP BY with both dimensions
    assert_sql_contains(&sql, &[
        "data->>'category'",
        "data->>'region'",
        "GROUP BY"
    ]);

    // Verify all aggregates
    assert_sql_contains(&sql, &[
        "COUNT(*)",
        "SUM(revenue)",
        "AVG(revenue)",
        "MIN(revenue)",
        "MAX(revenue)",
        "SUM(quantity)"
    ]);
}

// =============================================================================
// SQL Correctness Tests
// =============================================================================

#[test]
fn test_sql_structure_validity() {
    let query = json!({
        "groupBy": {"category": true},
        "aggregates": [
            {"count": {}},
            {"revenue_sum": {}}
        ]
    });

    let sql = parse_plan_generate(&query);

    // Verify SQL structure follows: SELECT ... FROM ... GROUP BY ...
    let select_pos = sql.find("SELECT").expect("Missing SELECT");
    let from_pos = sql.find("FROM").expect("Missing FROM");
    let group_by_pos = sql.find("GROUP BY").expect("Missing GROUP BY");

    assert!(select_pos < from_pos, "SELECT should come before FROM");
    assert!(from_pos < group_by_pos, "FROM should come before GROUP BY");
}

#[test]
fn test_sql_no_unnecessary_clauses() {
    let query = json!({
        "aggregates": [{"count": {}}]
    });

    let sql = parse_plan_generate(&query);

    // Simple count should not have GROUP BY, WHERE, HAVING, ORDER BY
    assert!(!sql.contains("GROUP BY"));
    assert!(!sql.contains("WHERE"));
    assert!(!sql.contains("HAVING"));
    assert!(!sql.contains("ORDER BY"));
    assert!(!sql.contains("LIMIT"));
}
