#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use super::*;

// --- DatabaseType::supports ---

#[test]
fn test_postgres_supports_all_features() {
    for feature in all_features() {
        assert!(
            DatabaseType::PostgreSQL.supports(feature),
            "PostgreSQL should support {feature:?}"
        );
    }
}

#[test]
fn test_mysql_does_not_support_jsonb() {
    assert!(!DatabaseType::MySQL.supports(Feature::JsonbPathOps));
}

#[test]
fn test_mysql_supports_mutations() {
    assert!(DatabaseType::MySQL.supports(Feature::Mutations));
}

#[test]
fn test_mysql_supports_window_functions() {
    assert!(DatabaseType::MySQL.supports(Feature::WindowFunctions));
}

#[test]
fn test_mysql_does_not_support_stddev() {
    assert!(!DatabaseType::MySQL.supports(Feature::StddevVariance));
}

#[test]
fn test_sqlite_supports_cte() {
    assert!(DatabaseType::SQLite.supports(Feature::CommonTableExpressions));
}

#[test]
fn test_sqlite_does_not_support_mutations() {
    assert!(!DatabaseType::SQLite.supports(Feature::Mutations));
}

#[test]
fn test_sqlite_does_not_support_subscriptions() {
    assert!(!DatabaseType::SQLite.supports(Feature::Subscriptions));
}

#[test]
fn test_sqlite_does_not_support_window_functions() {
    assert!(!DatabaseType::SQLite.supports(Feature::WindowFunctions));
}

#[test]
fn test_sqlserver_does_not_support_jsonb() {
    assert!(!DatabaseType::SQLServer.supports(Feature::JsonbPathOps));
}

#[test]
fn test_sqlserver_supports_mutations() {
    assert!(DatabaseType::SQLServer.supports(Feature::Mutations));
}

// --- DialectCapabilityGuard::check ---

#[test]
fn test_guard_ok_when_supported() {
    assert!(DialectCapabilityGuard::check(DatabaseType::MySQL, Feature::Mutations).is_ok());
}

#[test]
fn test_guard_err_when_unsupported() {
    let result = DialectCapabilityGuard::check(DatabaseType::MySQL, Feature::JsonbPathOps);
    assert!(matches!(result, Err(FraiseQLError::Unsupported { .. })));
}

#[test]
fn test_guard_error_mentions_feature_and_dialect() {
    let err =
        DialectCapabilityGuard::check(DatabaseType::MySQL, Feature::JsonbPathOps).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("JSONB"), "message should mention feature: {msg}");
    assert!(msg.contains("mysql"), "message should mention dialect: {msg}");
}

#[test]
fn test_guard_error_includes_suggestion() {
    let err =
        DialectCapabilityGuard::check(DatabaseType::MySQL, Feature::JsonbPathOps).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("json_extract"), "message should include suggestion: {msg}");
}

#[test]
fn test_guard_check_all_returns_all_failures() {
    let result = DialectCapabilityGuard::check_all(
        DatabaseType::SQLite,
        &[
            Feature::Mutations,
            Feature::WindowFunctions,
            Feature::CommonTableExpressions, // supported
        ],
    );
    let err = result.unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("Mutations"), "should mention mutations: {msg}");
    assert!(msg.contains("Window"), "should mention window functions: {msg}");
    // CTE is supported — must NOT appear in the error
    assert!(!msg.contains("Common Table"), "should not mention CTEs: {msg}");
}

#[test]
fn test_guard_check_all_ok_when_all_supported() {
    assert!(
        DialectCapabilityGuard::check_all(
            DatabaseType::PostgreSQL,
            &[
                Feature::JsonbPathOps,
                Feature::Subscriptions,
                Feature::Mutations
            ],
        )
        .is_ok()
    );
}

#[test]
fn test_guard_error_links_to_compatibility_docs() {
    let err =
        DialectCapabilityGuard::check(DatabaseType::MySQL, Feature::JsonbPathOps).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("docs/database-compatibility.md"),
        "unsupported feature error must link to compatibility docs: {msg}"
    );
}

#[test]
fn test_guard_check_all_error_links_to_compatibility_docs() {
    let err = DialectCapabilityGuard::check_all(
        DatabaseType::SQLite,
        &[Feature::Mutations, Feature::WindowFunctions],
    )
    .unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("docs/database-compatibility.md"),
        "check_all error must link to compatibility docs: {msg}"
    );
}

// --- DatabaseType::json_field_expr ---

