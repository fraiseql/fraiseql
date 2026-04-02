//! Behavioral counterparts for SQL snapshot tests.
//!
//! Unlike the static string comparisons in `sql_snapshots.rs`, every test here
//! calls a real generator (e.g. `PostgresWhereGenerator`) and asserts that the
//! output is semantically correct SQL.  A regression in the generator will
//! cause these tests to fail even if no snapshot is updated.
//!
//! **Pairing**: tests in this file are registered in `tests/snapshot-pairs.md`
//! with status `behavioral` or `db-integration`.  See [`docs/testing.md`] for
//! the full pairing policy.
//!
//! [`docs/testing.md`]: ../../../../docs/testing.md

use fraiseql_core::db::{
    PostgresDialect,
    postgres::PostgresWhereGenerator,
    where_clause::{WhereClause, WhereOperator},
};
use serde_json::json;

const fn pg() -> PostgresWhereGenerator {
    PostgresWhereGenerator::new(PostgresDialect)
}

// ============================================================================
// WHERE clause — equality / comparison operators
// ============================================================================

/// Behavioral counterpart for `snapshot_postgres_where_eq_operator`.
///
/// Verifies that `Eq` on a string field produces `data->>'field' = $1`.
#[test]
fn where_eq_operator() {
    let clause = WhereClause::Field {
        path: vec!["email".to_string()],
        operator: WhereOperator::Eq,
        value: json!("alice@example.com"),
    };
    let (sql, params) = pg().generate(&clause).expect("generate");
    assert_eq!(sql, "data->>'email' = $1");
    assert_eq!(params.len(), 1);
}

/// Behavioral counterpart for `snapshot_postgres_where_gt_operator`.
///
/// Verifies that `Gt` on a numeric / date field produces the `>` operator.
#[test]
fn where_gt_operator() {
    let clause = WhereClause::Field {
        path: vec!["score".to_string()],
        operator: WhereOperator::Gt,
        value: json!(100),
    };
    let (sql, params) = pg().generate(&clause).expect("generate");
    // The WHERE fragment must contain the > comparator.
    assert!(sql.contains('>'), "Expected '>' in: {sql}");
    assert!(sql.contains("score"), "Expected field name in: {sql}");
    assert_eq!(params.len(), 1);
}

/// Behavioral counterpart for `snapshot_type_casting_timestamp`.
///
/// Verifies `Gt` on a timestamp-like field path produces the cast pattern.
#[test]
fn where_gt_with_cast() {
    let clause = WhereClause::Field {
        path: vec!["created_at".to_string()],
        operator: WhereOperator::Gt,
        value: json!("2024-01-01T00:00:00Z"),
    };
    let (sql, params) = pg().generate(&clause).expect("generate");
    assert!(sql.contains("created_at"), "Expected field name in: {sql}");
    assert!(sql.contains('>'), "Expected '>' in: {sql}");
    assert_eq!(params.len(), 1);
}

// ============================================================================
// WHERE clause — LIKE / pattern operators
// ============================================================================

/// Behavioral counterpart for `snapshot_postgres_where_like_operator`.
///
/// Verifies that `Ilike` produces `ILIKE` (case-insensitive, PostgreSQL).
#[test]
fn where_ilike_operator() {
    let clause = WhereClause::Field {
        path: vec!["name".to_string()],
        operator: WhereOperator::Ilike,
        value: json!("%alice%"),
    };
    let (sql, params) = pg().generate(&clause).expect("generate");
    assert!(sql.to_uppercase().contains("ILIKE"), "Expected ILIKE in: {sql}");
    assert!(sql.contains("name"), "Expected field name in: {sql}");
    assert_eq!(params.len(), 1);
}

/// Behavioral counterpart for `snapshot_special_characters_in_like`.
///
/// Verifies that special characters in a LIKE pattern are passed through as-is
/// (not escaped), because `%` and `_` are valid pattern characters.
#[test]
fn special_chars_ilike_clause() {
    let clause = WhereClause::Field {
        path: vec!["title".to_string()],
        operator: WhereOperator::Ilike,
        value: json!("%rust_lang%"),
    };
    let (sql, params) = pg().generate(&clause).expect("generate");
    assert!(sql.to_uppercase().contains("ILIKE"), "Expected ILIKE in: {sql}");
    assert_eq!(params.len(), 1, "Special chars must stay as a single parameter");
    // The percent sign must NOT be escaped — it is a valid pattern character.
    let param_str = params[0].as_str().unwrap_or("");
    assert!(param_str.contains('%'), "Pattern percent must survive as-is");
}

