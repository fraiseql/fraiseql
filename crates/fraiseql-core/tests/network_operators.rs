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
