#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use serde_json::json;

use super::*;
use crate::{
    dialect::MySqlDialect,
    where_clause::{WhereClause, WhereOperator},
};

#[test]
fn test_simple_equality() {
    let gen = MySqlWhereGenerator::new(MySqlDialect);
    let clause = WhereClause::Field {
        path: vec!["email".to_string()],
        operator: WhereOperator::Eq,
        value: json!("test@example.com"),
    };

    let (sql, params) = gen.generate(&clause).unwrap();
    assert!(sql.contains("JSON_UNQUOTE"), "Expected JSON_UNQUOTE: {sql}");
    assert!(sql.contains("= ?"), "Expected = ?: {sql}");
    assert_eq!(params, vec![json!("test@example.com")]);
}

#[test]
fn test_icontains() {
    let gen = MySqlWhereGenerator::new(MySqlDialect);
    let clause = WhereClause::Field {
        path: vec!["name".to_string()],
        operator: WhereOperator::Icontains,
        value: json!("alice"),
    };

    let (sql, params) = gen.generate(&clause).unwrap();
    assert!(sql.contains("CONCAT"), "Expected CONCAT: {sql}");
    assert_eq!(params, vec![json!("alice")]);
}

#[test]
fn test_placeholders_are_question_marks() {
    let gen = MySqlWhereGenerator::new(MySqlDialect);
    let clause = WhereClause::And(vec![
        WhereClause::Field {
            path: vec!["a".to_string()],
            operator: WhereOperator::Eq,
            value: json!("x"),
        },
        WhereClause::Field {
            path: vec!["b".to_string()],
            operator: WhereOperator::Eq,
            value: json!("y"),
        },
    ]);

    let (sql, params) = gen.generate(&clause).unwrap();
    assert!(!sql.contains("$1"), "MySQL must not use $N placeholders: {sql}");
    assert_eq!(params.len(), 2);
}
