#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use super::*;
use crate::compiler::{
    aggregate_types::HavingOperator,
    aggregation::{AggregateSelection, AggregationRequest, GroupBySelection},
    fact_table::{DimensionColumn, FactTableMetadata, FilterColumn, MeasureColumn, SqlType},
};

/// Metadata helper for integration tests — includes `customer_id` as a native int8 column.
fn create_aggregation_test_metadata() -> crate::compiler::fact_table::FactTableMetadata {
    use crate::compiler::fact_table::{DimensionColumn, FilterColumn, MeasureColumn, SqlType};
    crate::compiler::fact_table::FactTableMetadata {
        table_name:           "tf_sales".to_string(),
        measures:             vec![MeasureColumn {
            name:     "revenue".to_string(),
            sql_type: SqlType::Decimal,
            nullable: false,
        }],
        dimensions:           DimensionColumn {
            name:  "data".to_string(),
            paths: vec![],
        },
        denormalized_filters: vec![FilterColumn {
            name:     "customer_id".to_string(),
            sql_type: SqlType::BigInt,
            indexed:  true,
        }],
        calendar_dimensions:  vec![],
        partial_period:       None,
    }
}

fn create_test_plan() -> AggregationPlan {
    let metadata = FactTableMetadata {
        table_name:           "tf_sales".to_string(),
        measures:             vec![MeasureColumn {
            name:     "revenue".to_string(),
            sql_type: SqlType::Decimal,
            nullable: false,
        }],
        dimensions:           DimensionColumn {
            name:  "dimensions".to_string(),
            paths: vec![],
        },
        denormalized_filters: vec![FilterColumn {
            name:     "occurred_at".to_string(),
            sql_type: SqlType::Timestamp,
            indexed:  true,
        }],
        calendar_dimensions:  vec![],
        partial_period:       None,
    };

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
                alias:  "day".to_string(),
            },
        ],
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
        limit:        Some(10),
        offset:       None,
    };

    crate::compiler::aggregation::AggregationPlanner::plan(request, metadata).unwrap()
}

#[test]
fn test_postgres_sql_generation() {
    let plan = create_test_plan();
    let generator = AggregationSqlGenerator::new(DatabaseType::PostgreSQL);
    let sql = generator.generate_parameterized(&plan).unwrap();

    assert!(sql.sql.contains("dimensions->>'category'"));
    assert!(sql.sql.contains("DATE_TRUNC('day', occurred_at)"));
    assert!(sql.sql.contains("COUNT(*)"));
    assert!(sql.sql.contains("SUM(revenue)"));
    assert!(sql.sql.contains("GROUP BY"));
    assert!(sql.sql.contains("LIMIT 10"));
}

#[test]
fn test_mysql_sql_generation() {
    let plan = create_test_plan();
    let generator = AggregationSqlGenerator::new(DatabaseType::MySQL);
    let sql = generator.generate_parameterized(&plan).unwrap();

    assert!(sql.sql.contains("JSON_UNQUOTE(JSON_EXTRACT(dimensions, '$.category'))"));
    assert!(sql.sql.contains("DATE_FORMAT(occurred_at"));
    assert!(sql.sql.contains("COUNT(*)"));
    assert!(sql.sql.contains("SUM(revenue)"));
}

#[test]
fn test_sqlite_sql_generation() {
    let plan = create_test_plan();
    let generator = AggregationSqlGenerator::new(DatabaseType::SQLite);
    let sql = generator.generate_parameterized(&plan).unwrap();

    assert!(sql.sql.contains("json_extract(dimensions, '$.category')"));
    assert!(sql.sql.contains("strftime"));
    assert!(sql.sql.contains("COUNT(*)"));
    assert!(sql.sql.contains("SUM(revenue)"));
}

#[test]
fn test_sqlserver_sql_generation() {
    let plan = create_test_plan();
    let generator = AggregationSqlGenerator::new(DatabaseType::SQLServer);
    let sql = generator.generate_parameterized(&plan).unwrap();

    assert!(sql.sql.contains("JSON_VALUE(dimensions, '$.category')"));
    assert!(sql.sql.contains("CAST(occurred_at AS DATE)"));
    assert!(sql.sql.contains("COUNT(*)"));
    assert!(sql.sql.contains("SUM(revenue)"));
}

#[test]
fn test_having_clause() {
    let mut plan = create_test_plan();
    plan.having_conditions = vec![ValidatedHavingCondition {
        aggregate: AggregateExpression::MeasureAggregate {
            column:   "revenue".to_string(),
            function: AggregateFunction::Sum,
            alias:    "revenue_sum".to_string(),
        },
        operator:  HavingOperator::Gt,
        value:     serde_json::json!(1000),
    }];

    let generator = AggregationSqlGenerator::new(DatabaseType::PostgreSQL);
    let sql = generator.generate_parameterized(&plan).unwrap();

    assert!(sql.sql.contains("HAVING SUM(revenue) > $1"));
    assert_eq!(sql.params, vec![serde_json::json!(1000)]);
}

#[test]
fn test_order_by_clause() {
    use crate::compiler::aggregation::OrderByClause;

    let mut plan = create_test_plan();
    plan.request.order_by = vec![OrderByClause::new(
        "revenue_sum".to_string(),
        OrderDirection::Desc,
    )];

    let generator = AggregationSqlGenerator::new(DatabaseType::PostgreSQL);
    let sql = generator.generate_parameterized(&plan).unwrap();

    assert!(sql.sql.contains("ORDER BY \"revenue_sum\" DESC"));
}

// ========================================
// NativeField WHERE clause tests
// ========================================

mod native_where {
    use fraiseql_db::where_clause::{WhereClause, WhereOperator};

    use super::*;

    fn plan_with_native_where(
        column: &str,
        pg_cast: &str,
        value: serde_json::Value,
    ) -> AggregationPlan {
        let mut plan = create_test_plan();
        plan.request.where_clause = Some(WhereClause::NativeField {
            column: column.to_string(),
            pg_cast: pg_cast.to_string(),
            operator: WhereOperator::Eq,
            value,
        });
        plan
    }

