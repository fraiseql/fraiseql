//! End-to-end aggregate query tests
//!
//! These tests exercise the full analytics pipeline from query parsing to SQL generation

mod common;

use common::{assert_sql_contains, create_sales_metadata};
use fraiseql_core::{
    db::types::DatabaseType,
    runtime::{AggregateQueryParser, AggregationSqlGenerator},
};
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
        query_with_table
            .as_object_mut()
            .unwrap()
            .insert("table".to_string(), json!("tf_sales"));
    }

    let parsed = AggregateQueryParser::parse(&query_with_table, &metadata).unwrap();
    let plan =
        fraiseql_core::compiler::aggregation::AggregationPlanner::plan(parsed, metadata).unwrap();
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

    assert_sql_contains(&sql, &["SELECT", "COUNT(*)", "FROM tf_sales"]);
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

    assert_sql_contains(&sql, &["COUNT(*)", "SUM(revenue)", "FROM tf_sales"]);
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

    assert_sql_contains(
        &sql,
        &[
            "COUNT(*)",
            "SUM(revenue)",
            "AVG(revenue)",
            "MIN(revenue)",
            "MAX(revenue)",
            "SUM(quantity)",
            "AVG(quantity)",
        ],
    );
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

    assert_sql_contains(&sql, &["data->>'category'", "GROUP BY", "COUNT(*)", "SUM(revenue)"]);
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

    assert_sql_contains(&sql, &["data->>'category'", "data->>'region'", "GROUP BY"]);
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

    assert_sql_contains(&sql, &["WHERE", "customer_id", "cust-001"]);
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

    assert_sql_contains(&sql, &["WHERE", "data->>'category'", "Electronics"]);
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

    assert_sql_contains(&sql, &["WHERE", "revenue", ">", "quantity", "<="]);
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

    assert_sql_contains(&sql, &["WHERE", "data->>'category'", "LIKE", "%electr%"]);
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

    assert_sql_contains(&sql, &["ORDER BY", "DESC"]);
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

    assert_sql_contains(&sql, &["ORDER BY", "ASC"]);
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

    assert_sql_contains(&sql, &["LIMIT 5", "OFFSET 10"]);
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
    assert_sql_contains(&sql, &["SELECT", "WHERE", "GROUP BY", "ORDER BY", "LIMIT"]);

    // Verify specific content
    assert_sql_contains(
        &sql,
        &[
            "data->>'region'",
            "data->>'category'",
            "COUNT(*)",
            "SUM(revenue)",
            "AVG(revenue)",
            "DESC",
        ],
    );
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

    assert_sql_contains(
        &sql,
        &[
            "WHERE",
            "customer_id",
            "data->>'category'",
            "revenue",
            "AND", // Multiple conditions should be AND-ed
        ],
    );
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
    assert_sql_contains(&sql, &["data->>'category'", "data->>'region'", "GROUP BY"]);

    // Verify all aggregates
    assert_sql_contains(
        &sql,
        &[
            "COUNT(*)",
            "SUM(revenue)",
            "AVG(revenue)",
            "MIN(revenue)",
            "MAX(revenue)",
            "SUM(quantity)",
        ],
    );
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

// =============================================================================
// HAVING Clause Tests
// =============================================================================

#[test]
fn test_having_simple_count() {
    let query = json!({
        "groupBy": {"category": true},
        "aggregates": [{"count": {}}],
        "having": {
            "count_gt": 5
        }
    });

    let sql = parse_plan_generate(&query);

    assert_sql_contains(&sql, &["GROUP BY", "HAVING", "COUNT(*) > 5"]);
}

#[test]
fn test_having_aggregate_sum() {
    let query = json!({
        "groupBy": {"category": true},
        "aggregates": [
            {"count": {}},
            {"revenue_sum": {}}
        ],
        "having": {
            "revenue_sum_gte": 1000.0
        }
    });

    let sql = parse_plan_generate(&query);

    assert_sql_contains(&sql, &["GROUP BY", "HAVING", "SUM(revenue) >= 1000"]);
}

#[test]
fn test_having_multiple_conditions() {
    let query = json!({
        "groupBy": {"category": true},
        "aggregates": [
            {"count": {}},
            {"revenue_sum": {}},
            {"revenue_avg": {}}
        ],
        "having": {
            "count_gt": 10,
            "revenue_avg_gte": 50.0
        }
    });

    let sql = parse_plan_generate(&query);

    assert_sql_contains(
        &sql,
        &[
            "HAVING",
            "COUNT(*) > 10",
            "AVG(revenue) >= 50",
            "AND", // Multiple conditions are AND-ed
        ],
    );
}

