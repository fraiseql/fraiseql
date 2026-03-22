//! Property-based tests for WHERE clause AST and SQL generation.
//!
//! Verifies structural invariants that must hold for all inputs:
//!
//! 1. **Serialization roundtrip** — `WhereClause` survives JSON serialize/deserialize.
//! 2. **Balanced parentheses** — generated SQL always has matched parens.
//! 3. **SQL injection safety** — single quotes in values are always escaped.
//! 4. **Empty clause identity** — `And([])` → `"TRUE"`, `Or([])` → `"FALSE"`.

#![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable

use fraiseql_db::{WhereClause, WhereOperator, where_sql_generator::WhereSqlGenerator};
use proptest::prelude::*;
use serde_json::json;

// ---------------------------------------------------------------------------
// Strategies
// ---------------------------------------------------------------------------

/// Generate an arbitrary simple comparison operator (supported by `WhereSqlGenerator`).
fn arb_simple_operator() -> impl Strategy<Value = WhereOperator> {
    prop_oneof![
        Just(WhereOperator::Eq),
        Just(WhereOperator::Neq),
        Just(WhereOperator::Gt),
        Just(WhereOperator::Gte),
        Just(WhereOperator::Lt),
        Just(WhereOperator::Lte),
        Just(WhereOperator::Contains),
        Just(WhereOperator::Icontains),
        Just(WhereOperator::Startswith),
        Just(WhereOperator::Istartswith),
        Just(WhereOperator::Endswith),
        Just(WhereOperator::Iendswith),
        Just(WhereOperator::Like),
        Just(WhereOperator::Ilike),
        Just(WhereOperator::Nlike),
        Just(WhereOperator::Nilike),
        Just(WhereOperator::Regex),
        Just(WhereOperator::Iregex),
        Just(WhereOperator::Nregex),
        Just(WhereOperator::Niregex),
    ]
}

/// Generate a safe field name (alphanumeric + underscore, 1-20 chars).
fn arb_field_name() -> impl Strategy<Value = String> {
    "[a-z][a-z0-9_]{0,19}".prop_map(String::from)
}

/// Generate a safe string value (printable ASCII, up to 100 chars).
fn arb_string_value() -> impl Strategy<Value = serde_json::Value> {
    "[[:print:]]{0,100}".prop_map(|s| json!(s))
}

/// Generate a leaf WHERE clause (Field node with string value).
fn arb_leaf_clause() -> impl Strategy<Value = WhereClause> {
    (arb_field_name(), arb_simple_operator(), arb_string_value()).prop_map(
        |(field, operator, value)| WhereClause::Field {
            path: vec![field],
            operator,
            value,
        },
    )
}

/// Generate an arbitrary WHERE clause tree (bounded depth).
fn arb_where_clause() -> impl Strategy<Value = WhereClause> {
    arb_leaf_clause().prop_recursive(
        3,  // max depth
        16, // max nodes
        4,  // items per collection
        |inner| {
            prop_oneof![
                // AND combinator
                prop::collection::vec(inner.clone(), 0..4).prop_map(WhereClause::And),
                // OR combinator
                prop::collection::vec(inner.clone(), 0..4).prop_map(WhereClause::Or),
                // NOT combinator
                inner.prop_map(|c| WhereClause::Not(Box::new(c))),
            ]
        },
    )
}

// ---------------------------------------------------------------------------
// Property: Serialization roundtrip
// ---------------------------------------------------------------------------

proptest! {
    #[test]
    fn where_clause_serialization_roundtrip(clause in arb_where_clause()) {
        let json = serde_json::to_string(&clause).unwrap();
        let deserialized: WhereClause = serde_json::from_str(&json).unwrap();
        prop_assert_eq!(clause, deserialized);
    }
}

// ---------------------------------------------------------------------------
// Property: SQL generation produces balanced parentheses (outside string literals)
// ---------------------------------------------------------------------------

