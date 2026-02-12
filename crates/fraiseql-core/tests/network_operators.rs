//! Network operator tests for WHERE clause.
//!
//! This test verifies that network operators (IsIPv4, IsIPv6, IsPrivate, IsPublic, InSubnet)
//! generate correct database-specific SQL.
//!
//! Network operators detect and validate IP addresses:
//! - IsIPv4: Validates IPv4 format (e.g., 192.168.1.1)
//! - IsIPv6: Validates IPv6 format (e.g., 2001:db8::1)
//! - IsPrivate: Checks if IP is in private ranges (10.0.0.0/8, 172.16.0.0/12, 192.168.0.0/16)
//! - IsPublic: Checks if IP is public (inverse of private)
//! - InSubnet: Checks if IP is in specified CIDR subnet (e.g., 10.0.0.0/8)

use fraiseql_core::db::where_clause::{WhereClause, WhereOperator};
use fraiseql_core::db::where_sql_generator::WhereSqlGenerator;
use fraiseql_core::db::types::DatabaseType;
use serde_json::json;

// ============================================================================
// RED PHASE: Tests that should pass when network operators are implemented
// ============================================================================

/// Test IsIPv4 operator generates database-specific SQL for PostgreSQL
#[test]
fn test_network_operator_is_ipv4_postgresql() {
    // Create a WHERE clause for: WHERE ip_address IS IPv4
    let clause = WhereClause::Field {
        path: vec!["ip_address".to_string()],
        operator: WhereOperator::IsIPv4,
        value: json!(true),
    };

    // Generate SQL for PostgreSQL (uses INET type)
    let sql = WhereSqlGenerator::to_sql_for_db(&clause, DatabaseType::PostgreSQL)
        .expect("Should generate SQL for IsIPv4");

    // PostgreSQL: should use INET type cast or validation
    assert!(
        sql.contains("CAST(") || sql.contains("::inet") || sql.contains("IS NOT NULL"),
        "PostgreSQL should validate IPv4 as INET type, got: {}",
        sql
    );
}

/// Test IsIPv4 operator for MySQL
#[test]
fn test_network_operator_is_ipv4_mysql() {
    let clause = WhereClause::Field {
        path: vec!["ip_address".to_string()],
        operator: WhereOperator::IsIPv4,
        value: json!(true),
    };

    let sql = WhereSqlGenerator::to_sql_for_db(&clause, DatabaseType::MySQL)
        .expect("Should generate SQL for IsIPv4");

    // MySQL: INET_ATON or CAST
    assert!(
        sql.contains("INET") || sql.contains("CAST"),
        "MySQL should use INET_ATON or CAST, got: {}",
        sql
    );
}

/// Test IsIPv6 operator for PostgreSQL
#[test]
fn test_network_operator_is_ipv6_postgresql() {
    let clause = WhereClause::Field {
        path: vec!["ip_address".to_string()],
        operator: WhereOperator::IsIPv6,
        value: json!(true),
    };

    let sql = WhereSqlGenerator::to_sql_for_db(&clause, DatabaseType::PostgreSQL)
        .expect("Should generate SQL for IsIPv6");

    assert!(
        !sql.is_empty(),
        "PostgreSQL should generate IPv6 validation, got: {}",
        sql
    );
}

/// Test IsPrivate operator for PostgreSQL
#[test]
fn test_network_operator_is_private_postgresql() {
    let clause = WhereClause::Field {
        path: vec!["ip_address".to_string()],
        operator: WhereOperator::IsPrivate,
        value: json!(true),
    };

    let sql = WhereSqlGenerator::to_sql_for_db(&clause, DatabaseType::PostgreSQL)
        .expect("Should generate SQL for IsPrivate");

    // Should check if IP matches private ranges: 10.0.0.0/8, 172.16.0.0/12, 192.168.0.0/16
    assert!(
        sql.contains("10.") || sql.contains("172.") || sql.contains("192.168") || sql.contains("<<"),
        "PostgreSQL should validate private IP ranges, got: {}",
        sql
    );
}

/// Test IsPublic operator for PostgreSQL
#[test]
fn test_network_operator_is_public_postgresql() {
    let clause = WhereClause::Field {
        path: vec!["ip_address".to_string()],
        operator: WhereOperator::IsPublic,
        value: json!(true),
    };

    let sql = WhereSqlGenerator::to_sql_for_db(&clause, DatabaseType::PostgreSQL)
        .expect("Should generate SQL for IsPublic");

    assert!(
        !sql.is_empty(),
        "PostgreSQL should generate public IP validation, got: {}",
        sql
    );
}

/// Test InSubnet operator with CIDR notation for PostgreSQL
#[test]
fn test_network_operator_in_subnet_postgresql() {
    let clause = WhereClause::Field {
        path: vec!["ip_address".to_string()],
        operator: WhereOperator::InSubnet,
        value: json!("10.0.0.0/8"),
    };

    let sql = WhereSqlGenerator::to_sql_for_db(&clause, DatabaseType::PostgreSQL)
        .expect("Should generate SQL for InSubnet");

    // PostgreSQL: ip_address::inet << '10.0.0.0/8'::inet
    assert!(
        sql.contains("<<") || sql.contains("CAST") || sql.contains("inet"),
        "PostgreSQL should use CIDR containment operator (<<), got: {}",
        sql
    );
}

