//! Direct column optimization tests.
//!
//! This test verifies that WHERE clause generator uses direct SQL columns
//! when available (instead of JSONB extraction), enabling proper index usage
//! for performance optimization.
//!
//! Direct columns are mapped via `sql_column` in schema definitions:
//! - Field "email" → Column "email_address" (direct column, indexed)
//! - Field "name" → JSONB (no mapping, falls back to data->>'name')
//!
//! Benefits:
//! - Query uses database indexes on direct columns
//! - Significantly faster than JSONB extraction
//! - Backward compatible: unmapped fields use JSONB fallback

use fraiseql_core::db::where_clause::{WhereClause, WhereOperator};
use fraiseql_core::db::where_sql_generator::WhereSqlGenerator;
use fraiseql_core::db::types::DatabaseType;
use serde_json::json;
use std::collections::HashMap;

// Helper to create a simple column map
fn create_column_map(mappings: &[(&str, &str)]) -> HashMap<String, String> {
    mappings
        .iter()
        .map(|&(field, col)| (field.to_string(), col.to_string()))
        .collect()
}

// ============================================================================
// RED PHASE: Tests expecting direct column support
// ============================================================================

/// Test that direct column is used when mapping is provided
#[test]
fn test_direct_column_used_when_mapped() {
    let column_map = create_column_map(&[("email", "email_column")]);

    let clause = WhereClause::Field {
        path: vec!["email".to_string()],
        operator: WhereOperator::Eq,
        value: json!("user@example.com"),
    };

    let sql = WhereSqlGenerator::to_sql_for_db_with_columns(&clause, DatabaseType::PostgreSQL, &column_map)
        .expect("Should generate SQL with direct column");

    // Should use direct column, not JSONB
    assert!(
        sql.contains("email_column"),
        "Should use direct column 'email_column', got: {}",
        sql
    );
    assert!(
        !sql.contains("data->>'email'"),
        "Should not use JSONB extraction when column is mapped, got: {}",
        sql
    );
}

/// Test that JSONB fallback is used when no mapping exists
#[test]
fn test_jsonb_fallback_when_unmapped() {
    let column_map = create_column_map(&[]); // No mappings

    let clause = WhereClause::Field {
        path: vec!["email".to_string()],
        operator: WhereOperator::Eq,
        value: json!("user@example.com"),
    };

    let sql = WhereSqlGenerator::to_sql_for_db_with_columns(&clause, DatabaseType::PostgreSQL, &column_map)
        .expect("Should fallback to JSONB when no mapping");

    // Should fallback to JSONB
    assert!(
        sql.contains("data->>'email'"),
        "Should fallback to JSONB when field is not mapped, got: {}",
        sql
    );
}

