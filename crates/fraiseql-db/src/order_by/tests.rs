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
fn test_append_order_by_mysql() {
    let mut sql = "SELECT data FROM v_user".to_string();
    let clauses = [OrderByClause::new(
        "firstName".to_string(),
        OrderDirection::Asc,
    )];
    let appended = append_order_by(&mut sql, Some(&clauses), DatabaseType::MySQL).unwrap();
    assert!(appended);
    assert_eq!(
        sql,
        "SELECT data FROM v_user ORDER BY JSON_UNQUOTE(JSON_EXTRACT(data, '$.first_name')) ASC"
    );
}

#[test]
fn test_append_order_by_sqlite() {
    let mut sql = "SELECT data FROM v_user".to_string();
    let clauses = [OrderByClause::new(
        "firstName".to_string(),
        OrderDirection::Asc,
    )];
    let appended = append_order_by(&mut sql, Some(&clauses), DatabaseType::SQLite).unwrap();
    assert!(appended);
    assert_eq!(sql, "SELECT data FROM v_user ORDER BY json_extract(data, '$.first_name') ASC");
}

#[test]
fn test_append_order_by_sqlserver() {
    let mut sql = "SELECT data FROM v_user".to_string();
    let clauses = [OrderByClause::new(
        "firstName".to_string(),
        OrderDirection::Desc,
    )];
    let appended = append_order_by(&mut sql, Some(&clauses), DatabaseType::SQLServer).unwrap();
    assert!(appended);
    assert_eq!(sql, "SELECT data FROM v_user ORDER BY JSON_VALUE(data, '$.first_name') DESC");
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
    use crate::types::sql_hints::OrderByFieldType;

    let mut sql = "SELECT data FROM v_order".to_string();
    let mut clause = OrderByClause::new("totalAmount".to_string(), OrderDirection::Desc);
    clause.field_type = OrderByFieldType::Numeric;
    let appended = append_order_by(&mut sql, Some(&[clause]), DatabaseType::PostgreSQL).unwrap();
    assert!(appended);
    assert_eq!(sql, "SELECT data FROM v_order ORDER BY (data->>'total_amount')::numeric DESC");
}

#[test]
fn test_append_order_by_integer_cast_mysql() {
    use crate::types::sql_hints::OrderByFieldType;

    let mut sql = "SELECT data FROM v_order".to_string();
    let mut clause = OrderByClause::new("quantity".to_string(), OrderDirection::Asc);
    clause.field_type = OrderByFieldType::Integer;
    let appended = append_order_by(&mut sql, Some(&[clause]), DatabaseType::MySQL).unwrap();
    assert!(appended);
    assert_eq!(
        sql,
        "SELECT data FROM v_order ORDER BY CAST(JSON_UNQUOTE(JSON_EXTRACT(data, '$.quantity')) AS SIGNED) ASC"
    );
}

#[test]
fn test_append_order_by_datetime_cast_postgres() {
    use crate::types::sql_hints::OrderByFieldType;

    let mut sql = "SELECT data FROM v_event".to_string();
    let mut clause = OrderByClause::new("createdAt".to_string(), OrderDirection::Desc);
    clause.field_type = OrderByFieldType::DateTime;
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
        field_type:    crate::types::sql_hints::OrderByFieldType::DateTime,
        native_column: Some("created_at".to_string()),
    };
    let appended = append_order_by(&mut sql, Some(&[clause]), DatabaseType::PostgreSQL).unwrap();
    assert!(appended);
    // Native column is used directly — no JSON extraction, no cast.
    assert_eq!(sql, "SELECT data FROM tv_user ORDER BY created_at DESC");
}

#[test]
fn test_append_order_by_mixed_native_and_jsonb() {
    use crate::types::sql_hints::OrderByFieldType;

    let mut sql = "SELECT data FROM tv_user".to_string();
    let clauses = [
        OrderByClause {
            field:         "createdAt".to_string(),
            direction:     OrderDirection::Desc,
            field_type:    OrderByFieldType::DateTime,
            native_column: Some("created_at".to_string()),
        },
        {
            let mut c = OrderByClause::new("name".to_string(), OrderDirection::Asc);
            c.field_type = OrderByFieldType::Text;
            c
        },
    ];
    let appended = append_order_by(&mut sql, Some(&clauses), DatabaseType::PostgreSQL).unwrap();
    assert!(appended);
    assert_eq!(sql, "SELECT data FROM tv_user ORDER BY created_at DESC, data->>'name' ASC");
}