/// Count unquoted parentheses in SQL (skips content inside single-quoted strings).
fn count_unquoted_parens(sql: &str) -> (usize, usize) {
    let mut opens = 0usize;
    let mut closes = 0usize;
    let mut in_string = false;
    let chars: Vec<char> = sql.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '\'' {
            if in_string && i + 1 < chars.len() && chars[i + 1] == '\'' {
                i += 2; // escaped quote ''
                continue;
            }
            in_string = !in_string;
        } else if !in_string {
            if chars[i] == '(' {
                opens += 1;
            } else if chars[i] == ')' {
                closes += 1;
            }
        }
        i += 1;
    }
    (opens, closes)
}

proptest! {
    #[test]
    fn sql_generation_balanced_parens(clause in arb_where_clause()) {
        if let Ok(sql) = WhereSqlGenerator::to_sql(&clause) {
            let (opens, closes) = count_unquoted_parens(&sql);
            prop_assert_eq!(
                opens, closes,
                "Unbalanced parentheses in SQL: {}", sql
            );
        }
        // Errors from unsupported operators are acceptable — we only check valid output.
    }
}

// ---------------------------------------------------------------------------
// Property: SQL injection safety — single quotes in values are escaped
// ---------------------------------------------------------------------------

/// Count occurrences of a substring.
fn count_substr(haystack: &str, needle: &str) -> usize {
    haystack.matches(needle).count()
}

proptest! {
    #[test]
    fn sql_generation_escapes_single_quotes(
        field in arb_field_name(),
        // Generate values that contain single quotes
        value in "[a-z]{0,20}'[a-z]{0,20}",
    ) {
        let input_quotes = count_substr(&value, "'");
        let clause = WhereClause::Field {
            path: vec![field],
            operator: WhereOperator::Eq,
            value: json!(value),
        };

        let sql = WhereSqlGenerator::to_sql(&clause).unwrap();

        // The escape function doubles every single quote in the value.
        // So if the input had N quotes, the escaped output has 2N quotes
        // from those positions. The total SQL will also have quotes for
        // the string delimiters and the JSONB accessor, but the key
        // invariant is: the escaped value portion has no lone quotes.
        let escaped_quotes = count_substr(&sql, "''");
        prop_assert!(
            escaped_quotes >= input_quotes,
            "Each input quote should be doubled. Input quotes: {}, doubled pairs: {}, SQL: {}",
            input_quotes, escaped_quotes, sql
        );
    }
}

// ---------------------------------------------------------------------------
// Property: Empty clause identity elements
// ---------------------------------------------------------------------------

#[test]
fn empty_and_produces_true() {
    let sql = WhereSqlGenerator::to_sql(&WhereClause::And(vec![])).unwrap();
    assert_eq!(sql, "TRUE");
}

#[test]
fn empty_or_produces_false() {
    let sql = WhereSqlGenerator::to_sql(&WhereClause::Or(vec![])).unwrap();
    assert_eq!(sql, "FALSE");
}

// ---------------------------------------------------------------------------
// Property: IsNull clause produces IS NULL / IS NOT NULL
// ---------------------------------------------------------------------------

proptest! {
    #[test]
    fn is_null_produces_valid_sql(
        field in arb_field_name(),
        is_null in prop::bool::ANY,
    ) {
        let clause = WhereClause::Field {
            path: vec![field],
            operator: WhereOperator::IsNull,
            value: json!(is_null),
        };

        let sql = WhereSqlGenerator::to_sql(&clause).unwrap();
        if is_null {
            prop_assert!(sql.ends_with("IS NULL"), "Expected IS NULL, got: {sql}");
        } else {
            prop_assert!(sql.ends_with("IS NOT NULL"), "Expected IS NOT NULL, got: {sql}");
        }
    }
}

// ---------------------------------------------------------------------------
// Property: Numeric and boolean values are never quoted in SQL
// ---------------------------------------------------------------------------

proptest! {
    #[test]
    fn numeric_values_not_quoted(
        field in arb_field_name(),
        n in -1_000_000i64..1_000_000i64,
    ) {
        let clause = WhereClause::Field {
            path: vec![field],
            operator: WhereOperator::Eq,
            value: json!(n),
        };

        let sql = WhereSqlGenerator::to_sql(&clause).unwrap();
        // The value portion should not be wrapped in quotes
        let value_part = sql.split("= ").last().unwrap();
        prop_assert!(
            !value_part.starts_with('\''),
            "Numeric value should not be quoted: {sql}"
        );
    }
}
