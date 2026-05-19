#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use serde_json::json;

use super::*;

#[test]
fn test_simple_equality() {
    let clause = WhereClause::Field {
        path: vec!["status".to_string()],
        operator: WhereOperator::Eq,
        value: json!("active"),
    };

    let sql = WhereSqlGenerator::to_sql(&clause).unwrap();
    assert_eq!(sql, "data->>'status' = 'active'");
}

#[test]
fn test_nested_path() {
    let clause = WhereClause::Field {
        path: vec!["user".to_string(), "email".to_string()],
        operator: WhereOperator::Eq,
        value: json!("test@example.com"),
    };

    let sql = WhereSqlGenerator::to_sql(&clause).unwrap();
    assert_eq!(sql, "data#>'{user}'->>'email' = 'test@example.com'");
}

#[test]
fn test_icontains() {
    let clause = WhereClause::Field {
        path: vec!["name".to_string()],
        operator: WhereOperator::Icontains,
        value: json!("john"),
    };

    let sql = WhereSqlGenerator::to_sql(&clause).unwrap();
    assert_eq!(sql, "data->>'name' ILIKE '%john%'");
}

#[test]
fn test_startswith() {
    let clause = WhereClause::Field {
        path: vec!["email".to_string()],
        operator: WhereOperator::Startswith,
        value: json!("admin"),
    };

    let sql = WhereSqlGenerator::to_sql(&clause).unwrap();
    assert_eq!(sql, "data->>'email' LIKE 'admin%'");
}

#[test]
fn test_and_clause() {
    let clause = WhereClause::And(vec![
        WhereClause::Field {
            path: vec!["status".to_string()],
            operator: WhereOperator::Eq,
            value: json!("active"),
        },
        WhereClause::Field {
            path: vec!["age".to_string()],
            operator: WhereOperator::Gte,
            value: json!(18),
        },
    ]);

    let sql = WhereSqlGenerator::to_sql(&clause).unwrap();
    assert_eq!(sql, "(data->>'status' = 'active' AND data->>'age' >= 18)");
}

#[test]
fn test_or_clause() {
    let clause = WhereClause::Or(vec![
        WhereClause::Field {
            path: vec!["type".to_string()],
            operator: WhereOperator::Eq,
            value: json!("admin"),
        },
        WhereClause::Field {
            path: vec!["type".to_string()],
            operator: WhereOperator::Eq,
            value: json!("moderator"),
        },
    ]);

    let sql = WhereSqlGenerator::to_sql(&clause).unwrap();
    assert_eq!(sql, "(data->>'type' = 'admin' OR data->>'type' = 'moderator')");
}

#[test]
fn test_not_clause() {
    let clause = WhereClause::Not(Box::new(WhereClause::Field {
        path: vec!["deleted".to_string()],
        operator: WhereOperator::Eq,
        value: json!(true),
    }));

    let sql = WhereSqlGenerator::to_sql(&clause).unwrap();
    assert_eq!(sql, "NOT (data->>'deleted' = true)");
}

#[test]
fn test_is_null() {
    let clause = WhereClause::Field {
        path: vec!["deleted_at".to_string()],
        operator: WhereOperator::IsNull,
        value: json!(true),
    };

    let sql = WhereSqlGenerator::to_sql(&clause).unwrap();
    assert_eq!(sql, "data->>'deleted_at' IS NULL");
}

#[test]
fn test_is_not_null() {
    let clause = WhereClause::Field {
        path: vec!["updated_at".to_string()],
        operator: WhereOperator::IsNull,
        value: json!(false),
    };

    let sql = WhereSqlGenerator::to_sql(&clause).unwrap();
    assert_eq!(sql, "data->>'updated_at' IS NOT NULL");
}

#[test]
fn test_in_operator() {
    let clause = WhereClause::Field {
        path: vec!["status".to_string()],
        operator: WhereOperator::In,
        value: json!(["active", "pending", "approved"]),
    };

    let sql = WhereSqlGenerator::to_sql(&clause).unwrap();
    assert_eq!(sql, "data->>'status' = ANY ARRAY['active', 'pending', 'approved']");
}

#[test]
fn test_sql_injection_prevention() {
    let clause = WhereClause::Field {
        path: vec!["name".to_string()],
        operator: WhereOperator::Eq,
        value: json!("'; DROP TABLE users; --"),
    };

    let sql = WhereSqlGenerator::to_sql(&clause).unwrap();
    assert_eq!(sql, "data->>'name' = '''; DROP TABLE users; --'");
    // Single quotes are escaped to ''
}

#[test]
fn test_numeric_comparison() {
    let clause = WhereClause::Field {
        path: vec!["price".to_string()],
        operator: WhereOperator::Gt,
        value: json!(99.99),
    };

    let sql = WhereSqlGenerator::to_sql(&clause).unwrap();
    assert_eq!(sql, "data->>'price' > 99.99");
}

#[test]
fn test_boolean_value() {
    let clause = WhereClause::Field {
        path: vec!["published".to_string()],
        operator: WhereOperator::Eq,
        value: json!(true),
    };

    let sql = WhereSqlGenerator::to_sql(&clause).unwrap();
    assert_eq!(sql, "data->>'published' = true");
}

#[test]
fn test_empty_and_clause() {
    let clause = WhereClause::And(vec![]);
    let sql = WhereSqlGenerator::to_sql(&clause).unwrap();
    assert_eq!(sql, "TRUE");
}

#[test]
fn test_empty_or_clause() {
    let clause = WhereClause::Or(vec![]);
    let sql = WhereSqlGenerator::to_sql(&clause).unwrap();
    assert_eq!(sql, "FALSE");
}

