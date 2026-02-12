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

// ============================================================================
// PHASE 3, CYCLE 2: LTree Array Operations
// ============================================================================

/// Test MatchesAnyLquery with array of patterns
#[test]
fn test_ltree_matches_any_lquery_with_array() {
    // MatchesAnyLquery should handle arrays of lquery patterns
    let clause = WhereClause::Field {
        path: vec!["path".to_string()],
        operator: WhereOperator::MatchesAnyLquery,
        value: json!(["1.*", "2.*", "3.*"]),  // Array of patterns
    };

    let sql = WhereSqlGenerator::to_sql_for_db(&clause, DatabaseType::PostgreSQL)
        .expect("MatchesAnyLquery should work with array on PostgreSQL");

    // Should contain ~ ANY and array syntax
    assert!(
        sql.contains("~") && (sql.contains("ANY") || sql.contains("array")),
        "MatchesAnyLquery should use ~ ANY for array patterns, got: {}",
        sql
    );
}

/// Test MatchesAnyLquery returns error on non-PostgreSQL with array
#[test]
fn test_ltree_matches_any_lquery_blocked_on_mysql() {
    let clause = WhereClause::Field {
        path: vec!["path".to_string()],
        operator: WhereOperator::MatchesAnyLquery,
        value: json!(["1.*", "2.*"]),
    };

    let result = WhereSqlGenerator::to_sql_for_db(&clause, DatabaseType::MySQL);
    assert!(
        result.is_err(),
        "MatchesAnyLquery should not work on MySQL even with array"
    );

    let error_msg = format!("{:?}", result.err());
    assert!(
        error_msg.contains("PostgreSQL"),
        "Error should mention PostgreSQL requirement, got: {}",
        error_msg
    );
}

/// Test that LTree operators handle both scalar and array values
#[test]
fn test_ltree_scalar_vs_array_values() {
    // Scalar pattern
    let scalar_clause = WhereClause::Field {
        path: vec!["path".to_string()],
        operator: WhereOperator::MatchesLquery,
        value: json!("1.2.*"),
    };

    let scalar_sql = WhereSqlGenerator::to_sql_for_db(&scalar_clause, DatabaseType::PostgreSQL)
        .expect("MatchesLquery should work with scalar pattern");

    assert!(
        scalar_sql.contains("~"),
        "Scalar pattern should use ~ operator, got: {}",
        scalar_sql
    );

    // Array pattern (with MatchesAnyLquery)
    let array_clause = WhereClause::Field {
        path: vec!["path".to_string()],
        operator: WhereOperator::MatchesAnyLquery,
        value: json!(["1.2.*", "2.3.*"]),
    };

    let array_sql = WhereSqlGenerator::to_sql_for_db(&array_clause, DatabaseType::PostgreSQL)
        .expect("MatchesAnyLquery should work with array");

    assert!(
        array_sql.contains("~") && array_sql.contains("ANY"),
        "Array pattern should use ~ ANY, got: {}",
        array_sql
    );
}

/// Test that array-based operators preserve array syntax
#[test]
fn test_ltree_array_syntax_preserved() {
    let clause = WhereClause::Field {
        path: vec!["tags".to_string()],
        operator: WhereOperator::MatchesAnyLquery,
        value: json!(["root.*", "section1.*"]),
    };

    let sql = WhereSqlGenerator::to_sql_for_db(&clause, DatabaseType::PostgreSQL)
        .expect("Should generate SQL for array patterns");

    // SQL should preserve array structure for PostgreSQL
    assert!(
        sql.contains("ANY") || sql.contains("array"),
        "Should use ANY operator for array matching in PostgreSQL, got: {}",
        sql
    );
}

// ============================================================================
// PHASE 3, CYCLE 3: Comprehensive LTree Testing (12 operators × 1 database)
// ============================================================================

/// Comprehensive test matrix: All 12 LTree operators on PostgreSQL
#[test]
fn test_all_12_ltree_operators_on_postgresql() {
    let ltree_operators = vec![
        (WhereOperator::AncestorOf, "AncestorOf", "@>"),
        (WhereOperator::DescendantOf, "DescendantOf", "<@"),
        (WhereOperator::MatchesLquery, "MatchesLquery", "~"),
        (WhereOperator::MatchesLtxtquery, "MatchesLtxtquery", "?"),
        (WhereOperator::MatchesAnyLquery, "MatchesAnyLquery", "~"),
        (WhereOperator::DepthEq, "DepthEq", "nlevel"),
        (WhereOperator::DepthNeq, "DepthNeq", "nlevel"),
        (WhereOperator::DepthGt, "DepthGt", "nlevel"),
        (WhereOperator::DepthGte, "DepthGte", "nlevel"),
        (WhereOperator::DepthLt, "DepthLt", "nlevel"),
        (WhereOperator::DepthLte, "DepthLte", "nlevel"),
        (WhereOperator::Lca, "Lca", "lca"),
    ];

    let mut success_count = 0;
    let mut failures = Vec::new();

    for (operator, op_name, expected_keyword) in ltree_operators {
        let clause = WhereClause::Field {
            path: vec!["path".to_string()],
            operator,
            value: json!("1.2.3"),
        };

        match WhereSqlGenerator::to_sql_for_db(&clause, DatabaseType::PostgreSQL) {
            Ok(sql) => {
                // Verify SQL contains expected PostgreSQL-specific keyword
                if sql.contains(expected_keyword) || sql.contains("nlevel") || sql.contains("lca") {
                    success_count += 1;
                } else {
                    failures.push(format!(
                        "{}: SQL missing expected '{}', got: {}",
                        op_name, expected_keyword, sql
                    ));
                }
            },
            Err(e) => {
                failures.push(format!("{}: Failed to generate SQL: {:?}", op_name, e));
            },
        }
    }

    eprintln!(
        "\n📊 LTree Operator Test Matrix Results:\n   ✅ {} successes (out of 12)\n   ❌ {} failures",
        success_count,
        failures.len()
    );

    if !failures.is_empty() {
        eprintln!("\nFailures:");
        for failure in &failures {
            eprintln!("  - {}", failure);
        }
    }

    assert_eq!(
        success_count, 12,
        "Expected all 12 LTree operators to work on PostgreSQL"
    );
}