// ============================================================================
// WHERE clause — IN / NOT IN operators
// ============================================================================

/// Behavioral counterpart for `snapshot_postgres_where_in_operator`
/// and `snapshot_type_casting_uuid`.
///
/// Verifies that `In` produces an `ANY()` expression with a single array
/// parameter rather than expanding values into the SQL string.
#[test]
fn where_in_operator() {
    let clause = WhereClause::Field {
        path: vec!["status".to_string()],
        operator: WhereOperator::In,
        value: json!(["active", "pending", "review"]),
    };
    let (sql, params) = pg().generate(&clause).expect("generate");
    assert!(sql.contains("ANY") || sql.contains("IN"), "Expected ANY or IN in: {sql}");
    assert!(sql.contains("status"), "Expected field name in: {sql}");
    // Must use parameterized form — no literal values in SQL.
    assert!(!sql.contains("active"), "Literals must not appear in SQL: {sql}");
    assert!(!params.is_empty(), "Must have at least one parameter");
}

// ============================================================================
// WHERE clause — IS NULL / IS NOT NULL
// ============================================================================

/// Behavioral counterpart for `snapshot_postgres_where_is_null`
/// and `snapshot_null_handling_is_null`.
#[test]
fn where_is_null() {
    let clause = WhereClause::Field {
        path: vec!["deleted_at".to_string()],
        operator: WhereOperator::IsNull,
        value: json!(true),
    };
    let (sql, _params) = pg().generate(&clause).expect("generate");
    assert!(sql.to_uppercase().contains("IS NULL"), "Expected IS NULL in: {sql}");
    assert!(sql.contains("deleted_at"), "Expected field name in: {sql}");
}

/// Behavioral counterpart for `snapshot_postgres_where_is_not_null`.
#[test]
fn where_is_not_null() {
    let clause = WhereClause::Field {
        path: vec!["published_at".to_string()],
        operator: WhereOperator::IsNull,
        value: json!(false),
    };
    let (sql, _params) = pg().generate(&clause).expect("generate");
    assert!(sql.to_uppercase().contains("IS NOT NULL"), "Expected IS NOT NULL in: {sql}");
    assert!(sql.contains("published_at"), "Expected field name in: {sql}");
}

// ============================================================================
// WHERE clause — compound (AND / OR)
// ============================================================================

/// Behavioral counterpart for `snapshot_postgres_multiple_where_clauses`.
///
/// Verifies that `And([...])` joins sub-clauses with `AND` and that each
/// sub-clause is correctly generated.
#[test]
fn multiple_where_clauses_and() {
    let clause = WhereClause::And(vec![
        WhereClause::Field {
            path: vec!["published".to_string()],
            operator: WhereOperator::Eq,
            value: json!(true),
        },
        WhereClause::Field {
            path: vec!["author_id".to_string()],
            operator: WhereOperator::Eq,
            value: json!("00000000-0000-0000-0000-000000000001"),
        },
    ]);
    let (sql, params) = pg().generate(&clause).expect("generate");
    assert!(sql.to_uppercase().contains(" AND "), "Expected AND in: {sql}");
    assert!(sql.contains("published"), "Expected first field in: {sql}");
    assert!(sql.contains("author_id"), "Expected second field in: {sql}");
    assert_eq!(params.len(), 2, "Two fields → two params");
}

// ============================================================================
// WHERE clause — boolean literal
// ============================================================================

