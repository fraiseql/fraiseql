#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use super::*;
use crate::types::sql_hints::OrderDirection;

#[test]
fn test_append_order_by_none() {
    let mut sql = "SELECT data FROM v_user".to_string();
    let appended = append_order_by(&mut sql, None, DatabaseType::PostgreSQL).unwrap();
    assert!(!appended);
    assert!(!sql.contains("ORDER BY"));
}

#[test]
fn test_append_order_by_empty() {
    let mut sql = "SELECT data FROM v_user".to_string();
    let appended = append_order_by(&mut sql, Some(&[]), DatabaseType::PostgreSQL).unwrap();
    assert!(!appended);
    assert!(!sql.contains("ORDER BY"));
}

#[test]
fn test_append_order_by_single_clause_postgres() {
    let mut sql = "SELECT data FROM v_user".to_string();
    let clauses = [OrderByClause::new(
        "createdAt".to_string(),
        OrderDirection::Desc,
    )];
    let appended = append_order_by(&mut sql, Some(&clauses), DatabaseType::PostgreSQL).unwrap();
    assert!(appended);
    assert_eq!(sql, "SELECT data FROM v_user ORDER BY data->>'created_at' DESC");
}

#[test]
fn test_append_order_by_multiple_clauses_postgres() {
    let mut sql = "SELECT data FROM v_user".to_string();
    let clauses = [
        OrderByClause::new("lastName".to_string(), OrderDirection::Asc),
        OrderByClause::new("createdAt".to_string(), OrderDirection::Desc),
    ];
    let appended = append_order_by(&mut sql, Some(&clauses), DatabaseType::PostgreSQL).unwrap();
    assert!(appended);
    assert_eq!(
        sql,
        "SELECT data FROM v_user ORDER BY data->>'last_name' ASC, data->>'created_at' DESC"
    );
}

#[test]
fn test_append_order_by_invalid_field_name() {
    let mut sql = "SELECT data FROM v_user".to_string();
    let clauses = [OrderByClause::new(
        "field'; DROP TABLE users; --".to_string(),
        OrderDirection::Asc,
    )];
    let result = append_order_by(&mut sql, Some(&clauses), DatabaseType::PostgreSQL);
    assert!(result.is_err());
}

#[test]
fn test_append_order_by_snake_case_passthrough() {
    let mut sql = "SELECT data FROM v_user".to_string();
    let clauses = [OrderByClause::new("id".to_string(), OrderDirection::Asc)];
    let appended = append_order_by(&mut sql, Some(&clauses), DatabaseType::PostgreSQL).unwrap();
    assert!(appended);
    assert_eq!(sql, "SELECT data FROM v_user ORDER BY data->>'id' ASC");
}

// ── typed ORDER BY ───────────────────────────────────────────────────

#[test]
fn test_append_order_by_numeric_cast_postgres() {
    use crate::types::sql_hints::ScalarFieldType;

    let mut sql = "SELECT data FROM v_order".to_string();
    let mut clause = OrderByClause::new("totalAmount".to_string(), OrderDirection::Desc);
    clause.field_type = ScalarFieldType::Numeric;
    let appended = append_order_by(&mut sql, Some(&[clause]), DatabaseType::PostgreSQL).unwrap();
    assert!(appended);
    assert_eq!(sql, "SELECT data FROM v_order ORDER BY (data->>'total_amount')::numeric DESC");
}

#[test]
fn test_append_order_by_datetime_cast_postgres() {
    use crate::types::sql_hints::ScalarFieldType;

    let mut sql = "SELECT data FROM v_event".to_string();
    let mut clause = OrderByClause::new("createdAt".to_string(), OrderDirection::Desc);
    clause.field_type = ScalarFieldType::DateTime;
    let appended = append_order_by(&mut sql, Some(&[clause]), DatabaseType::PostgreSQL).unwrap();
    assert!(appended);
    assert_eq!(sql, "SELECT data FROM v_event ORDER BY (data->>'created_at')::timestamptz DESC");
}

// ── native column ORDER BY ───────────────────────────────────────────

#[test]
fn test_append_order_by_native_column() {
    let mut sql = "SELECT data FROM tv_user".to_string();
    let clause = OrderByClause {
        field:         "createdAt".to_string(),
        direction:     OrderDirection::Desc,
        field_type:    crate::types::sql_hints::ScalarFieldType::DateTime,
        native_column: Some("created_at".to_string()),
        vector:        None,
    };
    let appended = append_order_by(&mut sql, Some(&[clause]), DatabaseType::PostgreSQL).unwrap();
    assert!(appended);
    // Native column is used directly — no JSON extraction, no cast.
    assert_eq!(sql, "SELECT data FROM tv_user ORDER BY created_at DESC");
}

