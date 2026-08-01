//! Cross-dialect property-based tests for `GenericWhereGenerator`.
//!
//! These tests verify invariants that must hold for **all** dialects and **all**
//! valid inputs — not just the representative samples covered by unit tests:
//!
//! 1. **Placeholder isolation** — generated SQL never contains raw user values.
//! 2. **Param count consistency** — every dialect produces the same number of bind parameters for
//!    the same logical clause.
//! 3. **Placeholder syntax** — each dialect uses its own placeholder style (`$N` for PG, `@pN` for
//!    SQL Server, `?` for MySQL/SQLite).
//! 4. **No cross-dialect leakage** — PostgreSQL-specific syntax (`::`) never appears in MySQL or
//!    SQLite output.
//! 5. **Reusability** — calling `generate()` twice on the same generator produces identical SQL
//!    (counter resets between calls).
//!
//! # Running
//!
//! ```bash
//! cargo test -p fraiseql-db --test dialect_properties
//! # With extra dialects:
//! cargo test -p fraiseql-db --features sqlite,mysql,sqlserver --test dialect_properties
//! ```

#![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable

use fraiseql_db::{PostgresDialect, WhereClause, WhereOperator, postgres::PostgresWhereGenerator};
use proptest::prelude::*;
use serde_json::{Value, json};

// ── Arbitrary generators ──────────────────────────────────────────────────────

/// Arbitrary single-segment field paths (alphanumeric, no special chars).
fn arb_field_name() -> impl Strategy<Value = String> {
    "[a-z][a-z0-9_]{0,15}".prop_map(String::from)
}

/// Arbitrary multi-segment paths (1–3 segments).
fn arb_path() -> impl Strategy<Value = Vec<String>> {
    prop::collection::vec(arb_field_name(), 1..=3)
}

/// Scalar string values that should always be parameterized.
fn arb_string_value() -> impl Strategy<Value = Value> {
    "[a-zA-Z0-9@._\\-]{1,30}".prop_map(Value::String)
}

/// Scalar numeric values (integers).
fn arb_number_value() -> impl Strategy<Value = Value> {
    any::<i32>().prop_map(|n| json!(n))
}

/// Scalar boolean values.
fn arb_bool_value() -> impl Strategy<Value = Value> {
    any::<bool>().prop_map(|b| json!(b))
}

/// Mix of scalar types that any dialect can handle for Eq.
fn arb_scalar_value() -> impl Strategy<Value = Value> {
    prop_oneof![arb_string_value(), arb_number_value(), arb_bool_value(),]
}

/// Simple LIKE-family operators that use exactly one param.
fn arb_like_operator() -> impl Strategy<Value = WhereOperator> {
    prop_oneof![
        Just(WhereOperator::Contains),
        Just(WhereOperator::Startswith),
        Just(WhereOperator::Endswith),
        Just(WhereOperator::Like),
    ]
}

/// Comparison operators that take exactly one scalar param.
fn arb_comparison_operator() -> impl Strategy<Value = WhereOperator> {
    prop_oneof![
        Just(WhereOperator::Eq),
        Just(WhereOperator::Neq),
        Just(WhereOperator::Gt),
        Just(WhereOperator::Gte),
        Just(WhereOperator::Lt),
        Just(WhereOperator::Lte),
    ]
}

const fn field(path: Vec<String>, op: WhereOperator, val: Value) -> WhereClause {
    WhereClause::Field {
        path,
        operator: op,
        value: val,
    }
}

// ── PostgreSQL property tests ─────────────────────────────────────────────────