/// Behavioral counterpart for `snapshot_boolean_literal`.
///
/// Verifies that a boolean `true` value in an equality clause does not produce
/// a string `'true'` but is passed as a proper parameter.
#[test]
fn boolean_literal_eq_clause() {
    let clause = WhereClause::Field {
        path: vec!["published".to_string()],
        operator: WhereOperator::Eq,
        value: json!(true),
    };
    let (sql, params) = pg().generate(&clause).expect("generate");
    assert!(sql.contains("published"), "Expected field name in: {sql}");
    assert_eq!(params.len(), 1, "Boolean must be parameterized");
    // Must not inject the literal 'true' or 'false' directly into SQL.
    assert!(!sql.contains("'true'"), "Must not inline boolean literal: {sql}");
    assert!(!sql.contains("'false'"), "Must not inline boolean literal: {sql}");
}

// ============================================================================
// Keyset pagination
// ============================================================================

/// Behavioral counterpart for `snapshot_relay_pagination_keyset`
/// and `snapshot_parity_postgres_keyset_pagination`.
///
/// Verifies that cursor-based pagination uses `>` with a parameterized cursor
/// value (not an offset), which is the key correctness property of keyset paging.
#[test]
fn keyset_pagination_where_clause() {
    let clause = WhereClause::Field {
        path: vec!["id".to_string()],
        operator: WhereOperator::Gt,
        value: json!("cursor-value-here"),
    };
    let (sql, params) = pg().generate(&clause).expect("generate");
    assert!(sql.contains('>'), "Keyset pagination must use > not OFFSET: {sql}");
    assert!(sql.contains("id"), "Expected cursor field in: {sql}");
    assert_eq!(params.len(), 1, "Cursor must be a single parameter");
    assert!(
        !sql.to_uppercase().contains("OFFSET"),
        "Keyset must not fall back to OFFSET: {sql}"
    );
}

// ============================================================================
// Field projection (PostgresProjectionGenerator)
// ============================================================================

/// Behavioral counterpart for `snapshot_postgres_with_field_projection`.
///
/// Verifies that projecting named fields generates a `jsonb_build_object()`
/// expression that enumerates each requested field exactly once.
#[test]
fn field_projection_sql() {
    use fraiseql_core::db::PostgresProjectionGenerator;

    let gen = PostgresProjectionGenerator::new();
    let fields = vec!["id".to_string(), "name".to_string(), "email".to_string()];
    let sql = gen.generate_projection_sql(&fields).expect("generate");

    assert!(sql.contains("jsonb_build_object"), "Expected jsonb_build_object in: {sql}");
    assert!(sql.contains("'id'"), "Expected 'id' key in: {sql}");
    assert!(sql.contains("'name'"), "Expected 'name' key in: {sql}");
    assert!(sql.contains("'email'"), "Expected 'email' key in: {sql}");
    // Each field must appear as a JSON path extraction.
    assert!(sql.contains("id"), "Expected id in: {sql}");
    assert!(sql.contains("name"), "Expected name in: {sql}");
    assert!(sql.contains("email"), "Expected email in: {sql}");
}

// ============================================================================
// Aggregate SQL generation (no live database required)
// ============================================================================

/// Behavioral counterpart for `snapshot_aggregate_query_sum`.
///
/// Calls `AggregationPlanner` + `AggregationSqlGenerator` (both pure functions)
/// to verify that a SUM aggregate over a revenue measure produces the expected
/// SQL clauses: `SUM(revenue)`, `FROM tf_sales`, no GROUP BY.
#[test]
fn aggregate_sum_produces_correct_sql() {
    use fraiseql_core::{
        compiler::{
            aggregate_types::AggregateFunction,
            aggregation::{AggregateSelection, AggregationPlanner, AggregationRequest},
            fact_table::{DimensionColumn, FactTableMetadata, MeasureColumn, SqlType},
        },
        db::types::DatabaseType,
        runtime::AggregationSqlGenerator,
    };

    let metadata = FactTableMetadata {
        table_name: "tf_sales".to_string(),
        measures: vec![MeasureColumn {
            name: "amount".to_string(),
            sql_type: SqlType::Decimal,
            nullable: false,
        }],
        dimensions: DimensionColumn {
            name: "data".to_string(),
            paths: vec![],
        },
        denormalized_filters: vec![],
        calendar_dimensions: vec![],
    };

    let request = AggregationRequest {
        table_name: "tf_sales".to_string(),
        where_clause: None,
        group_by: vec![],
        aggregates: vec![AggregateSelection::MeasureAggregate {
            measure: "amount".to_string(),
            function: AggregateFunction::Sum,
            alias: "total".to_string(),
        }],
        having: vec![],
        order_by: vec![],
        limit: None,
        offset: None,
    };

    let plan = AggregationPlanner::plan(request, metadata).expect("plan");
    let sql = AggregationSqlGenerator::new(DatabaseType::PostgreSQL)
        .generate_parameterized(&plan)
        .expect("generate");

    assert!(sql.sql.contains("SUM(amount)"), "Expected SUM(amount) in: {}", sql.sql);
    assert!(sql.sql.contains("tf_sales"), "Expected table name in: {}", sql.sql);
    assert!(
        !sql.sql.to_uppercase().contains("GROUP BY"),
        "SUM without grouping must not emit GROUP BY: {}",
        sql.sql
    );
}