#[test]
fn test_append_order_by_mixed_native_and_jsonb() {
    use crate::types::sql_hints::ScalarFieldType;

    let mut sql = "SELECT data FROM tv_user".to_string();
    let clauses = [
        OrderByClause {
            field:         "createdAt".to_string(),
            direction:     OrderDirection::Desc,
            field_type:    ScalarFieldType::DateTime,
            native_column: Some("created_at".to_string()),
            vector:        None,
        },
        {
            let mut c = OrderByClause::new("name".to_string(), OrderDirection::Asc);
            c.field_type = ScalarFieldType::Text;
            c
        },
    ];
    let appended = append_order_by(&mut sql, Some(&clauses), DatabaseType::PostgreSQL).unwrap();
    assert!(appended);
    assert_eq!(sql, "SELECT data FROM tv_user ORDER BY created_at DESC, data->>'name' ASC");
}

// ── render_order_by_columns (bare list, for backends that supply the keyword) ──

#[test]
fn test_render_order_by_columns_none() {
    assert!(render_order_by_columns(None, DatabaseType::PostgreSQL).unwrap().is_none());
}

#[test]
fn test_render_order_by_columns_empty() {
    assert!(render_order_by_columns(Some(&[]), DatabaseType::PostgreSQL).unwrap().is_none());
}

#[test]
fn test_render_order_by_columns_no_keyword_prefix() {
    let clauses = [
        OrderByClause::new("lastName".to_string(), OrderDirection::Asc),
        OrderByClause::new("createdAt".to_string(), OrderDirection::Desc),
    ];
    let cols = render_order_by_columns(Some(&clauses), DatabaseType::PostgreSQL)
        .unwrap()
        .unwrap();
    // No leading "ORDER BY" — the backend's query builder supplies the keyword.
    assert_eq!(cols, "data->>'last_name' ASC, data->>'created_at' DESC");
    assert!(!cols.contains("ORDER BY"));
}

#[test]
fn test_render_order_by_columns_matches_append_body() {
    // The bare list must equal append_order_by's output minus the " ORDER BY " prefix.
    let clauses = [OrderByClause::new(
        "createdAt".to_string(),
        OrderDirection::Desc,
    )];
    let cols = render_order_by_columns(Some(&clauses), DatabaseType::PostgreSQL)
        .unwrap()
        .unwrap();
    let mut sql = String::new();
    append_order_by(&mut sql, Some(&clauses), DatabaseType::PostgreSQL).unwrap();
    assert_eq!(sql, format!(" ORDER BY {cols}"));
}

#[test]
fn test_render_order_by_columns_invalid_field_name() {
    let clauses = [OrderByClause::new(
        "field'; DROP TABLE users; --".to_string(),
        OrderDirection::Asc,
    )];
    assert!(render_order_by_columns(Some(&clauses), DatabaseType::PostgreSQL).is_err());
}

// ── vector-distance ORDER BY (#386, #959) ────────────────────────────────

fn vector_order(operator: &str, literal: &str, kind: VectorOperandKind) -> OrderByClause {
    let mut clause = OrderByClause::new("embedding".to_string(), OrderDirection::Asc);
    clause.native_column = Some("\"embedding\"".to_string());
    clause.vector = Some(crate::types::sql_hints::VectorDistanceOrder {
        operator: operator.to_string(),
        query_vector: literal.to_string(),
        kind,
    });
    clause
}

#[test]
fn float_vector_order_casts_to_vector() {
    let cols = render_order_by_columns(
        Some(&[vector_order("<=>", "[1,0,0.5]", VectorOperandKind::Float)]),
        DatabaseType::PostgreSQL,
    )
    .unwrap()
    .unwrap();
    assert_eq!(cols, "\"embedding\" <=> '[1,0,0.5]'::vector ASC");
}

/// `varbit`, never `bit`: `'1011'::bit` is `bit(1)`, which silently reduces
/// every comparison to the first bit — an ordering that looks plausible and is
/// wrong.
#[test]
fn bit_vector_order_casts_to_varbit() {
    for (op, expected) in [("<~>", "<~>"), ("<%>", "<%>")] {
        let cols = render_order_by_columns(
            Some(&[vector_order(op, "1011", VectorOperandKind::Bit)]),
            DatabaseType::PostgreSQL,
        )
        .unwrap()
        .unwrap();
        assert_eq!(cols, format!("\"embedding\" {expected} '1011'::varbit ASC"));
    }
}

/// The operator is validated against the operand kind, not just against a list
/// of known operators: `<~>` over a `::vector` cast is an operator PostgreSQL
/// has no candidate for.
#[test]
fn a_vector_operator_of_the_other_kind_is_refused() {
    for (op, literal, kind) in [
        ("<~>", "[1,0]", VectorOperandKind::Float),
        ("<=>", "1011", VectorOperandKind::Bit),
    ] {
        let err = render_order_by_columns(
            Some(&[vector_order(op, literal, kind)]),
            DatabaseType::PostgreSQL,
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("unsupported vector distance operator"), "got: {err}");
    }
}

#[test]
fn a_malformed_bit_literal_is_refused() {
    for literal in ["10x1", "", "[1,0]"] {
        let err = render_order_by_columns(
            Some(&[vector_order("<~>", literal, VectorOperandKind::Bit)]),
            DatabaseType::PostgreSQL,
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("malformed vector literal"), "got for {literal:?}: {err}");
    }
}
