//! Extended operator tests for rich scalar types.
//!
//! This test verifies that Extended operators (Email, Phone, Country, etc.)
//! generate correct database-specific SQL using templates.
//!
//! Extended operators enable specialized filtering on structured data types:
//! - Email: Extract domain and filter (e.g., domainEq, domainIn, domainEndswith)
//! - Phone: Country code matching (e.g., countryCodeEq, isValid)
//! - Country: Continent/region lookup (e.g., continentEq, regionEq)
//! - Coordinates: Geographic queries (e.g., distanceWithin, withinBoundingBox)
//! - Financial: VIN, IBAN, CUSIP extraction and validation
//! - Identifiers: UUID, SSN, ISBN validation and formatting

use fraiseql_core::db::where_clause::{WhereClause, WhereOperator};
use fraiseql_core::db::where_sql_generator::WhereSqlGenerator;
use fraiseql_core::db::types::DatabaseType;
use fraiseql_core::filters::ExtendedOperator;
use serde_json::json;

// ============================================================================
// RED PHASE: Tests that should pass when extended operators are implemented
// ============================================================================

/// Test that email domain equality operator generates SQL
#[test]
fn test_extended_operator_email_domain_eq_postgresql() {
    let clause = WhereClause::Field {
        path: vec!["email".to_string()],
        operator: WhereOperator::Extended(ExtendedOperator::EmailDomainEq(
            "example.com".to_string(),
        )),
        value: json!(null), // Extended operators manage their own values
    };

    let sql = WhereSqlGenerator::to_sql_for_db(&clause, DatabaseType::PostgreSQL)
        .expect("EmailDomainEq should generate SQL on PostgreSQL");

    // PostgreSQL: SPLIT_PART(field, '@', 2) = 'example.com'
    assert!(
        sql.contains("SPLIT_PART") || sql.contains("@"),
        "PostgreSQL should extract email domain, got: {}",
        sql
    );
}

/// Test that email domain equality works on MySQL
#[test]
fn test_extended_operator_email_domain_eq_mysql() {
    let clause = WhereClause::Field {
        path: vec!["email".to_string()],
        operator: WhereOperator::Extended(ExtendedOperator::EmailDomainEq(
            "example.com".to_string(),
        )),
        value: json!(null),
    };

    let sql = WhereSqlGenerator::to_sql_for_db(&clause, DatabaseType::MySQL)
        .expect("EmailDomainEq should generate SQL on MySQL");

    // MySQL: SUBSTRING_INDEX(field, '@', -1) = 'example.com'
    assert!(
        sql.contains("SUBSTRING_INDEX") || sql.contains("@"),
        "MySQL should extract email domain, got: {}",
        sql
    );
}

/// Test that email domain in list operator generates SQL
#[test]
fn test_extended_operator_email_domain_in_postgresql() {
    let clause = WhereClause::Field {
        path: vec!["email".to_string()],
        operator: WhereOperator::Extended(ExtendedOperator::EmailDomainIn(vec![
            "example.com".to_string(),
            "test.org".to_string(),
        ])),
        value: json!(null),
    };

    let sql = WhereSqlGenerator::to_sql_for_db(&clause, DatabaseType::PostgreSQL)
        .expect("EmailDomainIn should generate SQL on PostgreSQL");

    // Should contain IN clause
    assert!(
        sql.contains("IN") || sql.contains("@"),
        "PostgreSQL should check domain in list, got: {}",
        sql
    );
}

/// Test country code equality operator
#[test]
fn test_extended_operator_country_code_eq_postgresql() {
    let clause = WhereClause::Field {
        path: vec!["country".to_string()],
        operator: WhereOperator::Extended(ExtendedOperator::IbanCountryEq(
            "DE".to_string(),
        )),
        value: json!(null),
    };

    let sql = WhereSqlGenerator::to_sql_for_db(&clause, DatabaseType::PostgreSQL)
        .expect("IbanCountryEq should generate SQL on PostgreSQL");

    // Should generate valid SQL
    assert!(!sql.is_empty(), "IbanCountryEq should generate non-empty SQL");
}

