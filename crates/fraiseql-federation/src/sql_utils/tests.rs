#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use fraiseql_db::DatabaseType;
use serde_json::json;

use super::*;

#[test]
fn test_value_to_sql_literal_string() {
    let result =
        value_to_sql_literal(DatabaseType::PostgreSQL, &Value::String("John".to_string())).unwrap();
    assert_eq!(result, "'John'");
}

#[test]
fn test_value_to_sql_literal_string_with_quotes() {
    let result =
        value_to_sql_literal(DatabaseType::PostgreSQL, &Value::String("O'Brien".to_string()))
            .unwrap();
    assert_eq!(result, "'O''Brien'");
}

#[test]
fn test_value_to_sql_literal_number() {
    let result = value_to_sql_literal(DatabaseType::PostgreSQL, &json!(123)).unwrap();
    assert_eq!(result, "123");

    let result = value_to_sql_literal(DatabaseType::PostgreSQL, &json!(99.99)).unwrap();
    assert_eq!(result, "99.99");
}

#[test]
fn test_value_to_sql_literal_bool() {
    let result = value_to_sql_literal(DatabaseType::PostgreSQL, &Value::Bool(true)).unwrap();
    assert_eq!(result, "true");

    let result = value_to_sql_literal(DatabaseType::PostgreSQL, &Value::Bool(false)).unwrap();
    assert_eq!(result, "false");
}

#[test]
fn test_value_to_sql_literal_null() {
    let result = value_to_sql_literal(DatabaseType::PostgreSQL, &Value::Null).unwrap();
    assert_eq!(result, "NULL");
}

#[test]
fn test_value_to_sql_literal_array_error() {
    let result = value_to_sql_literal(DatabaseType::PostgreSQL, &Value::Array(vec![]));
    assert!(
        matches!(result, Err(FraiseQLError::Validation { .. })),
        "expected Validation error for Array input, got: {result:?}"
    );
}

#[test]
fn test_value_to_string() {
    assert_eq!(value_to_string(&Value::String("test".to_string())).unwrap(), "test");
    assert_eq!(value_to_string(&Value::Number(789.into())).unwrap(), "789");
    assert_eq!(value_to_string(&Value::Bool(true)).unwrap(), "true");
    assert_eq!(value_to_string(&Value::Null).unwrap(), "null");
}

/// #728 — literal escaping is dialect-dependent, and single-quote doubling is
/// only sound where the backend never treats backslash as an escape
/// (PostgreSQL with `standard_conforming_strings=on`, SQLite, SQL Server).
/// MySQL's default mode DOES, so `'O''Brien\'` re-escapes differently and the
/// helper must refuse rather than emit a wrong (injectable) literal.
#[test]
fn literal_building_refuses_backslash_escaping_dialects() {
    use serde_json::json;

    // Sound dialects keep working.
    for db in [
        DatabaseType::PostgreSQL,
        DatabaseType::SQLite,
        DatabaseType::SQLServer,
    ] {
        assert_eq!(
            value_to_sql_literal(db, &json!("O'Brien")).unwrap(),
            "'O''Brien'",
            "quote doubling is correct for {db:?}"
        );
    }

    // MySQL must fail loud: its default backslash-escape mode makes
    // quote-doubling-only escaping unsound, and which mode is active is a
    // per-connection server setting this helper cannot see.
    let err = value_to_sql_literal(DatabaseType::MySQL, &json!("O'Brien"))
        .expect_err("MySQL literal building must be refused, not mis-escaped");
    assert!(err.to_string().contains("MySQL"), "the refusal must name the dialect: {err}");
}