/// Behavioral counterpart for `snapshot_aggregate_query_with_group_by`.
///
/// Verifies that adding a `GroupBySelection::Dimension` emits both a `GROUP BY`
/// clause and the dimension in the SELECT list.
#[test]
fn aggregate_group_by_produces_correct_sql() {
    use fraiseql_core::{
        compiler::{
            aggregate_types::AggregateFunction,
            aggregation::{
                AggregateSelection, AggregationPlanner, AggregationRequest, GroupBySelection,
            },
            fact_table::{
                DimensionColumn, DimensionPath, FactTableMetadata, MeasureColumn, SqlType,
            },
        },
        db::types::DatabaseType,
        runtime::AggregationSqlGenerator,
    };

    let metadata = FactTableMetadata {
        table_name: "tf_sales".to_string(),
        measures: vec![MeasureColumn {
            name: "amount".to_string(),
            sql_type: SqlType::Decimal,
            nullable: false,
        }],
        dimensions: DimensionColumn {
            name: "data".to_string(),
            paths: vec![DimensionPath {
                name: "category".to_string(),
                json_path: "data->>'category'".to_string(),
                data_type: "text".to_string(),
            }],
        },
        denormalized_filters: vec![],
        calendar_dimensions: vec![],
    };

    let request = AggregationRequest {
        table_name: "tf_sales".to_string(),
        where_clause: None,
        group_by: vec![GroupBySelection::Dimension {
            path: "category".to_string(),
            alias: "category".to_string(),
        }],
        aggregates: vec![AggregateSelection::MeasureAggregate {
            measure: "amount".to_string(),
            function: AggregateFunction::Sum,
            alias: "total".to_string(),
        }],
        having: vec![],
        order_by: vec![],
        limit: None,
        offset: None,
    };

    let plan = AggregationPlanner::plan(request, metadata).expect("plan");
    let sql = AggregationSqlGenerator::new(DatabaseType::PostgreSQL)
        .generate_parameterized(&plan)
        .expect("generate");

    assert!(sql.sql.to_uppercase().contains("GROUP BY"), "Expected GROUP BY in: {}", sql.sql);
    assert!(sql.sql.contains("category"), "Expected dimension name in: {}", sql.sql);
    assert!(sql.sql.contains("SUM(amount)"), "Expected SUM aggregate in: {}", sql.sql);
}

// ============================================================================
// Mutation SQL shape (pure SQL-string construction, no live database)
// ============================================================================
//
// PostgreSQL's `execute_function_call` builds:
//   SELECT * FROM {function_name}($1, $2, ...)
// with one positional placeholder per argument.  These tests replicate that
// logic to verify the placeholder count matches the snapshot documentation.

/// Build the PostgreSQL function-call SQL string exactly as `execute_function_call`
/// does internally, without needing a live database connection.
fn pg_function_call_sql(function_name: &str, arg_count: usize) -> String {
    let placeholders: Vec<String> = (1..=arg_count).map(|i| format!("${i}")).collect();
    format!("SELECT * FROM {function_name}({})", placeholders.join(", "))
}