#[test]
fn test_having_with_where() {
    // WHERE filters rows before aggregation
    // HAVING filters aggregated results
    let query = json!({
        "where": {
            "region_eq": "North"
        },
        "groupBy": {"category": true},
        "aggregates": [{"revenue_sum": {}}],
        "having": {
            "revenue_sum_gt": 5000.0
        }
    });

    let sql = parse_plan_generate(&query);

    // Verify SQL clause order: WHERE → GROUP BY → HAVING
    let where_pos = sql.find("WHERE").expect("Missing WHERE");
    let group_pos = sql.find("GROUP BY").expect("Missing GROUP BY");
    let having_pos = sql.find("HAVING").expect("Missing HAVING");

    assert!(where_pos < group_pos, "WHERE should come before GROUP BY");
    assert!(group_pos < having_pos, "GROUP BY should come before HAVING");

    assert_sql_contains(&sql, &["data->>'region'", "= 'North'", "SUM(revenue) > 5000"]);
}

#[test]
fn test_sql_clause_order_with_having() {
    let query = json!({
        "where": {"customer_id_eq": "cust-001"},
        "groupBy": {"category": true},
        "aggregates": [{"count": {}}, {"revenue_sum": {}}],
        "having": {"count_gt": 5},
        "orderBy": {"revenue_sum": "DESC"},
        "limit": 10
    });

    let sql = parse_plan_generate(&query);

    // Verify correct SQL clause order:
    // SELECT → FROM → WHERE → GROUP BY → HAVING → ORDER BY → LIMIT
    let select_pos = sql.find("SELECT").expect("Missing SELECT");
    let from_pos = sql.find("FROM").expect("Missing FROM");
    let where_pos = sql.find("WHERE").expect("Missing WHERE");
    let group_pos = sql.find("GROUP BY").expect("Missing GROUP BY");
    let having_pos = sql.find("HAVING").expect("Missing HAVING");
    let order_pos = sql.find("ORDER BY").expect("Missing ORDER BY");
    let limit_pos = sql.find("LIMIT").expect("Missing LIMIT");

    assert!(select_pos < from_pos);
    assert!(from_pos < where_pos);
    assert!(where_pos < group_pos);
    assert!(group_pos < having_pos);
    assert!(having_pos < order_pos);
    assert!(order_pos < limit_pos);
}

#[test]
fn test_having_different_operators() {
    // Test various HAVING operators: gt, gte, lt, lte, eq, neq
    let query = json!({
        "groupBy": {"category": true},
        "aggregates": [
            {"count": {}},
            {"revenue_sum": {}},
            {"revenue_avg": {}},
            {"revenue_min": {}},
            {"revenue_max": {}}
        ],
        "having": {
            "count_gt": 10,
            "revenue_sum_gte": 1000.0,
            "revenue_avg_lt": 200.0,
            "revenue_min_lte": 50.0,
            "revenue_max_eq": 500.0
        }
    });

    let sql = parse_plan_generate(&query);

    assert_sql_contains(
        &sql,
        &[
            "HAVING",
            "COUNT(*) > 10",
            "SUM(revenue) >= 1000",
            "AVG(revenue) < 200",
            "MIN(revenue) <= 50",
            "MAX(revenue) = 500",
        ],
    );
}

// =============================================================================
// Temporal Bucketing Tests
// =============================================================================

#[test]
fn test_temporal_bucket_day() {
    let query = json!({
        "groupBy": {
            "occurred_at": "day"
        },
        "aggregates": [
            {"count": {}},
            {"revenue_sum": {}}
        ]
    });

    let sql = parse_plan_generate(&query);

    assert_sql_contains(
        &sql,
        &[
            "DATE_TRUNC('day', occurred_at)", // PostgreSQL
            "GROUP BY",
        ],
    );
}

#[test]
fn test_temporal_bucket_month() {
    let query = json!({
        "groupBy": {
            "occurred_at": "month"
        },
        "aggregates": [{"count": {}}]
    });

    let sql = parse_plan_generate(&query);

    assert_sql_contains(&sql, &["DATE_TRUNC('month', occurred_at)", "GROUP BY"]);
}