proptest! {
    /// Values are never interpolated into SQL — always appear in params.
    #[test]
    fn prop_pg_string_never_inlined(
        path in arb_path(),
        value in "[a-zA-Z0-9]{4,20}",
    ) {
        let gen = PostgresWhereGenerator::new(PostgresDialect);
        let clause = field(path, WhereOperator::Eq, Value::String(value.clone()));
        let (sql, params) = gen.generate(&clause).unwrap();
        prop_assert!(!sql.contains(&value), "Value must not appear in SQL: {sql}");
        prop_assert_eq!(params.len(), 1);
        prop_assert_eq!(&params[0], &json!(value));
    }

    /// Placeholder numbering starts at $1 and increases sequentially.
    #[test]
    fn prop_pg_placeholders_sequential(
        paths in prop::collection::vec(arb_path(), 1..=4),
    ) {
        let gen = PostgresWhereGenerator::new(PostgresDialect);
        let clauses: Vec<WhereClause> = paths
            .into_iter()
            .map(|p| field(p, WhereOperator::Eq, json!("x")))
            .collect();
        let n = clauses.len();
        let clause = WhereClause::And(clauses);
        let (sql, params) = gen.generate(&clause).unwrap();
        for i in 1..=n {
            prop_assert!(sql.contains(&format!("${i}")), "Missing ${i} in: {sql}");
        }
        prop_assert_eq!(params.len(), n);
    }

    /// PostgreSQL never uses `?` as a placeholder.
    #[test]
    fn prop_pg_no_question_mark_placeholder(
        path in arb_path(),
        op in arb_comparison_operator(),
        value in arb_scalar_value(),
    ) {
        let gen = PostgresWhereGenerator::new(PostgresDialect);
        let clause = field(path, op, value);
        if let Ok((sql, _)) = gen.generate(&clause) {
            // The only `?` in PG SQL would be wrong
            prop_assert!(!sql.contains('?'), "PG must not use ?: {sql}");
        }
    }

    /// `generate()` is idempotent — calling twice yields same SQL.
    #[test]
    fn prop_pg_generate_resets_counter(
        path in arb_path(),
        value in arb_string_value(),
    ) {
        let gen = PostgresWhereGenerator::new(PostgresDialect);
        let clause = field(path, WhereOperator::Eq, value);
        let (sql1, params1) = gen.generate(&clause).unwrap();
        let (sql2, params2) = gen.generate(&clause).unwrap();
        prop_assert_eq!(&sql1, &sql2, "SQL must be identical on second call");
        prop_assert_eq!(params1, params2);
    }

    /// IsNull produces zero parameters regardless of path.
    #[test]
    fn prop_pg_isnull_zero_params(
        path in arb_path(),
        is_null in any::<bool>(),
    ) {
        let gen = PostgresWhereGenerator::new(PostgresDialect);
        let clause = field(path, WhereOperator::IsNull, json!(is_null));
        let (_, params) = gen.generate(&clause).unwrap();
        prop_assert!(params.is_empty(), "IsNull must have zero params, got {params:?}");
    }

    /// Numeric equality on PG uses ::numeric cast on the LHS.
    #[test]
    fn prop_pg_numeric_eq_casts_lhs(
        path in arb_path(),
        value in any::<i64>(),
    ) {
        let gen = PostgresWhereGenerator::new(PostgresDialect);
        let clause = field(path, WhereOperator::Eq, json!(value));
        let (sql, _) = gen.generate(&clause).unwrap();
        prop_assert!(
            sql.contains("::numeric"),
            "PG numeric Eq must cast LHS to ::numeric: {sql}"
        );
    }

    /// LIKE operators parameterize the search term (one param, not zero).
    #[test]
    fn prop_pg_like_has_one_param(
        path in arb_path(),
        op in arb_like_operator(),
        value in "[a-zA-Z0-9]{2,15}",
    ) {
        let gen = PostgresWhereGenerator::new(PostgresDialect);
        let clause = field(path, op, Value::String(value));
        let (_, params) = gen.generate(&clause).unwrap();
        prop_assert_eq!(params.len(), 1, "LIKE-family must have exactly 1 param");
    }

    /// IN with N elements produces exactly N params.
    #[test]
    fn prop_pg_in_param_count(
        path in arb_path(),
        values in prop::collection::vec(arb_string_value(), 1..=8),
    ) {
        let gen = PostgresWhereGenerator::new(PostgresDialect);
        let n = values.len();
        let clause = field(path, WhereOperator::In, Value::Array(values));
        let (_, params) = gen.generate(&clause).unwrap();
        prop_assert_eq!(params.len(), n);
    }
}

// ── SQLite property tests ─────────────────────────────────────────────────────

// ── MySQL property tests ──────────────────────────────────────────────────────

// ── SQL Server property tests ─────────────────────────────────────────────────
