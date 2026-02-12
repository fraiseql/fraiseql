//! LTree operator tests for PostgreSQL hierarchical data.
//!
//! This test verifies that LTree operators (PostgreSQL-specific) work correctly on PostgreSQL
//! and return appropriate errors on other databases.
//!
//! LTree operators query hierarchical tree structures:
//! - AncestorOf: Check if path is ancestor of another
//! - DescendantOf: Check if path is descendant of another
//! - MatchesLquery: Match path against lquery pattern
//! - MatchesLtxtquery: Match path against ltxtquery pattern
//! - DepthEq/Neq/Gt/Gte/Lt/Lte: Check depth (number of labels)
//! - Lca: Lowest common ancestor

use fraiseql_core::db::where_clause::{WhereClause, WhereOperator};
use fraiseql_core::db::where_sql_generator::WhereSqlGenerator;
use fraiseql_core::db::types::DatabaseType;
use serde_json::json;

// ============================================================================
// RED PHASE: Tests that should pass when LTree operators are implemented
// ============================================================================

/// Test that LTree operators work on PostgreSQL
#[test]
fn test_ltree_operators_work_on_postgresql() {
    let ltree_operators = vec![
        (WhereOperator::AncestorOf, "AncestorOf"),
        (WhereOperator::DescendantOf, "DescendantOf"),
        (WhereOperator::MatchesLquery, "MatchesLquery"),
        (WhereOperator::MatchesLtxtquery, "MatchesLtxtquery"),
        (WhereOperator::DepthEq, "DepthEq"),
    ];

    for (operator, op_name) in ltree_operators {
        let clause = WhereClause::Field {
            path: vec!["hierarchy".to_string()],
            operator,
            value: json!("1.2.3"),
        };

        let result = WhereSqlGenerator::to_sql_for_db(&clause, DatabaseType::PostgreSQL);

        assert!(
            result.is_ok(),
            "Operator {} should work on PostgreSQL, got error: {:?}",
            op_name,
            result
        );

        let sql = result.unwrap();
        assert!(
            !sql.is_empty(),
            "Operator {} should generate SQL on PostgreSQL",
            op_name
        );
    }
}

/// Test that LTree operators return errors on non-PostgreSQL databases
#[test]
fn test_ltree_operators_return_errors_on_non_postgres() {
    let ltree_operators = vec![
        (WhereOperator::AncestorOf, "AncestorOf"),
        (WhereOperator::DescendantOf, "DescendantOf"),
        (WhereOperator::MatchesLquery, "MatchesLquery"),
    ];

    let non_postgres_databases = vec![
        (DatabaseType::MySQL, "MySQL"),
        (DatabaseType::SQLite, "SQLite"),
        (DatabaseType::SQLServer, "SQL Server"),
    ];

    for (operator, op_name) in &ltree_operators {
        for (db_type, db_name) in &non_postgres_databases {
            let clause = WhereClause::Field {
                path: vec!["hierarchy".to_string()],
                operator: operator.clone(),
                value: json!("1.2.3"),
            };

            let result = WhereSqlGenerator::to_sql_for_db(&clause, *db_type);

            assert!(
                result.is_err(),
                "Operator {} should NOT be supported on {}, but got: {:?}",
                op_name,
                db_name,
                result
            );

            let error_msg = format!("{:?}", result.err());
            assert!(
                error_msg.contains("PostgreSQL") || error_msg.contains("not supported"),
                "Error should mention PostgreSQL requirement, got: {}",
                error_msg
            );
        }
    }
}

/// Test AncestorOf operator specifically on PostgreSQL
#[test]
fn test_ltree_ancestor_of_postgresql() {
    let clause = WhereClause::Field {
        path: vec!["path".to_string()],
        operator: WhereOperator::AncestorOf,
        value: json!("1.2.3"),
    };

    let sql = WhereSqlGenerator::to_sql_for_db(&clause, DatabaseType::PostgreSQL)
        .expect("AncestorOf should work on PostgreSQL");

    // Should contain LTree operator @> (is ancestor of)
    assert!(
        sql.contains("@>") || sql.contains("ancestor"),
        "AncestorOf should use @ > operator or mention ancestor, got: {}",
        sql
    );
}

/// Test DescendantOf operator specifically on PostgreSQL
#[test]
fn test_ltree_descendant_of_postgresql() {
    let clause = WhereClause::Field {
        path: vec!["path".to_string()],
        operator: WhereOperator::DescendantOf,
        value: json!("1.2.3"),
    };

    let sql = WhereSqlGenerator::to_sql_for_db(&clause, DatabaseType::PostgreSQL)
        .expect("DescendantOf should work on PostgreSQL");

    // Should contain LTree operator <@ (is descendant of)
    assert!(
        sql.contains("<@") || sql.contains("descendant"),
        "DescendantOf should use <@ operator or mention descendant, got: {}",
        sql
    );
}