#[test]
fn test_temporal_bucket_with_dimension() {
    // Mix temporal bucketing with regular dimension grouping
    let query = json!({
        "groupBy": {
            "category": true,
            "occurred_at": "day"
        },
        "aggregates": [{"count": {}}, {"revenue_sum": {}}]
    });

    let sql = parse_plan_generate(&query);

    assert_sql_contains(
        &sql,
        &[
            "data->>'category'",
            "DATE_TRUNC('day', occurred_at)",
            "GROUP BY",
        ],
    );
}

#[test]
fn test_temporal_bucket_all_types() {
    for bucket in &[
        "second", "minute", "hour", "day", "week", "month", "quarter", "year",
    ] {
        let query = json!({
            "groupBy": {
                "occurred_at": bucket
            },
            "aggregates": [{"count": {}}]
        });

        let sql = parse_plan_generate(&query);

        // Verify SQL generation doesn't panic for any bucket type
        assert!(!sql.is_empty());
        assert!(sql.contains("GROUP BY"));
    }
}

#[test]
fn test_temporal_bucket_multi_database() {
    use fraiseql_core::{
        compiler::aggregation::AggregationPlanner,
        db::DatabaseType,
        runtime::{AggregateQueryParser, AggregationSqlGenerator},
    };

    let query = json!({
        "table": "tf_sales",
        "groupBy": {"occurred_at": "day"},
        "aggregates": [{"count": {}}]
    });

    let metadata = create_sales_metadata();

    // PostgreSQL
    let pg_gen = AggregationSqlGenerator::new(DatabaseType::PostgreSQL);
    let pg_parsed = AggregateQueryParser::parse(&query, &metadata).unwrap();
    let pg_plan = AggregationPlanner::plan(pg_parsed, metadata.clone()).unwrap();
    let pg_sql = pg_gen.generate(&pg_plan).unwrap();
    assert!(pg_sql.complete_sql.contains("DATE_TRUNC('day', occurred_at)"));

    // MySQL
    let mysql_gen = AggregationSqlGenerator::new(DatabaseType::MySQL);
    let mysql_parsed = AggregateQueryParser::parse(&query, &metadata).unwrap();
    let mysql_plan = AggregationPlanner::plan(mysql_parsed, metadata.clone()).unwrap();
    let mysql_sql = mysql_gen.generate(&mysql_plan).unwrap();
    assert!(mysql_sql.complete_sql.contains("DATE_FORMAT(occurred_at,"));

    // SQLite
    let sqlite_gen = AggregationSqlGenerator::new(DatabaseType::SQLite);
    let sqlite_parsed = AggregateQueryParser::parse(&query, &metadata).unwrap();
    let sqlite_plan = AggregationPlanner::plan(sqlite_parsed, metadata.clone()).unwrap();
    let sqlite_sql = sqlite_gen.generate(&sqlite_plan).unwrap();
    assert!(sqlite_sql.complete_sql.contains("strftime("));

    // SQL Server
    let mssql_gen = AggregationSqlGenerator::new(DatabaseType::SQLServer);
    let mssql_parsed = AggregateQueryParser::parse(&query, &metadata).unwrap();
    let mssql_plan = AggregationPlanner::plan(mssql_parsed, metadata).unwrap();
    let mssql_sql = mssql_gen.generate(&mssql_plan).unwrap();
    assert!(mssql_sql.complete_sql.contains("CAST(occurred_at AS DATE)"));
}

#[test]
fn test_temporal_bucket_with_where_having() {
    let query = json!({
        "where": {
            "customer_id_eq": "cust-001"
        },
        "groupBy": {
            "occurred_at": "month"
        },
        "aggregates": [{"revenue_sum": {}}],
        "having": {
            "revenue_sum_gt": 1000.0
        },
        "orderBy": {"revenue_sum": "DESC"},
        "limit": 5
    });

    let sql = parse_plan_generate(&query);

    // Verify all clauses present with temporal bucketing
    assert_sql_contains(
        &sql,
        &[
            "WHERE",
            "customer_id = 'cust-001'",
            "DATE_TRUNC('month', occurred_at)",
            "GROUP BY",
            "HAVING",
            "SUM(revenue) > 1000",
            "ORDER BY",
            "DESC",
            "LIMIT 5",
        ],
    );
}