/// Test that extended operators work across multiple database types
#[test]
fn test_extended_operators_all_databases() {
    let operators = vec![
        ("Email Domain Eq", ExtendedOperator::EmailDomainEq("example.com".to_string())),
        ("Email Domain In", ExtendedOperator::EmailDomainIn(vec!["example.com".to_string()])),
        ("Email Domain Endswith", ExtendedOperator::EmailDomainEndswith(".edu".to_string())),
        ("URL Protocol Eq", ExtendedOperator::UrlProtocolEq("https".to_string())),
    ];

    let databases = vec![
        DatabaseType::PostgreSQL,
        DatabaseType::MySQL,
        DatabaseType::SQLite,
        DatabaseType::SQLServer,
    ];

    for (op_name, extended_op) in &operators {
        for db in &databases {
            let clause = WhereClause::Field {
                path: vec!["field".to_string()],
                operator: WhereOperator::Extended(extended_op.clone()),
                value: json!(null),
            };

            let result = WhereSqlGenerator::to_sql_for_db(&clause, *db);
            assert!(
                result.is_ok(),
                "Operator {} should work on {:?}, got: {:?}",
                op_name,
                db,
                result.err()
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

/// Test that extended operators generate different SQL for different databases
#[test]
fn test_extended_operators_database_specific_sql() {
    let clause = WhereClause::Field {
        path: vec!["email".to_string()],
        operator: WhereOperator::Extended(ExtendedOperator::EmailDomainEq(
            "example.com".to_string(),
        )),
        value: json!(null),
    };

    let pg_sql = WhereSqlGenerator::to_sql_for_db(&clause, DatabaseType::PostgreSQL)
        .expect("PostgreSQL should support EmailDomainEq");
    let mysql_sql = WhereSqlGenerator::to_sql_for_db(&clause, DatabaseType::MySQL)
        .expect("MySQL should support EmailDomainEq");
    let sqlite_sql = WhereSqlGenerator::to_sql_for_db(&clause, DatabaseType::SQLite)
        .expect("SQLite should support EmailDomainEq");

    // Each database should generate different SQL
    assert!(
        pg_sql != mysql_sql && mysql_sql != sqlite_sql,
        "Different databases should generate different SQL: PG={}, MySQL={}, SQLite={}",
        pg_sql,
        mysql_sql,
        sqlite_sql
    );

    // Verify database-specific functions
    assert!(
        pg_sql.contains("SPLIT_PART"),
        "PostgreSQL should use SPLIT_PART: {}",
        pg_sql
    );
    assert!(
        mysql_sql.contains("SUBSTRING_INDEX"),
        "MySQL should use SUBSTRING_INDEX: {}",
        mysql_sql
    );
    assert!(
        sqlite_sql.contains("SUBSTR") || sqlite_sql.contains("INSTR"),
        "SQLite should use SUBSTR/INSTR: {}",
        sqlite_sql
    );
}

/// Test email operators across all types
#[test]
fn test_all_email_operators_postgresql() {
    let email_operators = vec![
        ("EmailDomainEq", ExtendedOperator::EmailDomainEq("example.com".to_string())),
        ("EmailDomainIn", ExtendedOperator::EmailDomainIn(vec!["example.com".to_string()])),
        ("EmailDomainEndswith", ExtendedOperator::EmailDomainEndswith(".edu".to_string())),
        ("EmailLocalPartStartswith", ExtendedOperator::EmailLocalPartStartswith("admin".to_string())),
    ];

    for (op_name, operator) in email_operators {
        let clause = WhereClause::Field {
            path: vec!["email".to_string()],
            operator: WhereOperator::Extended(operator),
            value: json!(null),
        };

        let result = WhereSqlGenerator::to_sql_for_db(&clause, DatabaseType::PostgreSQL);
        assert!(
            result.is_ok(),
            "Email operator {} should work on PostgreSQL: {:?}",
            op_name,
            result.err()
        );

        let sql = result.unwrap();
        assert!(
            !sql.is_empty(),
            "Email operator {} should generate SQL",
            op_name
        );
    }
}

/// Test VIN operators across databases (VIN operators have templates)
#[test]
fn test_vin_operators_all_databases() {
    let vin_operators = vec![
        ("VinWmiEq", ExtendedOperator::VinWmiEq("1HG".to_string())),
        ("VinCountryEq", ExtendedOperator::VinCountryEq("US".to_string())),
        ("VinIsValid", ExtendedOperator::VinIsValid(true)),
    ];

    let databases = vec![
        DatabaseType::PostgreSQL,
        DatabaseType::MySQL,
        DatabaseType::SQLite,
        DatabaseType::SQLServer,
    ];

    for (op_name, operator) in &vin_operators {
        for db in &databases {
            let clause = WhereClause::Field {
                path: vec!["vin".to_string()],
                operator: WhereOperator::Extended(operator.clone()),
                value: json!(null),
            };

            let result = WhereSqlGenerator::to_sql_for_db(&clause, *db);
            assert!(
                result.is_ok(),
                "VIN operator {} should work on {:?}: {:?}",
                op_name,
                db,
                result.err()
            );
        }
    }
}

/// Test domain name operators
#[test]
fn test_domain_operators_postgresql() {
    let domain_operators = vec![
        ("DomainNameTldEq", ExtendedOperator::DomainNameTldEq("com".to_string())),
        ("DomainNameTldIn", ExtendedOperator::DomainNameTldIn(vec!["com".to_string(), "org".to_string()])),
        ("HostnameIsFqdn", ExtendedOperator::HostnameIsFqdn(true)),
    ];

    for (op_name, operator) in domain_operators {
        let clause = WhereClause::Field {
            path: vec!["domain".to_string()],
            operator: WhereOperator::Extended(operator),
            value: json!(null),
        };

        let result = WhereSqlGenerator::to_sql_for_db(&clause, DatabaseType::PostgreSQL);
        assert!(
            result.is_ok(),
            "Domain operator {} should work on PostgreSQL: {:?}",
            op_name,
            result.err()
        );

        let sql = result.unwrap();
        assert!(
            !sql.is_empty(),
            "Domain operator {} should generate SQL",
            op_name
        );
    }
}