/// Test depth comparison operators on PostgreSQL
#[test]
fn test_ltree_depth_operators_postgresql() {
    let depth_operators = vec![
        (WhereOperator::DepthEq, "DepthEq", "="),
        (WhereOperator::DepthNeq, "DepthNeq", "!="),
        (WhereOperator::DepthGt, "DepthGt", ">"),
        (WhereOperator::DepthGte, "DepthGte", ">="),
        (WhereOperator::DepthLt, "DepthLt", "<"),
        (WhereOperator::DepthLte, "DepthLte", "<="),
    ];

    for (operator, op_name, expected_op) in depth_operators {
        let clause = WhereClause::Field {
            path: vec!["path".to_string()],
            operator,
            value: json!(3),
        };

        let sql = WhereSqlGenerator::to_sql_for_db(&clause, DatabaseType::PostgreSQL)
            .expect(&format!("{} should work on PostgreSQL", op_name));

        // Should contain nlevel function and comparison operator
        assert!(
            sql.contains("nlevel") || sql.contains("depth"),
            "Depth operator {} should use nlevel function or mention depth, got: {}",
            op_name,
            sql
        );
        assert!(
            sql.contains(expected_op),
            "Operator {} should contain comparison '{}', got: {}",
            op_name,
            expected_op,
            sql
        );
    }
}

/// Test pattern matching operators on PostgreSQL
#[test]
fn test_ltree_pattern_matching_postgresql() {
    let pattern_operators = vec![
        (WhereOperator::MatchesLquery, "MatchesLquery", "~"),
        (WhereOperator::MatchesLtxtquery, "MatchesLtxtquery", "?"),
    ];

    for (operator, op_name, expected_op) in pattern_operators {
        let clause = WhereClause::Field {
            path: vec!["path".to_string()],
            operator,
            value: json!("1.*.3"),
        };

        let sql = WhereSqlGenerator::to_sql_for_db(&clause, DatabaseType::PostgreSQL)
            .expect(&format!("{} should work on PostgreSQL", op_name));

        assert!(
            sql.contains(expected_op) || sql.contains("match"),
            "Pattern operator {} should use '{}' or mention match, got: {}",
            op_name,
            expected_op,
            sql
        );
    }
}

/// Test that all 12 LTree operators have error handling on MySQL
#[test]
fn test_all_ltree_operators_blocked_on_mysql() {
    let ltree_operators = vec![
        (WhereOperator::AncestorOf, "AncestorOf"),
        (WhereOperator::DescendantOf, "DescendantOf"),
        (WhereOperator::MatchesLquery, "MatchesLquery"),
        (WhereOperator::MatchesLtxtquery, "MatchesLtxtquery"),
        (WhereOperator::MatchesAnyLquery, "MatchesAnyLquery"),
        (WhereOperator::DepthEq, "DepthEq"),
        (WhereOperator::DepthNeq, "DepthNeq"),
        (WhereOperator::DepthGt, "DepthGt"),
        (WhereOperator::DepthGte, "DepthGte"),
        (WhereOperator::DepthLt, "DepthLt"),
        (WhereOperator::DepthLte, "DepthLte"),
        (WhereOperator::Lca, "Lca"),
    ];

    for (operator, op_name) in ltree_operators {
        let clause = WhereClause::Field {
            path: vec!["path".to_string()],
            operator,
            value: json!("1.2.3"),
        };

        let result = WhereSqlGenerator::to_sql_for_db(&clause, DatabaseType::MySQL);

        assert!(
            result.is_err(),
            "LTree operator {} should be rejected on MySQL",
            op_name
        );
    }
}

/// Test error messages for PostgreSQL-only operators
#[test]
fn test_ltree_error_messages_helpful() {
    let clause = WhereClause::Field {
        path: vec!["path".to_string()],
        operator: WhereOperator::AncestorOf,
        value: json!("1.2.3"),
    };

    let result = WhereSqlGenerator::to_sql_for_db(&clause, DatabaseType::SQLite);
    assert!(result.is_err(), "AncestorOf should error on SQLite");

    let error_msg = format!("{:?}", result.err());
    assert!(
        error_msg.contains("PostgreSQL") || error_msg.contains("LTree"),
        "Error should mention PostgreSQL or LTree requirement, got: {}",
        error_msg
    );
}