#[test]
fn test_json_field_expr_postgres() {
    assert_eq!(DatabaseType::PostgreSQL.json_field_expr("created_at"), "data->>'created_at'");
}

#[test]
fn test_json_field_expr_mysql() {
    assert_eq!(
        DatabaseType::MySQL.json_field_expr("name"),
        "JSON_UNQUOTE(JSON_EXTRACT(data, '$.name'))"
    );
}

#[test]
fn test_json_field_expr_sqlite() {
    assert_eq!(DatabaseType::SQLite.json_field_expr("email"), "json_extract(data, '$.email')");
}

#[test]
fn test_json_field_expr_sqlserver() {
    assert_eq!(
        DatabaseType::SQLServer.json_field_expr("status"),
        "JSON_VALUE(data, '$.status')"
    );
}

// --- DatabaseType::typed_json_field_expr ---

#[test]
fn test_typed_expr_text_is_plain_extraction() {
    // Text type should produce the same result as json_field_expr
    assert_eq!(
        DatabaseType::PostgreSQL.typed_json_field_expr("name", OrderByFieldType::Text),
        DatabaseType::PostgreSQL.json_field_expr("name"),
    );
}

#[test]
fn test_typed_expr_postgres_numeric() {
    assert_eq!(
        DatabaseType::PostgreSQL.typed_json_field_expr("amount", OrderByFieldType::Numeric),
        "(data->>'amount')::numeric"
    );
}

#[test]
fn test_typed_expr_postgres_integer() {
    assert_eq!(
        DatabaseType::PostgreSQL.typed_json_field_expr("count", OrderByFieldType::Integer),
        "(data->>'count')::bigint"
    );
}

#[test]
fn test_typed_expr_postgres_datetime() {
    assert_eq!(
        DatabaseType::PostgreSQL
            .typed_json_field_expr("created_at", OrderByFieldType::DateTime),
        "(data->>'created_at')::timestamptz"
    );
}

#[test]
fn test_typed_expr_postgres_boolean() {
    assert_eq!(
        DatabaseType::PostgreSQL.typed_json_field_expr("active", OrderByFieldType::Boolean),
        "(data->>'active')::boolean"
    );
}

#[test]
fn test_typed_expr_mysql_numeric() {
    assert_eq!(
        DatabaseType::MySQL.typed_json_field_expr("amount", OrderByFieldType::Numeric),
        "CAST(JSON_UNQUOTE(JSON_EXTRACT(data, '$.amount')) AS DECIMAL(38,12))"
    );
}

#[test]
fn test_typed_expr_mysql_integer() {
    assert_eq!(
        DatabaseType::MySQL.typed_json_field_expr("count", OrderByFieldType::Integer),
        "CAST(JSON_UNQUOTE(JSON_EXTRACT(data, '$.count')) AS SIGNED)"
    );
}

#[test]
fn test_typed_expr_sqlite_numeric() {
    assert_eq!(
        DatabaseType::SQLite.typed_json_field_expr("amount", OrderByFieldType::Numeric),
        "CAST(json_extract(data, '$.amount') AS REAL)"
    );
}

#[test]
fn test_typed_expr_sqlite_datetime_is_text() {
    // SQLite: ISO-8601 dates sort correctly as TEXT
    assert_eq!(
        DatabaseType::SQLite.typed_json_field_expr("created_at", OrderByFieldType::DateTime),
        "CAST(json_extract(data, '$.created_at') AS TEXT)"
    );
}

#[test]
fn test_typed_expr_sqlserver_numeric() {
    assert_eq!(
        DatabaseType::SQLServer.typed_json_field_expr("amount", OrderByFieldType::Numeric),
        "CAST(JSON_VALUE(data, '$.amount') AS DECIMAL(38,12))"
    );
}

#[test]
fn test_typed_expr_sqlserver_datetime() {
    assert_eq!(
        DatabaseType::SQLServer.typed_json_field_expr("created_at", OrderByFieldType::DateTime),
        "CAST(JSON_VALUE(data, '$.created_at') AS DATETIME2)"
    );
}

// Helper: iterate all Feature variants
fn all_features() -> impl Iterator<Item = Feature> {
    [
        Feature::JsonbPathOps,
        Feature::Subscriptions,
        Feature::Mutations,
        Feature::WindowFunctions,
        Feature::CommonTableExpressions,
        Feature::FullTextSearch,
        Feature::AdvisoryLocks,
        Feature::StddevVariance,
        Feature::Upsert,
        Feature::ArrayTypes,
        Feature::BackwardPagination,
    ]
    .into_iter()
}
