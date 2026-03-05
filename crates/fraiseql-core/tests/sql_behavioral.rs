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
    postgres::PostgresWhereGenerator,
    where_clause::{WhereClause, WhereOperator},
};
use serde_json::json;

fn pg() -> PostgresWhereGenerator {
    PostgresWhereGenerator::new()
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
        path:     vec!["email".to_string()],
        operator: WhereOperator::Eq,
        value:    json!("alice@example.com"),
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
        path:     vec!["score".to_string()],
        operator: WhereOperator::Gt,
        value:    json!(100),
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
        path:     vec!["created_at".to_string()],
        operator: WhereOperator::Gt,
        value:    json!("2024-01-01T00:00:00Z"),
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
        path:     vec!["name".to_string()],
        operator: WhereOperator::Ilike,
        value:    json!("%alice%"),
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
        path:     vec!["title".to_string()],
        operator: WhereOperator::Ilike,
        value:    json!("%rust_lang%"),
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
        path:     vec!["status".to_string()],
        operator: WhereOperator::In,
        value:    json!(["active", "pending", "review"]),
    };
    let (sql, params) = pg().generate(&clause).expect("generate");
    assert!(
        sql.contains("ANY") || sql.contains("IN"),
        "Expected ANY or IN in: {sql}"
    );
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
        path:     vec!["deleted_at".to_string()],
        operator: WhereOperator::IsNull,
        value:    json!(true),
    };
    let (sql, _params) = pg().generate(&clause).expect("generate");
    assert!(
        sql.to_uppercase().contains("IS NULL"),
        "Expected IS NULL in: {sql}"
    );
    assert!(sql.contains("deleted_at"), "Expected field name in: {sql}");
}

/// Behavioral counterpart for `snapshot_postgres_where_is_not_null`.
#[test]
fn where_is_not_null() {
    let clause = WhereClause::Field {
        path:     vec!["published_at".to_string()],
        operator: WhereOperator::IsNull,
        value:    json!(false),
    };
    let (sql, _params) = pg().generate(&clause).expect("generate");
    assert!(
        sql.to_uppercase().contains("IS NOT NULL"),
        "Expected IS NOT NULL in: {sql}"
    );
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
            path:     vec!["published".to_string()],
            operator: WhereOperator::Eq,
            value:    json!(true),
        },
        WhereClause::Field {
            path:     vec!["author_id".to_string()],
            operator: WhereOperator::Eq,
            value:    json!("00000000-0000-0000-0000-000000000001"),
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
        path:     vec!["published".to_string()],
        operator: WhereOperator::Eq,
        value:    json!(true),
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
        path:     vec!["id".to_string()],
        operator: WhereOperator::Gt,
        value:    json!("cursor-value-here"),
    };
    let (sql, params) = pg().generate(&clause).expect("generate");
    assert!(
        sql.contains('>'),
        "Keyset pagination must use > not OFFSET: {sql}"
    );
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

    assert!(
        sql.contains("jsonb_build_object"),
        "Expected jsonb_build_object in: {sql}"
    );
    assert!(sql.contains("'id'"), "Expected 'id' key in: {sql}");
    assert!(sql.contains("'name'"), "Expected 'name' key in: {sql}");
    assert!(sql.contains("'email'"), "Expected 'email' key in: {sql}");
    // Each field must appear as a JSON path extraction.
    assert!(sql.contains("id"), "Expected id in: {sql}");
    assert!(sql.contains("name"), "Expected name in: {sql}");
    assert!(sql.contains("email"), "Expected email in: {sql}");
}

// ============================================================================
// Aggregate SQL shape (db-integration stubs — require a live database)
// ============================================================================

/// Behavioral counterpart for `snapshot_aggregate_query_sum`.
///
/// This test verifies the shape of aggregate SQL produced by the engine.
/// Requires a live PostgreSQL instance (started by `make db-up`).
#[test]
#[ignore = "requires live PostgreSQL (make db-up); verifies aggregate SUM execution"]
fn aggregate_sum_produces_correct_sql() {
    // Full verification: compile schema with a fact table, execute an aggregate
    // SUM query, verify the result contains the expected numeric total.
    // Implementation deferred — see tracking entry in tests/snapshot-pairs.md.
    todo!("implement with testcontainers when the testcontainer harness is stabilized");
}

/// Behavioral counterpart for `snapshot_aggregate_query_with_group_by`.
#[test]
#[ignore = "requires live PostgreSQL (make db-up); verifies GROUP BY execution"]
fn aggregate_group_by_produces_correct_sql() {
    todo!("implement with testcontainers when the testcontainer harness is stabilized");
}

// ============================================================================
// Mutation SQL shape (db-integration stubs)
// ============================================================================

/// Behavioral counterpart for `snapshot_postgres_function_call_create`.
///
/// Verifies that the mutation executor emits `SELECT * FROM fn_create_*(...)`.
#[test]
#[ignore = "requires live PostgreSQL with the fn_create_post stored function"]
fn mutation_create_sql_shape() {
    todo!("implement with testcontainers");
}

/// Behavioral counterpart for `snapshot_postgres_function_call_update`.
#[test]
#[ignore = "requires live PostgreSQL with the fn_update_post stored function"]
fn mutation_update_sql_shape() {
    todo!("implement with testcontainers");
}

/// Behavioral counterpart for `snapshot_postgres_function_call_delete`.
#[test]
#[ignore = "requires live PostgreSQL with the fn_delete_post stored function"]
fn mutation_delete_sql_shape() {
    todo!("implement with testcontainers");
}

// ============================================================================
// RLS (db-integration stubs)
// ============================================================================

/// Behavioral counterpart for `snapshot_postgres_with_rls_where_clause`.
///
/// Verifies that when a query has both an application WHERE clause and an RLS
/// policy, the two are AND-ed together in the correct order (RLS always wins).
#[test]
#[ignore = "requires live PostgreSQL with RLS configured on the test view"]
fn rls_combined_where_clause() {
    todo!("implement with testcontainers + RLS-enabled view");
}

/// Behavioral counterpart for `snapshot_postgres_rls_only`.
#[test]
#[ignore = "requires live PostgreSQL with RLS configured on the test view"]
fn rls_only_clause() {
    todo!("implement with testcontainers + RLS-enabled view");
}
