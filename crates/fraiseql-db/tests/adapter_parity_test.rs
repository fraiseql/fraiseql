//! WHERE-generator SQL shape tests.
//!
//! For each common WHERE operator, verifies that the PostgreSQL generator:
//! 1. Produces valid SQL (no error).
//! 2. Emits the expected operator syntax.
//! 3. Passes the correct number of bind parameters.
//!
//! These tests do NOT need a live database — they exercise SQL string
//! generation only. (This file was a cross-adapter parity suite until the
//! non-PostgreSQL adapters were removed; see `docs/database-compatibility.md`.)
//!
//! # Running
//!
//! ```bash
//! cargo test -p fraiseql-db --test adapter_parity_test
//! ```

#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use fraiseql_db::{
    PostgresDialect,
    postgres::PostgresWhereGenerator,
    where_clause::{WhereClause, WhereOperator},
};
use serde_json::json;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

const fn pg_gen() -> PostgresWhereGenerator {
    PostgresWhereGenerator::new(PostgresDialect)
}

fn field(field_name: &str, op: WhereOperator, val: serde_json::Value) -> WhereClause {
    WhereClause::Field {
        path:     vec![field_name.to_string()],
        operator: op,
        value:    val,
    }
}

fn nested_field(path: &[&str], op: WhereOperator, val: serde_json::Value) -> WhereClause {
    WhereClause::Field {
        path:     path.iter().map(|s| (*s).to_string()).collect(),
        operator: op,
        value:    val,
    }
}

// ---------------------------------------------------------------------------
// PostgreSQL-only tests (always run)
// ---------------------------------------------------------------------------

#[test]
fn pg_eq_single_field_uses_dollar_placeholder() {
    let clause = field("status", WhereOperator::Eq, json!("active"));
    let (sql, params) = pg_gen().generate(&clause).unwrap();
    assert!(sql.contains("$1"), "PG should use $1: {sql}");
    assert_eq!(params.len(), 1);
    assert_eq!(params[0], json!("active"));
}

#[test]
fn pg_neq_uses_bang_eq() {
    let clause = field("role", WhereOperator::Neq, json!("admin"));
    let (sql, _) = pg_gen().generate(&clause).unwrap();
    assert!(sql.contains("!="), "PG Neq should use '!=': {sql}");
}

#[test]
fn pg_gt_uses_greater_than() {
    let clause = field("score", WhereOperator::Gt, json!(42));
    let (sql, params) = pg_gen().generate(&clause).unwrap();
    assert!(sql.contains('>'), "PG Gt should use '>': {sql}");
    assert_eq!(params.len(), 1);
}

#[test]
fn pg_lte_uses_less_equal() {
    let clause = field("age", WhereOperator::Lte, json!(65));
    let (sql, _) = pg_gen().generate(&clause).unwrap();
    assert!(sql.contains("<="), "PG Lte should use '<=': {sql}");
}

#[test]
fn pg_icontains_uses_ilike() {
    let clause = field("email", WhereOperator::Icontains, json!("example.com"));
    let (sql, params) = pg_gen().generate(&clause).unwrap();
    assert!(sql.to_uppercase().contains("ILIKE"), "PG Icontains should use ILIKE: {sql}");
    assert_eq!(params.len(), 1);
    assert_eq!(params[0], json!("example.com"));
}

#[test]
fn pg_startswith_uses_like() {
    let clause = field("code", WhereOperator::Startswith, json!("US-"));
    let (sql, params) = pg_gen().generate(&clause).unwrap();
    assert!(sql.to_uppercase().contains("LIKE"), "PG Startswith should use LIKE: {sql}");
    assert_eq!(params.len(), 1);
}

#[test]
fn pg_endswith_uses_like() {
    let clause = field("filename", WhereOperator::Endswith, json!(".pdf"));
    let (sql, params) = pg_gen().generate(&clause).unwrap();
    assert!(sql.to_uppercase().contains("LIKE"), "PG Endswith should use LIKE: {sql}");
    assert_eq!(params.len(), 1);
}

#[test]
fn pg_isnull_true_is_null() {
    let clause = field("deleted_at", WhereOperator::IsNull, json!(true));
    let (sql, params) = pg_gen().generate(&clause).unwrap();
    assert!(sql.to_uppercase().contains("IS NULL"), "PG IsNull(true): {sql}");
    assert_eq!(params.len(), 0, "IS NULL should have no params");
}

#[test]
fn pg_isnull_false_is_not_null() {
    let clause = field("published_at", WhereOperator::IsNull, json!(false));
    let (sql, _) = pg_gen().generate(&clause).unwrap();
    assert!(sql.to_uppercase().contains("IS NOT NULL"), "PG IsNull(false): {sql}");
}

#[test]
fn pg_and_combinator_two_conditions() {
    let clause = WhereClause::And(vec![
        field("active", WhereOperator::Eq, json!(true)),
        field("role", WhereOperator::Eq, json!("user")),
    ]);
    let (sql, params) = pg_gen().generate(&clause).unwrap();
    assert!(sql.contains("AND"), "PG AND: {sql}");
    assert_eq!(params.len(), 2);
}

#[test]
fn pg_or_combinator_two_conditions() {
    let clause = WhereClause::Or(vec![
        field("status", WhereOperator::Eq, json!("pending")),
        field("status", WhereOperator::Eq, json!("processing")),
    ]);
    let (sql, params) = pg_gen().generate(&clause).unwrap();
    assert!(sql.contains("OR"), "PG OR: {sql}");
    assert_eq!(params.len(), 2);
}

#[test]
fn pg_empty_and_produces_sentinel() {
    let clause = WhereClause::And(vec![]);
    let (sql, params) = pg_gen().generate(&clause).unwrap();
    assert!(!sql.is_empty());
    assert_eq!(params.len(), 0);
}

#[test]
fn pg_empty_or_produces_sentinel() {
    let clause = WhereClause::Or(vec![]);
    let (sql, params) = pg_gen().generate(&clause).unwrap();
    assert!(!sql.is_empty());
    assert_eq!(params.len(), 0);
}

#[test]
fn pg_nested_path_references_path_segments() {
    let clause = nested_field(&["address", "city"], WhereOperator::Eq, json!("Paris"));
    let (sql, params) = pg_gen().generate(&clause).unwrap();
    assert_eq!(params.len(), 1);
    assert!(sql.contains("address"), "PG nested: should reference 'address': {sql}");
}

#[test]
fn pg_nested_and_or_three_params() {
    let clause = WhereClause::And(vec![
        WhereClause::Or(vec![
            field("tag", WhereOperator::Eq, json!("rust")),
            field("tag", WhereOperator::Eq, json!("go")),
        ]),
        field("published", WhereOperator::Eq, json!(true)),
    ]);
    let (_, params) = pg_gen().generate(&clause).unwrap();
    assert_eq!(params.len(), 3);
}

#[test]
fn pg_contains_param_count() {
    let clause = field("name", WhereOperator::Contains, json!("alice"));
    let (_, params) = pg_gen().generate(&clause).unwrap();
    assert_eq!(params.len(), 1);
    assert_eq!(params[0], json!("alice"));
}

// ---------------------------------------------------------------------------
// ---------------------------------------------------------------------------