/// Test that LTree operators generate different SQL for different operations
#[test]
fn test_ltree_operators_generate_distinct_sql() {
    // Each operator should generate distinct SQL based on its semantics
    let test_value = "1.2.3";

    // Ancestor check
    let ancestor_clause = WhereClause::Field {
        path: vec!["path".to_string()],
        operator: WhereOperator::AncestorOf,
        value: json!(test_value),
    };
    let ancestor_sql = WhereSqlGenerator::to_sql_for_db(&ancestor_clause, DatabaseType::PostgreSQL)
        .expect("AncestorOf should work");

    // Descendant check (should be different)
    let descendant_clause = WhereClause::Field {
        path: vec!["path".to_string()],
        operator: WhereOperator::DescendantOf,
        value: json!(test_value),
    };
    let descendant_sql = WhereSqlGenerator::to_sql_for_db(&descendant_clause, DatabaseType::PostgreSQL)
        .expect("DescendantOf should work");

    // SQLs should be different
    assert!(
        ancestor_sql != descendant_sql,
        "AncestorOf and DescendantOf should generate different SQL"
    );

    // Depth operators should also differ
    let depth_eq_clause = WhereClause::Field {
        path: vec!["path".to_string()],
        operator: WhereOperator::DepthEq,
        value: json!(3),
    };
    let depth_eq_sql = WhereSqlGenerator::to_sql_for_db(&depth_eq_clause, DatabaseType::PostgreSQL)
        .expect("DepthEq should work");

    let depth_gt_clause = WhereClause::Field {
        path: vec!["path".to_string()],
        operator: WhereOperator::DepthGt,
        value: json!(3),
    };
    let depth_gt_sql = WhereSqlGenerator::to_sql_for_db(&depth_gt_clause, DatabaseType::PostgreSQL)
        .expect("DepthGt should work");

    assert!(
        depth_eq_sql != depth_gt_sql,
        "DepthEq and DepthGt should generate different SQL"
    );
}

/// Test that all 12 LTree operators are blocked on all non-PostgreSQL databases
#[test]
fn test_all_ltree_operators_blocked_on_non_postgres() {
    let ltree_operators = vec![
        WhereOperator::AncestorOf,
        WhereOperator::DescendantOf,
        WhereOperator::MatchesLquery,
        WhereOperator::MatchesLtxtquery,
        WhereOperator::MatchesAnyLquery,
        WhereOperator::DepthEq,
        WhereOperator::DepthNeq,
        WhereOperator::DepthGt,
        WhereOperator::DepthGte,
        WhereOperator::DepthLt,
        WhereOperator::DepthLte,
        WhereOperator::Lca,
    ];

    let non_postgres_dbs = vec![DatabaseType::MySQL, DatabaseType::SQLite, DatabaseType::SQLServer];

    for operator in &ltree_operators {
        for db_type in &non_postgres_dbs {
            let clause = WhereClause::Field {
                path: vec!["path".to_string()],
                operator: operator.clone(),
                value: json!("1.2.3"),
            };

            let result = WhereSqlGenerator::to_sql_for_db(&clause, *db_type);
            assert!(
                result.is_err(),
                "LTree operator {:?} should be blocked on {:?}",
                operator,
                db_type
            );
        }
    }
}

/// Integration test: verify LTree operators work with nested paths
#[test]
fn test_ltree_operators_with_nested_paths() {
    let clause = WhereClause::Field {
        path: vec!["metadata".to_string(), "hierarchy".to_string()],
        operator: WhereOperator::AncestorOf,
        value: json!("1.2.3"),
    };

    let sql = WhereSqlGenerator::to_sql_for_db(&clause, DatabaseType::PostgreSQL)
        .expect("Should work with nested paths");

    // Should reference the nested field correctly
    assert!(
        sql.contains("metadata") || sql.contains("hierarchy"),
        "SQL should contain reference to nested path, got: {}",
        sql
    );
}