    #[test]
    fn postgres_native_uuid_where() {
        let plan = plan_with_native_where("order_id", "uuid", serde_json::json!("abc-123"));
        let gen = AggregationSqlGenerator::new(DatabaseType::PostgreSQL);
        let sql = gen.generate_parameterized(&plan).unwrap().sql;
        assert!(sql.contains(r#""order_id" = $1::uuid"#), "got: {sql}");
    }

    #[test]
    fn postgres_native_int_where() {
        let plan = plan_with_native_where("customer_id", "int8", serde_json::json!(42));
        let gen = AggregationSqlGenerator::new(DatabaseType::PostgreSQL);
        let sql = gen.generate_parameterized(&plan).unwrap().sql;
        assert!(sql.contains(r#""customer_id" = $1::int8"#), "got: {sql}");
    }

    #[test]
    fn postgres_native_no_cast_where() {
        let plan = plan_with_native_where("status", "", serde_json::json!("active"));
        let gen = AggregationSqlGenerator::new(DatabaseType::PostgreSQL);
        let sql = gen.generate_parameterized(&plan).unwrap().sql;
        assert!(sql.contains(r#""status" = $1"#), "got: {sql}");
        assert!(!sql.contains("::"), "unexpected cast: {sql}");
    }

    #[test]
    fn mysql_native_where() {
        let plan = plan_with_native_where("customer_id", "int8", serde_json::json!(42));
        let gen = AggregationSqlGenerator::new(DatabaseType::MySQL);
        let sql = gen.generate_parameterized(&plan).unwrap().sql;
        assert!(sql.contains("`customer_id` = ?"), "got: {sql}");
    }

    #[test]
    fn sqlite_native_where() {
        let plan = plan_with_native_where("customer_id", "int8", serde_json::json!(42));
        let gen = AggregationSqlGenerator::new(DatabaseType::SQLite);
        let sql = gen.generate_parameterized(&plan).unwrap().sql;
        assert!(sql.contains(r#""customer_id" = ?"#), "got: {sql}");
    }

    #[test]
    fn sqlserver_native_where() {
        let plan = plan_with_native_where("customer_id", "int8", serde_json::json!(42));
        let gen = AggregationSqlGenerator::new(DatabaseType::SQLServer);
        let sql = gen.generate_parameterized(&plan).unwrap().sql;
        assert!(sql.contains("[customer_id] = @P1"), "got: {sql}");
    }

    #[test]
    fn and_wrapping_native_field() {
        let mut plan = create_test_plan();
        plan.request.where_clause = Some(WhereClause::And(vec![
            WhereClause::NativeField {
                column:   "customer_id".to_string(),
                pg_cast:  "int8".to_string(),
                operator: WhereOperator::Eq,
                value:    serde_json::json!(1),
            },
            WhereClause::NativeField {
                column:   "status".to_string(),
                pg_cast:  String::new(),
                operator: WhereOperator::Eq,
                value:    serde_json::json!("active"),
            },
        ]));
        let gen = AggregationSqlGenerator::new(DatabaseType::PostgreSQL);
        let sql = gen.generate_parameterized(&plan).unwrap().sql;
        assert!(sql.contains(r#""customer_id" = $1::int8"#), "got: {sql}");
        assert!(sql.contains(r#""status" = $2"#), "got: {sql}");
    }
}

// ========================================
// NativeColumn GROUP BY variant tests
// ========================================

mod native_groupby {
    use super::*;
    use crate::compiler::aggregation::GroupByExpression;

    fn plan_with_native_groupby() -> AggregationPlan {
        let mut plan = create_test_plan();
        plan.group_by_expressions = vec![GroupByExpression::NativeColumn {
            column:  "customer_id".to_string(),
            pg_cast: "int8".to_string(),
            alias:   "customer_id".to_string(),
        }];
        plan
    }

    #[test]
    fn postgres_native_groupby_select_clause() {
        let plan = plan_with_native_groupby();
        let gen = AggregationSqlGenerator::new(DatabaseType::PostgreSQL);
        let sql = gen.generate_parameterized(&plan).unwrap().sql;
        assert!(sql.contains(r#""customer_id" AS customer_id"#), "got: {sql}");
        assert!(!sql.contains("data->>'customer_id'"), "unexpected JSONB ref: {sql}");
    }

    #[test]
    fn postgres_native_groupby_group_by_clause() {
        let plan = plan_with_native_groupby();
        let gen = AggregationSqlGenerator::new(DatabaseType::PostgreSQL);
        let sql = gen.generate_parameterized(&plan).unwrap().sql;
        assert!(sql.contains(r#"GROUP BY "customer_id""#), "got: {sql}");
        assert!(!sql.contains("data->>'customer_id'"), "unexpected JSONB ref: {sql}");
    }

    #[test]
    fn mixed_native_and_jsonb_groupby() {
        let mut plan = create_test_plan();
        plan.group_by_expressions = vec![
            GroupByExpression::NativeColumn {
                column:  "customer_id".to_string(),
                pg_cast: "int8".to_string(),
                alias:   "customer_id".to_string(),
            },
            GroupByExpression::JsonbPath {
                jsonb_column: "data".to_string(),
                path:         "status".to_string(),
                alias:        "status".to_string(),
            },
        ];
        let gen = AggregationSqlGenerator::new(DatabaseType::PostgreSQL);
        let sql = gen.generate_parameterized(&plan).unwrap().sql;
        assert!(sql.contains(r#""customer_id" AS customer_id"#), "got: {sql}");
        // PostgreSQL jsonb_extract_sql produces a trailing space before AS
        assert!(sql.contains("data->>'status'"), "got: {sql}");
        assert!(sql.contains("AS status"), "got: {sql}");
        assert!(sql.contains(r#""customer_id""#), "got: {sql}");
    }

    #[test]
    fn mysql_native_groupby() {
        let plan = plan_with_native_groupby();
        let gen = AggregationSqlGenerator::new(DatabaseType::MySQL);
        let sql = gen.generate_parameterized(&plan).unwrap().sql;
        assert!(sql.contains("`customer_id` AS customer_id"), "got: {sql}");
        assert!(sql.contains("GROUP BY `customer_id`"), "got: {sql}");
    }
}

// ========================================
// native_columns threading integration tests
// ========================================

mod native_columns_integration {
    use std::collections::HashMap;

    use fraiseql_db::where_clause::WhereClause;

    use super::*;
    use crate::{
        compiler::aggregation::{AggregationPlanner, GroupByExpression},
        runtime::aggregate_parser::AggregateQueryParser,
    };

    fn native_cols() -> HashMap<String, String> {
        std::iter::once(("customer_id".to_string(), "int8".to_string())).collect()
    }

    #[test]
    fn parser_emits_native_where_when_column_in_map() {
        let query_json = serde_json::json!({
            "table": "tf_sales",
            "where": { "customer_id_eq": 42 },
            "groupBy": { "status": true },
            "aggregates": [{ "count": {} }]
        });
        let metadata = create_aggregation_test_metadata();
        let native = native_cols();

        let request = AggregateQueryParser::parse(&query_json, &metadata, &native).unwrap();

        let where_clause = request.where_clause.unwrap();
        let found_native = match &where_clause {
            WhereClause::And(clauses) => clauses.iter().any(
                |c| matches!(c, WhereClause::NativeField { column, .. } if column == "customer_id"),
            ),
            WhereClause::NativeField { column, .. } => column == "customer_id",
            _ => false,
        };
        assert!(found_native, "expected NativeField for customer_id, got: {where_clause:?}");
    }

    #[test]
    fn parser_emits_native_groupby_when_column_in_map() {
        let query_json = serde_json::json!({
            "table": "tf_sales",
            "groupBy": { "customer_id": true, "status": true },
            "aggregates": [{ "count": {} }]
        });
        let metadata = create_aggregation_test_metadata();
        let native = native_cols();

        let request = AggregateQueryParser::parse(&query_json, &metadata, &native).unwrap();
        let plan = AggregationPlanner::plan(request, metadata).unwrap();

        let has_native = plan.group_by_expressions.iter().any(|e| {
            matches!(e, GroupByExpression::NativeColumn { column, .. } if column == "customer_id")
        });
        assert!(
            has_native,
            "expected NativeColumn for customer_id; got: {:?}",
            plan.group_by_expressions
        );

        let has_jsonb = plan
            .group_by_expressions
            .iter()
            .any(|e| matches!(e, GroupByExpression::JsonbPath { path, .. } if path == "status"));
        assert!(has_jsonb, "expected JsonbPath for status; got: {:?}", plan.group_by_expressions);
    }

    #[test]
    fn full_sql_uses_native_column_references() {
        let query_json = serde_json::json!({
            "table": "tf_sales",
            "where": { "customer_id_eq": 42 },
            "groupBy": { "customer_id": true },
            "aggregates": [{ "count": {} }]
        });
        let metadata = create_aggregation_test_metadata();
        let native = native_cols();

        let request = AggregateQueryParser::parse(&query_json, &metadata, &native).unwrap();
        let plan = AggregationPlanner::plan(request, metadata).unwrap();
        let gen = AggregationSqlGenerator::new(DatabaseType::PostgreSQL);
        let result = gen.generate_parameterized(&plan).unwrap();

        assert!(result.sql.contains(r#""customer_id""#), "got: {}", result.sql);
        assert!(result.sql.contains("$1::int8"), "got: {}", result.sql);
        assert!(!result.sql.contains("data->>'customer_id'"), "unexpected JSONB: {}", result.sql);
    }

    #[test]
    fn empty_native_map_falls_back_to_jsonb() {
        let query_json = serde_json::json!({
            "table": "tf_sales",
            "groupBy": { "customer_id": true },
            "aggregates": [{ "count": {} }]
        });
        let metadata = create_aggregation_test_metadata();
        let empty: HashMap<String, String> = HashMap::new();

        let request = AggregateQueryParser::parse(&query_json, &metadata, &empty).unwrap();
        let plan = AggregationPlanner::plan(request, metadata).unwrap();
        let gen = AggregationSqlGenerator::new(DatabaseType::PostgreSQL);
        let result = gen.generate_parameterized(&plan).unwrap();

        // Without native_columns, falls back to JSONB extraction
        assert!(
            result.sql.contains("data->>'customer_id'"),
            "expected JSONB fallback: {}",
            result.sql
        );
    }
}

// ========================================
// native_aliases and ORDER BY tests
// ========================================

mod native_orderby {
    use super::*;
    use crate::compiler::aggregation::{GroupByExpression, OrderByClause, OrderDirection};

    #[test]
    fn order_by_native_column_uses_alias_not_jsonb() {
        let mut plan = create_test_plan();
        plan.group_by_expressions = vec![GroupByExpression::NativeColumn {
            column:  "customer_id".to_string(),
            pg_cast: "int8".to_string(),
            alias:   "customer_id".to_string(),
        }];
        plan.request.order_by = vec![OrderByClause::new(
            "customer_id".to_string(),
            OrderDirection::Asc,
        )];

        let gen = AggregationSqlGenerator::new(DatabaseType::PostgreSQL);
        let sql = gen.generate_parameterized(&plan).unwrap().sql;

        assert!(sql.contains(r#"ORDER BY "customer_id" ASC"#), "got: {sql}");
        assert!(!sql.contains("data->>'customer_id'"), "unexpected JSONB in ORDER BY: {sql}");
    }

    #[test]
    fn order_by_jsonb_dimension_unchanged() {
        let mut plan = create_test_plan();
        plan.request.order_by = vec![OrderByClause::new(
            "category".to_string(),
            OrderDirection::Desc,
        )];

        let gen = AggregationSqlGenerator::new(DatabaseType::PostgreSQL);
        let sql = gen.generate_parameterized(&plan).unwrap().sql;

        assert!(sql.contains(r#"ORDER BY "category" DESC"#), "got: {sql}");
    }

    #[test]
    fn native_aliases_helper_returns_correct_set() {
        let mut plan = create_test_plan();
        plan.group_by_expressions = vec![
            GroupByExpression::NativeColumn {
                column:  "customer_id".to_string(),
                pg_cast: "int8".to_string(),
                alias:   "customer_id".to_string(),
            },
            GroupByExpression::JsonbPath {
                jsonb_column: "data".to_string(),
                path:         "status".to_string(),
                alias:        "status".to_string(),
            },
        ];

        let aliases = plan.native_aliases();
        assert!(aliases.contains("customer_id"), "expected customer_id in {aliases:?}");
        assert!(!aliases.contains("status"), "status should not be native: {aliases:?}");
    }
}

// ========================================
// Advanced Aggregates Tests
// ========================================

#[test]
fn test_array_agg_postgres() {
    let generator = AggregationSqlGenerator::new(DatabaseType::PostgreSQL);

    // Test without ORDER BY
    let sql = generator.generate_array_agg_sql("product_id", None);
    assert_eq!(sql, "ARRAY_AGG(product_id)");

    // Test with ORDER BY
    let order_by = vec![OrderByClause::new(
        "revenue".to_string(),
        OrderDirection::Desc,
    )];
    let sql = generator.generate_array_agg_sql("product_id", Some(&order_by));
    assert_eq!(sql, "ARRAY_AGG(product_id ORDER BY \"revenue\" DESC)");
}

#[test]
fn test_array_agg_mysql() {
    let generator = AggregationSqlGenerator::new(DatabaseType::MySQL);
    let sql = generator.generate_array_agg_sql("product_id", None);
    assert_eq!(sql, "JSON_ARRAYAGG(product_id)");
}

#[test]
fn test_array_agg_sqlite() {
    let generator = AggregationSqlGenerator::new(DatabaseType::SQLite);
    let sql = generator.generate_array_agg_sql("product_id", None);
    assert!(sql.contains("GROUP_CONCAT"));
    assert!(sql.contains("'[' ||"));
    assert!(sql.contains("|| ']'"));
}

#[test]
fn test_string_agg_postgres() {
    let generator = AggregationSqlGenerator::new(DatabaseType::PostgreSQL);

    // Test without ORDER BY
    let sql = generator.generate_string_agg_sql("product_name", ", ", None);
    assert_eq!(sql, "STRING_AGG(product_name, ', ')");

    // Test with ORDER BY
    let order_by = vec![OrderByClause::new(
        "revenue".to_string(),
        OrderDirection::Desc,
    )];
    let sql = generator.generate_string_agg_sql("product_name", ", ", Some(&order_by));
    assert_eq!(sql, "STRING_AGG(product_name, ', ' ORDER BY \"revenue\" DESC)");
}

#[test]
fn test_string_agg_mysql() {
    let generator = AggregationSqlGenerator::new(DatabaseType::MySQL);

    let order_by = vec![OrderByClause::new(
        "revenue".to_string(),
        OrderDirection::Desc,
    )];
    let sql = generator.generate_string_agg_sql("product_name", ", ", Some(&order_by));
    assert_eq!(sql, "GROUP_CONCAT(product_name ORDER BY `revenue` DESC SEPARATOR ', ')");
}

#[test]
fn test_string_agg_sqlserver() {
    let generator = AggregationSqlGenerator::new(DatabaseType::SQLServer);

    let order_by = vec![OrderByClause::new(
        "revenue".to_string(),
        OrderDirection::Desc,
    )];
    let sql = generator.generate_string_agg_sql("product_name", ", ", Some(&order_by));
    assert!(sql.contains("STRING_AGG(CAST(product_name AS NVARCHAR(MAX)), ', ')"));
    assert!(sql.contains("WITHIN GROUP (ORDER BY [revenue] DESC)"));
}

#[test]
fn test_json_agg_postgres() {
    let generator = AggregationSqlGenerator::new(DatabaseType::PostgreSQL);
    let sql = generator.generate_json_agg_sql("data", None);
    assert_eq!(sql, "JSON_AGG(data)");

    let order_by = vec![OrderByClause::new(
        "created_at".to_string(),
        OrderDirection::Asc,
    )];
    let sql = generator.generate_json_agg_sql("data", Some(&order_by));
    assert_eq!(sql, "JSON_AGG(data ORDER BY \"created_at\" ASC)");
}

#[test]
fn test_jsonb_agg_postgres() {
    let generator = AggregationSqlGenerator::new(DatabaseType::PostgreSQL);
    let sql = generator.generate_jsonb_agg_sql("data", None);
    assert_eq!(sql, "JSONB_AGG(data)");
}

#[test]
fn test_bool_and_postgres() {
    use crate::compiler::aggregate_types::BoolAggregateFunction;

    let generator = AggregationSqlGenerator::new(DatabaseType::PostgreSQL);
    let sql = generator.generate_bool_agg_sql("is_active", BoolAggregateFunction::And);
    assert_eq!(sql, "BOOL_AND(is_active)");

    let sql = generator.generate_bool_agg_sql("has_discount", BoolAggregateFunction::Or);
    assert_eq!(sql, "BOOL_OR(has_discount)");
}

#[test]
fn test_bool_and_mysql() {
    use crate::compiler::aggregate_types::BoolAggregateFunction;

    let generator = AggregationSqlGenerator::new(DatabaseType::MySQL);
    let sql = generator.generate_bool_agg_sql("is_active", BoolAggregateFunction::And);
    assert_eq!(sql, "MIN(is_active) = 1");

    let sql = generator.generate_bool_agg_sql("has_discount", BoolAggregateFunction::Or);
    assert_eq!(sql, "MAX(has_discount) = 1");
}

#[test]
fn test_bool_and_sqlserver() {
    use crate::compiler::aggregate_types::BoolAggregateFunction;

    let generator = AggregationSqlGenerator::new(DatabaseType::SQLServer);
    let sql = generator.generate_bool_agg_sql("is_active", BoolAggregateFunction::And);
    assert_eq!(sql, "MIN(CAST(is_active AS BIT)) = 1");

    let sql = generator.generate_bool_agg_sql("has_discount", BoolAggregateFunction::Or);
    assert_eq!(sql, "MAX(CAST(has_discount AS BIT)) = 1");
}

#[test]
fn test_advanced_aggregate_full_query() {
    // Create a plan with advanced aggregates
    let mut plan = create_test_plan();

    // Add an ARRAY_AGG aggregate
    plan.aggregate_expressions.push(AggregateExpression::AdvancedAggregate {
        column:    "product_id".to_string(),
        function:  AggregateFunction::ArrayAgg,
        alias:     "products".to_string(),
        delimiter: None,
        order_by:  Some(vec![OrderByClause::new(
            "revenue".to_string(),
            OrderDirection::Desc,
        )]),
    });

    // Add a STRING_AGG aggregate
    plan.aggregate_expressions.push(AggregateExpression::AdvancedAggregate {
        column:    "product_name".to_string(),
        function:  AggregateFunction::StringAgg,
        alias:     "product_names".to_string(),
        delimiter: Some(", ".to_string()),
        order_by:  None,
    });

    let generator = AggregationSqlGenerator::new(DatabaseType::PostgreSQL);
    let sql = generator.generate_parameterized(&plan).unwrap();

    assert!(sql.sql.contains("ARRAY_AGG(product_id ORDER BY \"revenue\" DESC)"));
    assert!(sql.sql.contains("STRING_AGG(product_name, ', ')"));
}

// ========================================
// Security / Escaping Tests
// ========================================

#[test]
fn test_having_string_value_is_bound_not_escaped() {
    use crate::compiler::aggregate_types::AggregateFunction;

    let mut plan = create_test_plan();
    plan.having_conditions = vec![ValidatedHavingCondition {
        aggregate: AggregateExpression::MeasureAggregate {
            column:   "label".to_string(),
            function: AggregateFunction::Max,
            alias:    "label_max".to_string(),
        },
        operator:  HavingOperator::Eq,
        value:     serde_json::json!("O'Reilly"),
    }];

    let generator = AggregationSqlGenerator::new(DatabaseType::PostgreSQL);
    let sql = generator.generate_parameterized(&plan).unwrap();

    // Value must be a bind parameter — raw string must never appear in SQL.
    assert!(sql.sql.contains("HAVING MAX(label) = $1"));
    assert!(!sql.sql.contains("O'Reilly"), "raw string must not appear in SQL: {}", sql.sql);
    assert_eq!(sql.params, vec![serde_json::json!("O'Reilly")]);
}

#[test]
fn test_escape_sql_string_mysql_doubles_backslash() {
    // MySQL treats backslash as an escape character in string literals.
    // A bare backslash before the closing quote would consume it, breaking the SQL.
    let gen = AggregationSqlGenerator::new(DatabaseType::MySQL);
    assert_eq!(gen.escape_sql_string("test\\"), "test\\\\");
    assert_eq!(gen.escape_sql_string("te'st"), "te''st");
    // Backslash followed by a quote: escape backslash first (→ \\), then double the
    // quote (→ '').  Result for te\'st is te\\''st.
    assert_eq!(gen.escape_sql_string("te\\'st"), "te\\\\''st");
}

#[test]
fn test_escape_sql_string_postgres_only_doubles_quote() {
    let gen = AggregationSqlGenerator::new(DatabaseType::PostgreSQL);
    // Backslash is not special in standard SQL string literals.
    assert_eq!(gen.escape_sql_string("test\\"), "test\\");
    assert_eq!(gen.escape_sql_string("te'st"), "te''st");
}

#[test]
fn test_escape_sql_string_strips_null_bytes() {
    // Null bytes are never valid in SQL string literals.
    // PostgreSQL rejects them with "invalid byte sequence"; stripping is safer than an error.
    let gen = AggregationSqlGenerator::new(DatabaseType::PostgreSQL);
    assert_eq!(gen.escape_sql_string("before\x00after"), "beforeafter");
    assert_eq!(gen.escape_sql_string("\x00"), "");
    assert_eq!(gen.escape_sql_string("no-null"), "no-null");

    // Same for MySQL — null stripping happens before backslash/quote escaping.
    let mysql = AggregationSqlGenerator::new(DatabaseType::MySQL);
    assert_eq!(mysql.escape_sql_string("te\x00st\\"), "test\\\\");
}

// ── jsonb_extract_sql injection tests ──────────────────────────────────────

#[test]
fn test_jsonb_postgres_single_quote_escaped() {
    let gen = AggregationSqlGenerator::new(DatabaseType::PostgreSQL);
    let sql = gen.jsonb_extract_sql("dimensions", "user'name");
    // Single quote must be doubled; must not break out of the string literal.
    assert!(sql.contains("user''name"), "Expected doubled quote, got: {sql}");
    assert!(!sql.contains("user'name'"), "Unescaped quote still present");
}

#[test]
fn test_jsonb_postgres_pg_sleep_injection_neutralised() {
    let gen = AggregationSqlGenerator::new(DatabaseType::PostgreSQL);
    let sql = gen.jsonb_extract_sql("dimensions", "a' || pg_sleep(10) --");
    // The injected payload must appear inside the string literal (quote doubled).
    assert!(sql.contains("a'' || pg_sleep(10) --"), "Escaping not applied: {sql}");
}

#[test]
fn test_jsonb_postgres_clean_path_unchanged() {
    let gen = AggregationSqlGenerator::new(DatabaseType::PostgreSQL);
    let sql = gen.jsonb_extract_sql("dimensions", "category");
    assert!(sql.contains("dimensions->>'category'"), "Clean path altered: {sql}");
}

#[test]
fn test_jsonb_mysql_single_quote_escaped() {
    let gen = AggregationSqlGenerator::new(DatabaseType::MySQL);
    let sql = gen.jsonb_extract_sql("dimensions", "user'name");
    // MySQL JSON paths use doubled-quote escaping (''): backslash escaping is NOT used.
    assert!(sql.contains("user''name"), "Expected doubled-quote escape in MySQL: {sql}");
}

#[test]
fn test_jsonb_mysql_path_prefix_not_doubled() {
    // escape_mysql_json_path already adds "$." — must not appear as "$.$.path"
    let gen = AggregationSqlGenerator::new(DatabaseType::MySQL);
    let sql = gen.jsonb_extract_sql("dimensions", "category");
    assert!(sql.contains("$.category"), "Path prefix missing: {sql}");
    assert!(!sql.contains("$.$."), "Double prefix detected: {sql}");
}

#[test]
fn test_jsonb_sqlite_single_quote_escaped() {
    let gen = AggregationSqlGenerator::new(DatabaseType::SQLite);
    let sql = gen.jsonb_extract_sql("dimensions", "it's");
    // SQLite JSON paths use doubled-quote escaping (''): backslash escaping is NOT used.
    assert!(sql.contains("it''s"), "Expected doubled-quote escape in SQLite: {sql}");
}

#[test]
fn test_jsonb_sqlserver_single_quote_escaped() {
    let gen = AggregationSqlGenerator::new(DatabaseType::SQLServer);
    let sql = gen.jsonb_extract_sql("dimensions", "user'name");
    assert!(sql.contains("user''name"), "Expected doubled quote in SQL Server: {sql}");
}

// ── STRING_AGG delimiter injection tests ───────────────────────────────────

#[test]
fn test_stringagg_delimiter_single_quote_escaped() {
    let gen = AggregationSqlGenerator::new(DatabaseType::PostgreSQL);
    let sql = gen.generate_string_agg_sql("product_name", "O'Reilly", None);
    assert!(sql.contains("'O''Reilly'"), "single quote must be doubled: {sql}");
    assert!(!sql.contains("'O'Reilly'"), "unescaped quote must not appear");
}

#[test]
fn test_stringagg_delimiter_injection_payload_neutralised() {
    let gen = AggregationSqlGenerator::new(DatabaseType::PostgreSQL);
    let payload = "'; DROP TABLE users; --";
    let sql = gen.generate_string_agg_sql("product_name", payload, None);
    // After escaping, the payload single quote is doubled — no free semicolon outside a literal.
    assert!(sql.contains("''"), "single quotes must be doubled: {sql}");
    // Verify the SQL starts and ends as a valid STRING_AGG call (no injected statements).
    assert!(sql.starts_with("STRING_AGG("), "must remain a STRING_AGG call: {sql}");
}

#[test]
fn test_stringagg_delimiter_mysql_backslash_and_quote_escaped() {
    let gen = AggregationSqlGenerator::new(DatabaseType::MySQL);
    // MySQL also escapes backslashes; a trailing backslash could consume the closing quote.
    let sql = gen.generate_string_agg_sql("col", r"a\b", None);
    assert!(sql.contains(r"a\\b"), "backslash must be doubled for MySQL: {sql}");
}

#[test]
fn test_stringagg_delimiter_mysql_single_quote_escaped() {
    let gen = AggregationSqlGenerator::new(DatabaseType::MySQL);
    let sql = gen.generate_string_agg_sql("col", "O'Reilly", None);
    assert!(sql.contains("O''Reilly"), "single quote must be doubled for MySQL: {sql}");
}

#[test]
fn test_stringagg_delimiter_sqlite_single_quote_escaped() {
    let gen = AggregationSqlGenerator::new(DatabaseType::SQLite);
    let sql = gen.generate_string_agg_sql("col", "it's", None);
    assert!(sql.contains("it''s"), "single quote must be doubled for SQLite: {sql}");
}

#[test]
fn test_stringagg_delimiter_sqlserver_single_quote_escaped() {
    let gen = AggregationSqlGenerator::new(DatabaseType::SQLServer);
    let sql = gen.generate_string_agg_sql("col", "O'Reilly", None);
    assert!(sql.contains("O''Reilly"), "single quote must be doubled for SQL Server: {sql}");
}

#[test]
fn test_stringagg_delimiter_clean_value_unchanged() {
    // A safe delimiter should pass through unchanged.
    let gen = AggregationSqlGenerator::new(DatabaseType::PostgreSQL);
    let sql = gen.generate_string_agg_sql("product_name", ", ", None);
    assert_eq!(sql, "STRING_AGG(product_name, ', ')");
}

// =========================================================================
// Parameterized query generation tests
// =========================================================================

fn make_string_where_plan(_db: DatabaseType) -> AggregationPlan {
    let metadata = FactTableMetadata {
        table_name:           "tf_sales".to_string(),
        measures:             vec![],
        dimensions:           DimensionColumn {
            name:  "data".to_string(),
            paths: vec![],
        },
        denormalized_filters: vec![FilterColumn {
            name:     "status".to_string(),
            sql_type: SqlType::Timestamp,
            indexed:  true,
        }],
        calendar_dimensions:  vec![],
        partial_period:       None,
    };

    let request = AggregationRequest {
        table_name:   "tf_sales".to_string(),
        where_clause: Some(WhereClause::Field {
            path:     vec!["status".to_string()],
            operator: WhereOperator::Eq,
            value:    serde_json::json!("test_value"),
        }),
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

    crate::compiler::aggregation::AggregationPlanner::plan(request, metadata).unwrap()
}

#[test]
fn test_generate_parameterized_where_string_becomes_placeholder() {
    // PostgreSQL: string value must become $1, not an escaped literal
    let plan = make_string_where_plan(DatabaseType::PostgreSQL);
    let gen = AggregationSqlGenerator::new(DatabaseType::PostgreSQL);
    let result = gen.generate_parameterized(&plan).unwrap();

    assert!(result.sql.contains("$1"), "PostgreSQL placeholder must be $1: {}", result.sql);
    assert!(
        !result.sql.contains("'test_value'"),
        "String value must not appear as literal: {}",
        result.sql
    );
    assert_eq!(result.params.len(), 1);
    assert_eq!(result.params[0], serde_json::json!("test_value"));
}

#[test]
fn test_generate_parameterized_having_string_becomes_placeholder() {
    // MySQL: HAVING string value must become ? placeholder, not escaped inline
    let injection = "test\\' injection";
    // Build a base plan and then inject HAVING directly (like test_having_clause).
    let mut plan = create_test_plan();
    plan.having_conditions = vec![ValidatedHavingCondition {
        aggregate: AggregateExpression::MeasureAggregate {
            column:   "revenue".to_string(),
            function: AggregateFunction::Sum,
            alias:    "revenue_sum".to_string(),
        },
        operator:  HavingOperator::Eq,
        value:     serde_json::json!(injection),
    }];

    let gen = AggregationSqlGenerator::new(DatabaseType::MySQL);
    let result = gen.generate_parameterized(&plan).unwrap();

    assert!(
        result.sql.contains("HAVING SUM(revenue) = ?"),
        "SQL must use ? placeholder: {}",
        result.sql
    );
    assert_eq!(result.params.len(), 1);
    assert_eq!(result.params[0], serde_json::json!(injection));
    // injection string must NOT appear verbatim in the SQL
    assert!(
        !result.sql.contains("injection"),
        "Injection string must not appear in SQL: {}",
        result.sql
    );
}

#[test]
fn test_parameterized_postgres_placeholder_numbering() {
    // WHERE uses $1, HAVING uses $2 (shared counter).
    // Build a plan with a WHERE clause on a denormalized filter field,
    // then inject a HAVING condition directly (like test_having_clause).
    let injection = "risky";
    let metadata = FactTableMetadata {
        table_name:           "tf_sales".to_string(),
        measures:             vec![MeasureColumn {
            name:     "revenue".to_string(),
            sql_type: SqlType::Decimal,
            nullable: false,
        }],
        dimensions:           DimensionColumn {
            name:  "dimensions".to_string(),
            paths: vec![],
        },
        denormalized_filters: vec![
            FilterColumn {
                name:     "occurred_at".to_string(),
                sql_type: SqlType::Timestamp,
                indexed:  true,
            },
            FilterColumn {
                name:     "channel".to_string(),
                sql_type: SqlType::Timestamp,
                indexed:  true,
            },
        ],
        calendar_dimensions:  vec![],
        partial_period:       None,
    };

    let request = AggregationRequest {
        table_name:   "tf_sales".to_string(),
        where_clause: Some(WhereClause::Field {
            path:     vec!["channel".to_string()],
            operator: WhereOperator::Eq,
            value:    serde_json::json!(injection),
        }),
        group_by:     vec![GroupBySelection::TemporalBucket {
            column: "occurred_at".to_string(),
            bucket: TemporalBucket::Day,
            alias:  "day".to_string(),
        }],
        aggregates:   vec![AggregateSelection::MeasureAggregate {
            measure:  "revenue".to_string(),
            function: AggregateFunction::Sum,
            alias:    "total".to_string(),
        }],
        having:       vec![],
        order_by:     vec![],
        limit:        None,
        offset:       None,
    };

    let mut plan =
        crate::compiler::aggregation::AggregationPlanner::plan(request, metadata).unwrap();
    // Inject HAVING directly to avoid navigating the unvalidated HavingCondition type.
    plan.having_conditions = vec![ValidatedHavingCondition {
        aggregate: AggregateExpression::MeasureAggregate {
            column:   "revenue".to_string(),
            function: AggregateFunction::Sum,
            alias:    "total".to_string(),
        },
        operator:  HavingOperator::Gt,
        value:     serde_json::json!("threshold"),
    }];

    let gen = AggregationSqlGenerator::new(DatabaseType::PostgreSQL);
    let result = gen.generate_parameterized(&plan).unwrap();

    assert!(result.sql.contains("WHERE channel = $1"), "SQL: {}", result.sql);
    assert!(result.sql.contains("HAVING SUM(revenue) > $2"), "SQL: {}", result.sql);
    assert_eq!(result.params.len(), 2);
    assert_eq!(result.params[0], serde_json::json!(injection));
    assert_eq!(result.params[1], serde_json::json!("threshold"));
}

#[test]
fn test_parameterized_mysql_uses_question_mark() {
    let plan = make_string_where_plan(DatabaseType::MySQL);
    let gen = AggregationSqlGenerator::new(DatabaseType::MySQL);
    let result = gen.generate_parameterized(&plan).unwrap();

    assert!(result.sql.contains("WHERE status = ?"), "SQL: {}", result.sql);
    assert_eq!(result.params.len(), 1);
    assert_eq!(result.params[0], serde_json::json!("test_value"));
}

#[test]
fn test_parameterized_sqlserver_uses_at_p_placeholder() {
    let plan = make_string_where_plan(DatabaseType::SQLServer);
    let gen = AggregationSqlGenerator::new(DatabaseType::SQLServer);
    let result = gen.generate_parameterized(&plan).unwrap();

    assert!(result.sql.contains("WHERE status = @P1"), "SQL: {}", result.sql);
    assert_eq!(result.params.len(), 1);
    assert_eq!(result.params[0], serde_json::json!("test_value"));
}

#[test]
fn test_parameterized_in_array_expands_to_multiple_placeholders() {
    // WHERE status IN ("a","b","c") → WHERE status IN ($1,$2,$3) with 3 params
    let metadata = FactTableMetadata {
        table_name:           "tf_sales".to_string(),
        measures:             vec![],
        dimensions:           DimensionColumn {
            name:  "data".to_string(),
            paths: vec![],
        },
        denormalized_filters: vec![FilterColumn {
            name:     "status".to_string(),
            sql_type: SqlType::Timestamp,
            indexed:  true,
        }],
        calendar_dimensions:  vec![],
        partial_period:       None,
    };
    let request = AggregationRequest {
        table_name:   "tf_sales".to_string(),
        where_clause: Some(WhereClause::Field {
            path:     vec!["status".to_string()],
            operator: WhereOperator::In,
            value:    serde_json::json!(["a", "b", "c"]),
        }),
        group_by:     vec![],
        aggregates:   vec![AggregateSelection::Count {
            alias: "count".to_string(),
        }],
        having:       vec![],
        order_by:     vec![],
        limit:        None,
        offset:       None,
    };
    let plan = crate::compiler::aggregation::AggregationPlanner::plan(request, metadata).unwrap();
    let gen = AggregationSqlGenerator::new(DatabaseType::PostgreSQL);
    let result = gen.generate_parameterized(&plan).unwrap();

    assert!(
        result.sql.contains("status IN ($1, $2, $3)"),
        "IN clause must expand to 3 placeholders: {}",
        result.sql
    );
    assert_eq!(result.params.len(), 3);
    assert_eq!(result.params[0], serde_json::json!("a"));
    assert_eq!(result.params[1], serde_json::json!("b"));
    assert_eq!(result.params[2], serde_json::json!("c"));
}

// =============================================================================
// Partial-period UNION ALL builder
// =============================================================================

mod partial_period_builder_tests {
    use chrono::NaiveDate;
    use serde_json::json;

    use super::*;
    use crate::{
        compiler::{
            aggregate_types::AggregateFunction,
            aggregation::{AggregateSelection, AggregationRequest, GroupBySelection},
            fact_table::{
                DimensionColumn, FactTableMetadata, FilterColumn, MeasureColumn,
                PartialPeriodConfig, SqlType, TemporalGrain,
            },
        },
        runtime::partial_period::BranchPlan,
    };

    fn date(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    fn test_metadata() -> FactTableMetadata {
        FactTableMetadata {
            table_name:           "v_events_month".to_string(),
            measures:             vec![MeasureColumn {
                name:     "volume".to_string(),
                sql_type: SqlType::BigInt,
                nullable: false,
            }],
            dimensions:           DimensionColumn {
                name:  "data".to_string(),
                paths: vec![],
            },
            denormalized_filters: vec![
                FilterColumn {
                    name:     "tenant_id".to_string(),
                    sql_type: SqlType::BigInt,
                    indexed:  true,
                },
                FilterColumn {
                    name:     "period_start".to_string(),
                    sql_type: SqlType::Date,
                    indexed:  true,
                },
            ],
            calendar_dimensions:  vec![],
            partial_period:       Some(PartialPeriodConfig {
                fine_grain_view:   "v_events_day".to_string(),
                time_grain_column: "period_start".to_string(),
                time_grain_trunc:  TemporalGrain::Month,
            }),
        }
    }

    fn test_config() -> PartialPeriodConfig {
        PartialPeriodConfig {
            fine_grain_view:   "v_events_day".to_string(),
            time_grain_column: "period_start".to_string(),
            time_grain_trunc:  TemporalGrain::Month,
        }
    }

    fn test_plan() -> AggregationPlan {
        let metadata = test_metadata();
        let request = AggregationRequest {
            table_name:   "v_events_month".to_string(),
            where_clause: None,
            group_by:     vec![
                GroupBySelection::Dimension {
                    path:  "category".to_string(),
                    alias: "category".to_string(),
                },
                GroupBySelection::TemporalBucket {
                    column: "period_start".to_string(),
                    bucket: TemporalBucket::Month,
                    alias:  "period_start".to_string(),
                },
            ],
            aggregates:   vec![
                AggregateSelection::Count {
                    alias: "count".to_string(),
                },
                AggregateSelection::MeasureAggregate {
                    measure:  "volume".to_string(),
                    function: AggregateFunction::Sum,
                    alias:    "volume_sum".to_string(),
                },
            ],
            having:       vec![],
            order_by:     vec![],
            limit:        None,
            offset:       None,
        };

        crate::compiler::aggregation::AggregationPlanner::plan(request, metadata).unwrap()
    }

    #[test]
    fn test_fine_grain_branch_uses_date_trunc() {
        let plan = test_plan();
        let config = test_config();
        let gen = AggregationSqlGenerator::new(DatabaseType::PostgreSQL);

        let branch_plan = BranchPlan {
            partial_leading: None,
            complete_middle: None,
            current_period:  (date(2024, 3, 1), date(2024, 3, 21)),
        };

        let result = gen
            .generate_partial_period(&plan, &config, &branch_plan, None)
            .unwrap();

        // Fine-grain branch uses fine-grain view
        assert!(
            result.sql.contains("FROM v_events_day"),
            "expected fine-grain view: {}",
            result.sql
        );
        // Fine-grain branch applies DATE_TRUNC for re-aggregation
        assert!(
            result.sql.contains("DATE_TRUNC('month', period_start)"),
            "expected DATE_TRUNC: {}",
            result.sql
        );
        // Date range parameterized
        assert!(
            result.sql.contains("\"period_start\" >= $1"),
            "expected parameterized gte: {}",
            result.sql
        );
        assert!(
            result.sql.contains("\"period_start\" < $2"),
            "expected parameterized lt: {}",
            result.sql
        );
        assert_eq!(result.params.len(), 2);
        assert_eq!(result.params[0], json!("2024-03-01"));
        assert_eq!(result.params[1], json!("2024-03-21"));
    }

    #[test]
    fn test_coarse_branch_no_date_trunc() {
        let plan = test_plan();
        let config = test_config();
        let gen = AggregationSqlGenerator::new(DatabaseType::PostgreSQL);

        let branch_plan = BranchPlan {
            partial_leading: None,
            complete_middle: Some((date(2024, 2, 1), date(2024, 3, 1))),
            current_period:  (date(2024, 3, 1), date(2024, 3, 21)),
        };

        let result = gen
            .generate_partial_period(&plan, &config, &branch_plan, None)
            .unwrap();

        // UNION ALL present
        assert!(
            result.sql.contains("UNION ALL"),
            "expected UNION ALL: {}",
            result.sql
        );
        // Coarse branch uses original view
        assert!(
            result.sql.contains("FROM v_events_month"),
            "expected coarse view: {}",
            result.sql
        );
        // Coarse branch does NOT apply DATE_TRUNC — uses the column directly
        // (the coarse branch should have "period_start" in GROUP BY, not DATE_TRUNC)
        let branches: Vec<&str> = result.sql.split("UNION ALL").collect();
        assert_eq!(branches.len(), 2, "expected 2 branches");
        let coarse_branch = branches[0];
        assert!(
            !coarse_branch.contains("DATE_TRUNC"),
            "coarse branch should NOT have DATE_TRUNC: {}",
            coarse_branch
        );
    }

    #[test]
    fn test_three_branch_parameter_numbering() {
        let plan = test_plan();
        let config = test_config();
        let gen = AggregationSqlGenerator::new(DatabaseType::PostgreSQL);

        let branch_plan = BranchPlan {
            partial_leading: Some((date(2024, 1, 15), date(2024, 2, 1))),
            complete_middle: Some((date(2024, 2, 1), date(2024, 3, 1))),
            current_period:  (date(2024, 3, 1), date(2024, 3, 21)),
        };

        let result = gen
            .generate_partial_period(&plan, &config, &branch_plan, None)
            .unwrap();

        // 3 branches × 2 date params each = 6 params
        assert_eq!(
            result.params.len(),
            6,
            "expected 6 params, got {}",
            result.params.len()
        );

        // Branch 1: $1, $2
        assert!(result.sql.contains("$1"), "missing $1: {}", result.sql);
        assert!(result.sql.contains("$2"), "missing $2: {}", result.sql);
        // Branch 2: $3, $4
        assert!(result.sql.contains("$3"), "missing $3: {}", result.sql);
        assert!(result.sql.contains("$4"), "missing $4: {}", result.sql);
        // Branch 3: $5, $6
        assert!(result.sql.contains("$5"), "missing $5: {}", result.sql);
        assert!(result.sql.contains("$6"), "missing $6: {}", result.sql);

        // Param values in order
        assert_eq!(result.params[0], json!("2024-01-15"));
        assert_eq!(result.params[1], json!("2024-02-01"));
        assert_eq!(result.params[2], json!("2024-02-01"));
        assert_eq!(result.params[3], json!("2024-03-01"));
        assert_eq!(result.params[4], json!("2024-03-01"));
        assert_eq!(result.params[5], json!("2024-03-21"));
    }

    #[test]
    fn test_extra_where_passed_to_all_branches() {
        let plan = test_plan();
        let config = test_config();
        let gen = AggregationSqlGenerator::new(DatabaseType::PostgreSQL);

        let branch_plan = BranchPlan {
            partial_leading: Some((date(2024, 1, 15), date(2024, 2, 1))),
            complete_middle: None,
            current_period:  (date(2024, 3, 1), date(2024, 3, 21)),
        };

        let extra = WhereClause::NativeField {
            column:   "tenant_id".into(),
            pg_cast:  "int8".into(),
            operator: WhereOperator::Eq,
            value:    json!(42),
        };

        let result = gen
            .generate_partial_period(&plan, &config, &branch_plan, Some(&extra))
            .unwrap();

        // Two branches, each should have the tenant_id condition
        let branches: Vec<&str> = result.sql.split("UNION ALL").collect();
        assert_eq!(branches.len(), 2);
        for (i, branch) in branches.iter().enumerate() {
            assert!(
                branch.contains("\"tenant_id\""),
                "branch {} missing tenant_id filter: {}",
                i + 1,
                branch
            );
        }

        // 2 branches × (2 date params + 1 tenant_id param) = 6 total
        assert_eq!(
            result.params.len(),
            6,
            "expected 6 params: {:?}",
            result.params
        );
    }

    #[test]
    fn test_two_branch_aligned_lower_bound() {
        let plan = test_plan();
        let config = test_config();
        let gen = AggregationSqlGenerator::new(DatabaseType::PostgreSQL);

        // Aligned lower bound: no B1
        let branch_plan = BranchPlan {
            partial_leading: None,
            complete_middle: Some((date(2024, 2, 1), date(2024, 3, 1))),
            current_period:  (date(2024, 3, 1), date(2024, 3, 21)),
        };

        let result = gen
            .generate_partial_period(&plan, &config, &branch_plan, None)
            .unwrap();

        assert_eq!(
            result.sql.split("UNION ALL").count(),
            2,
            "expected 2 branches for aligned lower bound"
        );
        assert_eq!(result.params.len(), 4);
    }

    #[test]
    fn test_single_branch_current_only() {
        let plan = test_plan();
        let config = test_config();
        let gen = AggregationSqlGenerator::new(DatabaseType::PostgreSQL);

        // Single branch: only current period
        let branch_plan = BranchPlan {
            partial_leading: None,
            complete_middle: None,
            current_period:  (date(2024, 3, 1), date(2024, 3, 21)),
        };

        let result = gen
            .generate_partial_period(&plan, &config, &branch_plan, None)
            .unwrap();

        assert!(
            !result.sql.contains("UNION ALL"),
            "single branch should not have UNION ALL: {}",
            result.sql
        );
        assert_eq!(result.params.len(), 2);
    }

    #[test]
    fn test_aggregate_expressions_in_all_branches() {
        let plan = test_plan();
        let config = test_config();
        let gen = AggregationSqlGenerator::new(DatabaseType::PostgreSQL);

        let branch_plan = BranchPlan {
            partial_leading: Some((date(2024, 1, 15), date(2024, 2, 1))),
            complete_middle: Some((date(2024, 2, 1), date(2024, 3, 1))),
            current_period:  (date(2024, 3, 1), date(2024, 3, 21)),
        };

        let result = gen
            .generate_partial_period(&plan, &config, &branch_plan, None)
            .unwrap();

        let branches: Vec<&str> = result.sql.split("UNION ALL").collect();
        assert_eq!(branches.len(), 3);

        for (i, branch) in branches.iter().enumerate() {
            assert!(
                branch.contains("COUNT(*)"),
                "branch {} missing COUNT(*): {}",
                i + 1,
                branch
            );
            assert!(
                branch.contains("SUM(volume)"),
                "branch {} missing SUM(volume): {}",
                i + 1,
                branch
            );
            assert!(
                branch.contains("GROUP BY"),
                "branch {} missing GROUP BY: {}",
                i + 1,
                branch
            );
        }
    }
}
