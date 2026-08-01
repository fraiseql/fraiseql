#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use super::*;

// --- DatabaseType::json_field_expr ---

#[test]
fn test_json_field_expr_postgres() {
    assert_eq!(DatabaseType::PostgreSQL.json_field_expr("created_at"), "data->>'created_at'");
}

// --- DatabaseType::typed_json_field_expr ---

#[test]
fn test_typed_expr_text_is_plain_extraction() {
    // Text type should produce the same result as json_field_expr
    assert_eq!(
        DatabaseType::PostgreSQL.typed_json_field_expr("name", ScalarFieldType::Text),
        DatabaseType::PostgreSQL.json_field_expr("name"),
    );
}

#[test]
fn test_typed_expr_postgres_numeric() {
    assert_eq!(
        DatabaseType::PostgreSQL.typed_json_field_expr("amount", ScalarFieldType::Numeric),
        "(data->>'amount')::numeric"
    );
}

#[test]
fn test_typed_expr_postgres_integer() {
    assert_eq!(
        DatabaseType::PostgreSQL.typed_json_field_expr("count", ScalarFieldType::Integer),
        "(data->>'count')::bigint"
    );
}

#[test]
fn test_typed_expr_postgres_datetime() {
    assert_eq!(
        DatabaseType::PostgreSQL.typed_json_field_expr("created_at", ScalarFieldType::DateTime),
        "(data->>'created_at')::timestamptz"
    );
}

#[test]
fn test_typed_expr_postgres_boolean() {
    assert_eq!(
        DatabaseType::PostgreSQL.typed_json_field_expr("is_active", ScalarFieldType::Boolean),
        "(data->>'is_active')::boolean"
    );
}

// ---------------------------------------------------------------------------
// #722 — PostgreSQL escapes LIKE metacharacters with `\` by default
// ---------------------------------------------------------------------------

/// `escape_like_literal` neutralises `%`, `_` and `\` with a backslash, which
/// is PostgreSQL's default LIKE escape character — an explicit `ESCAPE` clause
/// would be noise.
#[test]
fn postgres_needs_no_explicit_like_escape_clause() {
    use crate::dialect::PostgresDialect;

    assert_eq!((&PostgresDialect as &dyn SqlDialect).like_escape_clause(), "");
}

/// The dialect renders each scalar type through one table.
///
/// The WHERE generator and the ORDER BY renderer used to carry separate
/// type → SQL-type mappings, so `ORDER BY amount` and `amount: { gt: … }`
/// could disagree about what `amount` is (#798).
#[test]
fn order_by_and_where_render_the_same_cast() {
    let db = DatabaseType::PostgreSQL;
    for ty in [
        ScalarFieldType::Text,
        ScalarFieldType::Integer,
        ScalarFieldType::Numeric,
        ScalarFieldType::Boolean,
        ScalarFieldType::DateTime,
        ScalarFieldType::Date,
        ScalarFieldType::Time,
    ] {
        let order_by = db.typed_json_field_expr("amount", ty);
        let base = db.json_field_expr("amount");
        let where_side = db.dialect().cast_expr_as(&base, ty).into_owned();
        assert_eq!(
            order_by, where_side,
            "{db:?} renders {ty:?} differently for ORDER BY and WHERE"
        );
    }
}