#[test]
fn test_temporal_bucket_week_quarter_year() {
    // Test less common bucket types
    for bucket in &["week", "quarter", "year"] {
        let query = json!({
            "groupBy": {
                "occurred_at": bucket
            },
            "aggregates": [{"count": {}}, {"revenue_sum": {}}]
        });

        let sql = parse_plan_generate(&query);

        // PostgreSQL generates DATE_TRUNC for all types
        assert!(sql.contains(&format!("DATE_TRUNC('{}', occurred_at)", bucket)));
        assert!(sql.contains("GROUP BY"));
        assert!(sql.contains("COUNT(*)"));
        assert!(sql.contains("SUM(revenue)"));
    }
}

// =============================================================================
// Advanced Aggregates Tests
// =============================================================================

#[test]
fn test_string_agg_simple() {
    let query = json!({
        "groupBy": {"category": true},
        "aggregates": [
            {"count": {}},
            {"customer_id_string_agg": {}}
        ]
    });

    let sql = parse_plan_generate(&query);

    // PostgreSQL uses STRING_AGG
    assert_sql_contains(&sql, &["GROUP BY", "STRING_AGG(customer_id"]);
}

#[test]
fn test_array_agg_simple() {
    let query = json!({
        "groupBy": {"category": true},
        "aggregates": [
            {"count": {}},
            {"customer_id_array_agg": {}}
        ]
    });

    let sql = parse_plan_generate(&query);

    // PostgreSQL uses ARRAY_AGG
    assert_sql_contains(&sql, &["GROUP BY", "ARRAY_AGG(customer_id"]);
}

#[test]
fn test_advanced_aggregates_multi_database() {
    use fraiseql_core::{
        compiler::aggregation::AggregationPlanner,
        db::DatabaseType,
        runtime::{AggregateQueryParser, AggregationSqlGenerator},
    };

    let query = json!({
        "table": "tf_sales",
        "groupBy": {"category": true},
        "aggregates": [
            {"customer_id_string_agg": {}}
        ]
    });

    let metadata = create_sales_metadata();

    // PostgreSQL
    let pg_gen = AggregationSqlGenerator::new(DatabaseType::PostgreSQL);
    let pg_parsed = AggregateQueryParser::parse(&query, &metadata).unwrap();
    let pg_plan = AggregationPlanner::plan(pg_parsed, metadata.clone()).unwrap();
    let pg_sql = pg_gen.generate(&pg_plan).unwrap();
    assert!(pg_sql.complete_sql.contains("STRING_AGG(customer_id"));

    // MySQL
    let mysql_gen = AggregationSqlGenerator::new(DatabaseType::MySQL);
    let mysql_parsed = AggregateQueryParser::parse(&query, &metadata).unwrap();
    let mysql_plan = AggregationPlanner::plan(mysql_parsed, metadata.clone()).unwrap();
    let mysql_sql = mysql_gen.generate(&mysql_plan).unwrap();
    assert!(mysql_sql.complete_sql.contains("GROUP_CONCAT(customer_id"));

    // SQLite
    let sqlite_gen = AggregationSqlGenerator::new(DatabaseType::SQLite);
    let sqlite_parsed = AggregateQueryParser::parse(&query, &metadata).unwrap();
    let sqlite_plan = AggregationPlanner::plan(sqlite_parsed, metadata.clone()).unwrap();
    let sqlite_sql = sqlite_gen.generate(&sqlite_plan).unwrap();
    assert!(sqlite_sql.complete_sql.contains("GROUP_CONCAT(customer_id"));

    // SQL Server
    let mssql_gen = AggregationSqlGenerator::new(DatabaseType::SQLServer);
    let mssql_parsed = AggregateQueryParser::parse(&query, &metadata).unwrap();
    let mssql_plan = AggregationPlanner::plan(mssql_parsed, metadata).unwrap();
    let mssql_sql = mssql_gen.generate(&mssql_plan).unwrap();
    assert!(mssql_sql.complete_sql.contains("STRING_AGG"));
}

// =============================================================================
// Statistical Functions Tests
// =============================================================================