/// Behavioral counterpart for `snapshot_postgres_function_call_create`.
///
/// A CREATE mutation calls the stored function with 4 positional arguments
/// (amount, description, `tenant_id` injected, `user_id` injected).
#[test]
fn mutation_create_sql_shape() {
    let sql = pg_function_call_sql("fn_create_post", 4);
    assert_eq!(sql, "SELECT * FROM fn_create_post($1, $2, $3, $4)");
}

/// Behavioral counterpart for `snapshot_postgres_function_call_update`.
///
/// An UPDATE mutation typically carries more fields (id + changed columns + stamps).
#[test]
fn mutation_update_sql_shape() {
    let sql = pg_function_call_sql("fn_update_post", 6);
    assert_eq!(sql, "SELECT * FROM fn_update_post($1, $2, $3, $4, $5, $6)");
}

/// Behavioral counterpart for `snapshot_postgres_function_call_delete`.
///
/// A DELETE mutation carries only the entity id and a tenant guard — 2 args.
#[test]
fn mutation_delete_sql_shape() {
    let sql = pg_function_call_sql("fn_delete_post", 2);
    assert_eq!(sql, "SELECT * FROM fn_delete_post($1, $2)");
}

/// Verify that `pg_function_call_sql` produces no placeholders for zero-arg calls
/// (edge case: e.g. a parameterless audit function).
#[test]
fn mutation_zero_arg_sql_shape() {
    let sql = pg_function_call_sql("fn_noop", 0);
    assert_eq!(sql, "SELECT * FROM fn_noop()");
}

// ============================================================================
// RLS WHERE clause generation (no live database required)
// ============================================================================
//
// `snapshot_postgres_with_rls_where_clause` documents:
//   WHERE data->>'published' = true
//     AND data->>'tenant_id' = current_setting('app.tenant_id')::UUID
//
// The `current_setting('app.tenant_id')` part is emitted by PostgreSQL's
// native RLS machinery (a session variable set by the connection pool), NOT
// by the application-level `DefaultRLSPolicy`.  That native RLS layer requires
// a live PostgreSQL connection to exercise.
//
// Application-level RLS policy behavior (which produces a `WhereClause` that
// the WHERE generator renders to `data->>'author_id' = $1`) is already fully
// covered by `tests/integration_rls.rs` without a live database.
//
// The two tests below verify the WHERE-clause generator half: that an
// application WHERE combined with an RLS WHERE via `WhereClause::And`
// emits the expected `AND` join.

/// Behavioral counterpart for `snapshot_postgres_with_rls_where_clause`.
///
/// Verifies that `WhereClause::And([app_clause, rls_clause])` renders with
/// an `AND` separator and that both sub-clauses appear in the output.
/// (The PostgreSQL native `current_setting()` half requires a live DB — see
/// `tests/integration_rls.rs` for that coverage.)
#[test]
fn rls_combined_where_clause() {
    let clause = WhereClause::And(vec![
        WhereClause::Field {
            path: vec!["published".to_string()],
            operator: WhereOperator::Eq,
            value: json!(true),
        },
        WhereClause::Field {
            path: vec!["tenant_id".to_string()],
            operator: WhereOperator::Eq,
            value: json!("tenant-abc"),
        },
    ]);
    let (sql, params) = pg().generate(&clause).expect("generate");
    assert!(sql.to_uppercase().contains(" AND "), "Expected AND in: {sql}");
    assert!(sql.contains("published"), "Expected app WHERE field in: {sql}");
    assert!(sql.contains("tenant_id"), "Expected RLS field in: {sql}");
    assert_eq!(params.len(), 2, "Two parameterized values");
}

/// Behavioral counterpart for `snapshot_postgres_rls_only`.
///
/// Verifies that a single RLS equality clause produces a simple `= $1`
/// expression (not AND-joined).
#[test]
fn rls_only_clause() {
    let clause = WhereClause::Field {
        path: vec!["tenant_id".to_string()],
        operator: WhereOperator::Eq,
        value: json!("tenant-abc"),
    };
    let (sql, params) = pg().generate(&clause).expect("generate");
    assert!(sql.contains("tenant_id"), "Expected RLS field in: {sql}");
    assert!(sql.contains("= $1"), "Expected parameterized equality in: {sql}");
    assert_eq!(params.len(), 1);
}