#[test]
fn test_complex_nested_condition() {
    let clause = WhereClause::And(vec![
        WhereClause::Field {
            path: vec!["type".to_string()],
            operator: WhereOperator::Eq,
            value: json!("article"),
        },
        WhereClause::Or(vec![
            WhereClause::Field {
                path: vec!["status".to_string()],
                operator: WhereOperator::Eq,
                value: json!("published"),
            },
            WhereClause::And(vec![
                WhereClause::Field {
                    path: vec!["status".to_string()],
                    operator: WhereOperator::Eq,
                    value: json!("draft"),
                },
                WhereClause::Field {
                    path: vec!["author".to_string(), "role".to_string()],
                    operator: WhereOperator::Eq,
                    value: json!("admin"),
                },
            ]),
        ]),
    ]);

    let sql = WhereSqlGenerator::to_sql(&clause).unwrap();
    assert_eq!(
        sql,
        "(data->>'type' = 'article' AND (data->>'status' = 'published' OR (data->>'status' = 'draft' AND data#>'{author}'->>'role' = 'admin')))"
    );
}

#[test]
fn test_sql_injection_in_field_name_simple() {
    // Test that malicious field names are escaped to prevent SQL injection
    let clause = WhereClause::Field {
        path: vec!["name'; DROP TABLE users; --".to_string()],
        operator: WhereOperator::Eq,
        value: json!("value"),
    };

    let sql = WhereSqlGenerator::to_sql(&clause).unwrap();
    // Field name should be escaped with doubled single quotes
    // Result: data->>'name''; DROP TABLE users; --' = 'value'
    // The doubled '' prevents the quote from closing the string
    assert!(sql.contains("''")); // Escaped quotes present
    // The SQL structure should be: identifier->>'field' operator value
    // With escaping, DROP TABLE becomes part of the field string, not executable
    assert!(sql.contains("data->>'"));
    assert!(sql.contains("= 'value'")); // Proper value comparison
}

#[test]
fn test_sql_injection_prevention_in_array_operator() {
    // SECURITY: Ensure JSON injection in array operators is escaped
    let clause = WhereClause::Field {
        path: vec!["tags".to_string()],
        operator: WhereOperator::ArrayContains,
        value: json!(["normal", "'; DROP TABLE users; --"]),
    };

    let sql = WhereSqlGenerator::to_sql(&clause).unwrap();
    // The JSON serializer will escape the inner quotes, and we escape SQL single quotes.
    // The result should be a properly escaped JSONB literal, not executable SQL.
    assert!(sql.contains("::jsonb"), "Must produce valid JSONB cast");
    // Verify the value is inside a JSON string (double-quoted), not a raw SQL string.
    // serde_json serializes this as: ["normal","'; DROP TABLE users; --"]
    // After SQL escaping: ["normal","''; DROP TABLE users; --"]
    // The single quote inside the JSON value is doubled for SQL safety.
    assert!(
        sql.contains("''"),
        "Single quotes inside JSON values must be doubled for SQL safety"
    );
}

#[test]
fn test_sql_injection_in_nested_field_name() {
    // Test that malicious nested field names are also escaped
    let clause = WhereClause::Field {
        path: vec![
            "user".to_string(),
            "role'; DROP TABLE users; --".to_string(),
        ],
        operator: WhereOperator::Eq,
        value: json!("admin"),
    };

    let sql = WhereSqlGenerator::to_sql(&clause).unwrap();
    // Both simple and nested path components should be escaped
    assert!(sql.contains("''")); // Escaped quotes present
    assert!(sql.contains("data#>'{")); // Nested path syntax
}

#[test]
fn escape_sql_string_rejects_oversized_input() {
    let large = "a".repeat(MAX_SQL_VALUE_BYTES + 1);
    let result = WhereSqlGenerator::escape_sql_string(&large);
    assert!(matches!(result, Err(FraiseQLError::Validation { .. })));
}

#[test]
fn escape_sql_string_accepts_exactly_max_bytes() {
    let at_limit = "a".repeat(MAX_SQL_VALUE_BYTES);
    WhereSqlGenerator::escape_sql_string(&at_limit)
        .unwrap_or_else(|e| panic!("expected Ok for string at exactly MAX_SQL_VALUE_BYTES: {e}"));
}

#[test]
fn escape_sql_string_escapes_single_quotes() {
    let result = WhereSqlGenerator::escape_sql_string("it's").unwrap();
    assert_eq!(result, "it''s");
}

#[test]
fn value_to_sql_rejects_oversized_string_value() {
    let large = "a".repeat(MAX_SQL_VALUE_BYTES + 1);
    let clause = WhereClause::Field {
        path: vec!["name".to_string()],
        operator: WhereOperator::Eq,
        value: json!(large),
    };
    assert!(matches!(
        WhereSqlGenerator::to_sql(&clause),
        Err(FraiseQLError::Validation { .. })
    ));
}

#[test]
fn value_to_sql_rejects_oversized_jsonb_value() {
    // Build an array large enough to exceed MAX_SQL_VALUE_BYTES when serialized
    let large_element = "a".repeat(MAX_SQL_VALUE_BYTES);
    let clause = WhereClause::Field {
        path: vec!["tags".to_string()],
        operator: WhereOperator::ArrayContains,
        value: json!([large_element]),
    };
    assert!(matches!(
        WhereSqlGenerator::to_sql(&clause),
        Err(FraiseQLError::Validation { .. })
    ));
}