#[test]
fn test_stddev_postgres_mysql() {
    use fraiseql_core::{
        compiler::aggregation::AggregationPlanner,
        db::DatabaseType,
        runtime::{AggregateQueryParser, AggregationSqlGenerator},
    };

    let query = json!({
        "table": "tf_sales",
        "groupBy": {"category": true},
        "aggregates": [
            {"count": {}},
            {"revenue_stddev": {}}
        ]
    });

    let metadata = create_sales_metadata();

    // PostgreSQL
    let pg_gen = AggregationSqlGenerator::new(DatabaseType::PostgreSQL);
    let pg_parsed = AggregateQueryParser::parse(&query, &metadata).unwrap();
    let pg_plan = AggregationPlanner::plan(pg_parsed, metadata.clone()).unwrap();
    let pg_sql = pg_gen.generate(&pg_plan).unwrap();
    assert!(pg_sql.complete_sql.contains("STDDEV_SAMP(revenue)"));

    // MySQL
    let mysql_gen = AggregationSqlGenerator::new(DatabaseType::MySQL);
    let mysql_parsed = AggregateQueryParser::parse(&query, &metadata).unwrap();
    let mysql_plan = AggregationPlanner::plan(mysql_parsed, metadata.clone()).unwrap();
    let mysql_sql = mysql_gen.generate(&mysql_plan).unwrap();
    assert!(mysql_sql.complete_sql.contains("STDDEV_SAMP(revenue)"));

    // SQL Server
    let mssql_gen = AggregationSqlGenerator::new(DatabaseType::SQLServer);
    let mssql_parsed = AggregateQueryParser::parse(&query, &metadata).unwrap();
    let mssql_plan = AggregationPlanner::plan(mssql_parsed, metadata).unwrap();
    let mssql_sql = mssql_gen.generate(&mssql_plan).unwrap();
    assert!(mssql_sql.complete_sql.contains("STDEV(revenue)"));
}

#[test]
fn test_variance_postgres_mysql() {
    use fraiseql_core::{
        compiler::aggregation::AggregationPlanner,
        db::DatabaseType,
        runtime::{AggregateQueryParser, AggregationSqlGenerator},
    };

    let query = json!({
        "table": "tf_sales",
        "groupBy": {"category": true},
        "aggregates": [
            {"count": {}},
            {"revenue_variance": {}}
        ]
    });

    let metadata = create_sales_metadata();

    // PostgreSQL
    let pg_gen = AggregationSqlGenerator::new(DatabaseType::PostgreSQL);
    let pg_parsed = AggregateQueryParser::parse(&query, &metadata).unwrap();
    let pg_plan = AggregationPlanner::plan(pg_parsed, metadata.clone()).unwrap();
    let pg_sql = pg_gen.generate(&pg_plan).unwrap();
    assert!(pg_sql.complete_sql.contains("VAR_SAMP(revenue)"));

    // MySQL
    let mysql_gen = AggregationSqlGenerator::new(DatabaseType::MySQL);
    let mysql_parsed = AggregateQueryParser::parse(&query, &metadata).unwrap();
    let mysql_plan = AggregationPlanner::plan(mysql_parsed, metadata.clone()).unwrap();
    let mysql_sql = mysql_gen.generate(&mysql_plan).unwrap();
    assert!(mysql_sql.complete_sql.contains("VAR_SAMP(revenue)"));

    // SQL Server
    let mssql_gen = AggregationSqlGenerator::new(DatabaseType::SQLServer);
    let mssql_parsed = AggregateQueryParser::parse(&query, &metadata).unwrap();
    let mssql_plan = AggregationPlanner::plan(mssql_parsed, metadata).unwrap();
    let mssql_sql = mssql_gen.generate(&mssql_plan).unwrap();
    assert!(mssql_sql.complete_sql.contains("VAR(revenue)"));
}

#[test]
fn test_statistical_functions_sqlite_unsupported() {
    use fraiseql_core::{
        compiler::aggregation::AggregationPlanner,
        db::DatabaseType,
        runtime::{AggregateQueryParser, AggregationSqlGenerator},
    };

    let query = json!({
        "table": "tf_sales",
        "groupBy": {"category": true},
        "aggregates": [
            {"count": {}},
            {"revenue_stddev": {}},
            {"revenue_variance": {}}
        ]
    });

    let metadata = create_sales_metadata();

    // SQLite - should return NULL placeholders
    let sqlite_gen = AggregationSqlGenerator::new(DatabaseType::SQLite);
    let sqlite_parsed = AggregateQueryParser::parse(&query, &metadata).unwrap();
    let sqlite_plan = AggregationPlanner::plan(sqlite_parsed, metadata).unwrap();
    let sqlite_sql = sqlite_gen.generate(&sqlite_plan).unwrap();

    // SQLite doesn't support STDDEV/VARIANCE natively
    assert!(sqlite_sql.complete_sql.contains("NULL /* STDDEV not supported"));
    assert!(sqlite_sql.complete_sql.contains("NULL /* VARIANCE not supported"));
}