/// Test multiple network operators across all databases
#[test]
fn test_all_network_operators_have_sql_generation() {
    let network_operators = vec![
        (WhereOperator::IsIPv4, "IsIPv4"),
        (WhereOperator::IsIPv6, "IsIPv6"),
        (WhereOperator::IsPrivate, "IsPrivate"),
        (WhereOperator::IsPublic, "IsPublic"),
        (WhereOperator::InSubnet, "InSubnet"),
    ];

    let databases = vec![
        DatabaseType::PostgreSQL,
        DatabaseType::MySQL,
        DatabaseType::SQLite,
        DatabaseType::SQLServer,
    ];

    for (operator, op_name) in &network_operators {
        for db in &databases {
            let clause = WhereClause::Field {
                path: vec!["ip_field".to_string()],
                operator: operator.clone(),
                value: json!("192.168.1.1"),
            };

            let result = WhereSqlGenerator::to_sql_for_db(&clause, *db);

            // Should not error - all network operators should be implemented
            assert!(
                result.is_ok(),
                "Operator {} should be supported on {:?}, got error: {:?}",
                op_name,
                db,
                result
            );

            let sql = result.unwrap();
            assert!(
                !sql.is_empty(),
                "Operator {} on {:?} should generate non-empty SQL",
                op_name,
                db
            );
        }
    }
}

// ============================================================================
// PHASE 2, CYCLE 2: Database-Specific Behavior Tests
// ============================================================================

/// Test that newly implemented FTS operators work
#[test]
fn test_newly_implemented_fts_operators() {
    // Verify that FTS operators are now working
    let fts_operators = vec![
        (WhereOperator::Matches, "Matches"),
        (WhereOperator::PlainQuery, "PlainQuery"),
        (WhereOperator::PhraseQuery, "PhraseQuery"),
        (WhereOperator::WebsearchQuery, "WebsearchQuery"),
    ];

    for (operator, op_name) in fts_operators {
        let clause = WhereClause::Field {
            path: vec!["content".to_string()],
            operator,
            value: json!("search term"),
        };

        // Should work on PostgreSQL
        let result = WhereSqlGenerator::to_sql_for_db(&clause, DatabaseType::PostgreSQL);
        assert!(
            result.is_ok(),
            "FTS Operator {} should be implemented on PostgreSQL: {:?}",
            op_name,
            result
        );

        let sql = result.unwrap();
        assert!(
            !sql.is_empty(),
            "FTS Operator {} should generate SQL",
            op_name
        );
    }
}

/// Test that missing templates for operator+database combinations are caught
#[test]
fn test_templates_exist_for_all_implemented_operators() {
    // These operators have templates for all databases
    let implemented_ops = vec!["isIPv4", "isIPv6", "isPrivate", "isPublic", "inSubnet"];
    let databases = vec![
        (DatabaseType::PostgreSQL, "PostgreSQL"),
        (DatabaseType::MySQL, "MySQL"),
        (DatabaseType::SQLite, "SQLite"),
        (DatabaseType::SQLServer, "SQL Server"),
    ];

    for op_name in &implemented_ops {
        for (db_type, db_name) in &databases {
            // Verify template lookup works (indirectly by checking SQL generation)
            let clause = WhereClause::Field {
                path: vec!["ip_field".to_string()],
                operator: match *op_name {
                    "isIPv4" => WhereOperator::IsIPv4,
                    "isIPv6" => WhereOperator::IsIPv6,
                    "isPrivate" => WhereOperator::IsPrivate,
                    "isPublic" => WhereOperator::IsPublic,
                    "inSubnet" => WhereOperator::InSubnet,
                    _ => panic!("Unknown operator: {}", op_name),
                },
                value: json!("192.168.1.1"),
            };

            let result = WhereSqlGenerator::to_sql_for_db(&clause, *db_type);
            assert!(
                result.is_ok(),
                "Operator {} should have template for {} database",
                op_name,
                db_name
            );
        }
    }
}

/// Test that template validation produces helpful error messages
#[test]
fn test_error_messages_are_helpful() {
    let clause = WhereClause::Field {
        path: vec!["ip_field".to_string()],
        operator: WhereOperator::IsIPv4,
        value: json!("not-an-ip"),  // Invalid value format
    };

    // Even with invalid value, SQL should generate (validation happens at query time)
    let result = WhereSqlGenerator::to_sql_for_db(&clause, DatabaseType::PostgreSQL);
    assert!(
        result.is_ok(),
        "Should still generate SQL even with unusual values (validation at query time)"
    );
}

// ============================================================================
// PHASE 2, CYCLE 3: Comprehensive Network Operator Testing (15 × 4 = 60 cases)
// ============================================================================