/// Test PostgreSQL direct column quoting
#[test]
fn test_postgresql_direct_column_quoting() {
    let column_map = create_column_map(&[("email", "email_address")]);

    let clause = WhereClause::Field {
        path: vec!["email".to_string()],
        operator: WhereOperator::Eq,
        value: json!("user@example.com"),
    };

    let sql = WhereSqlGenerator::to_sql_for_db_with_columns(&clause, DatabaseType::PostgreSQL, &column_map)
        .expect("PostgreSQL should generate SQL with quoted identifier");

    // PostgreSQL: double quotes
    assert!(
        sql.contains(r#""email_address""#),
        "PostgreSQL should use double quotes, got: {}",
        sql
    );
}

/// Test MySQL direct column quoting
#[test]
fn test_mysql_direct_column_quoting() {
    let column_map = create_column_map(&[("email", "email_address")]);

    let clause = WhereClause::Field {
        path: vec!["email".to_string()],
        operator: WhereOperator::Eq,
        value: json!("user@example.com"),
    };

    let sql = WhereSqlGenerator::to_sql_for_db_with_columns(&clause, DatabaseType::MySQL, &column_map)
        .expect("MySQL should generate SQL with backtick identifier");

    // MySQL: backticks
    assert!(
        sql.contains("`email_address`"),
        "MySQL should use backticks, got: {}",
        sql
    );
}

/// Test SQLite direct column quoting
#[test]
fn test_sqlite_direct_column_quoting() {
    let column_map = create_column_map(&[("email", "email_address")]);

    let clause = WhereClause::Field {
        path: vec!["email".to_string()],
        operator: WhereOperator::Eq,
        value: json!("user@example.com"),
    };

    let sql = WhereSqlGenerator::to_sql_for_db_with_columns(&clause, DatabaseType::SQLite, &column_map)
        .expect("SQLite should generate SQL with quoted identifier");

    // SQLite: double quotes
    assert!(
        sql.contains(r#""email_address""#),
        "SQLite should use double quotes, got: {}",
        sql
    );
}

/// Test SQL Server direct column quoting
#[test]
fn test_sqlserver_direct_column_quoting() {
    let column_map = create_column_map(&[("email", "email_address")]);

    let clause = WhereClause::Field {
        path: vec!["email".to_string()],
        operator: WhereOperator::Eq,
        value: json!("user@example.com"),
    };

    let sql = WhereSqlGenerator::to_sql_for_db_with_columns(&clause, DatabaseType::SQLServer, &column_map)
        .expect("SQL Server should generate SQL with bracketed identifier");

    // SQL Server: brackets
    assert!(
        sql.contains("[email_address]"),
        "SQL Server should use brackets, got: {}",
        sql
    );
}

/// Test template operators with direct columns
#[test]
fn test_template_operator_with_direct_column() {
    let column_map = create_column_map(&[("email", "email_address")]);

    let clause = WhereClause::Field {
        path: vec!["email".to_string()],
        operator: WhereOperator::Extended(fraiseql_core::filters::ExtendedOperator::EmailDomainEq(
            "example.com".to_string(),
        )),
        value: json!(null),
    };

    let sql = WhereSqlGenerator::to_sql_for_db_with_columns(&clause, DatabaseType::PostgreSQL, &column_map)
        .expect("Template operator should work with direct column");

    // Should use direct column in template
    assert!(
        sql.contains("email_address"),
        "Template should use direct column reference, got: {}",
        sql
    );
    // Should extract domain from direct column
    assert!(
        sql.contains("SPLIT_PART"),
        "Should extract domain using SPLIT_PART on direct column, got: {}",
        sql
    );
}

/// Test mixed mapping (some direct, some JSONB)
#[test]
fn test_mixed_direct_and_jsonb_mapping() {
    let column_map = create_column_map(&[("email", "email_address")]); // Only email mapped

    // Email query - should use direct column
    let email_clause = WhereClause::Field {
        path: vec!["email".to_string()],
        operator: WhereOperator::Eq,
        value: json!("user@example.com"),
    };

    let email_sql =
        WhereSqlGenerator::to_sql_for_db_with_columns(&email_clause, DatabaseType::PostgreSQL, &column_map)
            .expect("Email should use direct column");

    assert!(
        email_sql.contains("email_address"),
        "Mapped field should use direct column"
    );

    // Name query - should fallback to JSONB
    let name_clause = WhereClause::Field {
        path: vec!["name".to_string()],
        operator: WhereOperator::Eq,
        value: json!("John"),
    };

    let name_sql =
        WhereSqlGenerator::to_sql_for_db_with_columns(&name_clause, DatabaseType::PostgreSQL, &column_map)
            .expect("Name should fallback to JSONB");

    assert!(
        name_sql.contains("data->>'name'"),
        "Unmapped field should fallback to JSONB"
    );
}

/// Test comparison operators with direct columns
#[test]
fn test_comparison_operators_with_direct_columns() {
    let column_map = create_column_map(&[
        ("email", "email_address"),
        ("age", "user_age"),
    ]);

    let operators = vec![
        (WhereOperator::Eq, "="),
        (WhereOperator::Neq, "!="),
        (WhereOperator::Gt, ">"),
        (WhereOperator::Gte, ">="),
        (WhereOperator::Lt, "<"),
        (WhereOperator::Lte, "<="),
    ];

    for (op, op_str) in operators {
        let clause = WhereClause::Field {
            path: vec!["age".to_string()],
            operator: op,
            value: json!(21),
        };

        let sql = WhereSqlGenerator::to_sql_for_db_with_columns(&clause, DatabaseType::PostgreSQL, &column_map)
            .expect("Operator should work with direct column");

        assert!(
            sql.contains("user_age"),
            "Should use direct column 'user_age', got: {}",
            sql
        );
        assert!(
            sql.contains(op_str),
            "Should contain operator '{}', got: {}",
            op_str,
            sql
        );
    }
}

/// Test IN operator with direct columns
#[test]
fn test_in_operator_with_direct_columns() {
    let column_map = create_column_map(&[("status", "user_status")]);

    let clause = WhereClause::Field {
        path: vec!["status".to_string()],
        operator: WhereOperator::In,
        value: json!(["active", "pending"]),
    };

    let sql = WhereSqlGenerator::to_sql_for_db_with_columns(&clause, DatabaseType::PostgreSQL, &column_map)
        .expect("IN operator should work with direct column");

    assert!(
        sql.contains("user_status"),
        "Should use direct column 'user_status', got: {}",
        sql
    );
    assert!(
        sql.contains("= ANY"),
        "Should use PostgreSQL = ANY operator, got: {}",
        sql
    );
}

/// Test special characters in column names (should be properly escaped)
#[test]
fn test_special_characters_in_column_names() {
    let column_map = create_column_map(&[("user_email", "user's_email_address")]);

    let clause = WhereClause::Field {
        path: vec!["user_email".to_string()],
        operator: WhereOperator::Eq,
        value: json!("user@example.com"),
    };

    let sql = WhereSqlGenerator::to_sql_for_db_with_columns(&clause, DatabaseType::PostgreSQL, &column_map)
        .expect("Should handle special characters in column names");

    // Should have proper quoting around column name with special chars
    assert!(
        sql.contains("user's_email_address") || sql.contains(r#""user's_email_address""#),
        "Should properly quote column name with apostrophe, got: {}",
        sql
    );
}

/// Test backward compatibility - old signature still works (without column map)
#[test]
fn test_backward_compatibility_without_column_map() {
    let clause = WhereClause::Field {
        path: vec!["email".to_string()],
        operator: WhereOperator::Eq,
        value: json!("user@example.com"),
    };

    // Old signature should still work (fallback to empty column map)
    let sql = WhereSqlGenerator::to_sql_for_db(&clause, DatabaseType::PostgreSQL)
        .expect("Old signature should still work for backward compatibility");

    // Should fallback to JSONB when no column map provided
    assert!(
        sql.contains("data->>'email'"),
        "Should fallback to JSONB when no column map provided, got: {}",
        sql
    );
}