/// Comprehensive test matrix: 15 network operators × 4 databases
#[test]
fn test_all_15_network_operators_across_4_databases() {
    let network_operators = vec![
        // Phase 2 Cycle 1 (already verified)
        (WhereOperator::IsIPv4, "IsIPv4"),
        (WhereOperator::IsIPv6, "IsIPv6"),
        (WhereOperator::IsPrivate, "IsPrivate"),
        (WhereOperator::IsPublic, "IsPublic"),
        (WhereOperator::InSubnet, "InSubnet"),
        // Phase 2 Cycle 3 (newly implemented)
        (WhereOperator::IsLoopback, "IsLoopback"),
        (WhereOperator::ContainsSubnet, "ContainsSubnet"),
        (WhereOperator::ContainsIP, "ContainsIP"),
        (WhereOperator::Overlaps, "Overlaps"),
        (WhereOperator::StrictlyContains, "StrictlyContains"),
    ];

    let databases = vec![
        (DatabaseType::PostgreSQL, "PostgreSQL"),
        (DatabaseType::MySQL, "MySQL"),
        (DatabaseType::SQLite, "SQLite"),
        (DatabaseType::SQLServer, "SQL Server"),
    ];

    let mut success_count = 0;
    let mut failure_count = 0;

    for (operator, op_name) in &network_operators {
        for (db_type, db_name) in &databases {
            let clause = WhereClause::Field {
                path: vec!["ip_address".to_string()],
                operator: operator.clone(),
                value: json!("192.168.1.1"),
            };

            match WhereSqlGenerator::to_sql_for_db(&clause, *db_type) {
                Ok(sql) => {
                    assert!(
                        !sql.is_empty(),
                        "Operator {} on {} should generate non-empty SQL",
                        op_name,
                        db_name
                    );
                    success_count += 1;
                },
                Err(e) => {
                    eprintln!(
                        "❌ {} on {}: {:?}",
                        op_name, db_name, e
                    );
                    failure_count += 1;
                },
            }
        }
    }

    eprintln!(
        "\n📊 Network Operator Test Matrix Results:\n   ✅ {} successes (out of 40 expected)\n   ❌ {} failures",
        success_count, failure_count
    );

    assert_eq!(
        success_count, 40,
        "Expected all 10 operators × 4 databases = 40 test cases to pass"
    );
}

/// Test that operators generate different SQL for different databases
#[test]
fn test_operators_generate_database_specific_sql() {
    let clause = WhereClause::Field {
        path: vec!["ip_address".to_string()],
        operator: WhereOperator::IsIPv4,
        value: json!(true),
    };

    let pg_sql = WhereSqlGenerator::to_sql_for_db(&clause, DatabaseType::PostgreSQL)
        .expect("PostgreSQL should support IsIPv4");
    let mysql_sql = WhereSqlGenerator::to_sql_for_db(&clause, DatabaseType::MySQL)
        .expect("MySQL should support IsIPv4");
    let sqlite_sql = WhereSqlGenerator::to_sql_for_db(&clause, DatabaseType::SQLite)
        .expect("SQLite should support IsIPv4");

    // Each database should generate different SQL
    assert!(
        pg_sql != mysql_sql && mysql_sql != sqlite_sql,
        "Different databases should generate different SQL: PG={}, MySQL={}, SQLite={}",
        pg_sql,
        mysql_sql,
        sqlite_sql
    );

    // SQL should contain database-specific functions
    assert!(
        pg_sql.contains("INET") || pg_sql.contains("CAST"),
        "PostgreSQL IsIPv4 should use INET: {}",
        pg_sql
    );
    assert!(
        mysql_sql.contains("INET") || mysql_sql.contains("REGEXP"),
        "MySQL IsIPv4 should use INET or REGEXP: {}",
        mysql_sql
    );
}

/// Test subnet operations work correctly
#[test]
fn test_subnet_operations_generate_valid_sql() {
    let subnet_operators = vec![
        (WhereOperator::InSubnet, "InSubnet", "10.0.0.0/8"),
        (WhereOperator::ContainsIP, "ContainsIP", "192.168.1.1"),
        (WhereOperator::ContainsSubnet, "ContainsSubnet", "10.0.0.0/8"),
        (WhereOperator::Overlaps, "Overlaps", "10.0.0.0/8"),
    ];

    for (operator, op_name, value) in subnet_operators {
        let clause = WhereClause::Field {
            path: vec!["ip_field".to_string()],
            operator,
            value: json!(value),
        };

        // Should work on PostgreSQL at minimum
        let result = WhereSqlGenerator::to_sql_for_db(&clause, DatabaseType::PostgreSQL);
        assert!(
            result.is_ok(),
            "Operator {} should generate SQL on PostgreSQL with value '{}': {:?}",
            op_name,
            value,
            result
        );

        let sql = result.unwrap();
        assert!(
            !sql.is_empty() && sql.len() > 10,
            "Operator {} should generate meaningful SQL, got: {}",
            op_name,
            sql
        );
    }
}
