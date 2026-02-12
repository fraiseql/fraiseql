//! Array length and full-text search operator tests.
//!
//! This test verifies that array length operators (LenEq, LenGt, etc.) and
//! full-text search operators (Matches, PlainQuery, etc.) work across all databases
//! with database-specific SQL generation.
//!
//! Array Length Operators:
//! - LenEq: Array length equals N
//! - LenNeq: Array length not equal to N
//! - LenGt: Array length > N
//! - LenGte: Array length >= N
//! - LenLt: Array length < N
//! - LenLte: Array length <= N
//!
//! Full-Text Search Operators:
//! - Matches: Full-text search match (any mode)
//! - PlainQuery: Plain text query (no special syntax)
//! - PhraseQuery: Phrase search (quoted strings)
//! - WebsearchQuery: Web search syntax (like Google)

use fraiseql_core::db::where_clause::{WhereClause, WhereOperator};
use fraiseql_core::db::where_sql_generator::WhereSqlGenerator;
use fraiseql_core::db::types::DatabaseType;
use serde_json::json;

// ============================================================================
// RED PHASE: Array Length Operator Tests
// ============================================================================

/// Test LenEq (array length equals) on PostgreSQL
#[test]
fn test_array_len_eq_postgresql() {
    let clause = WhereClause::Field {
        path: vec!["tags".to_string()],
        operator: WhereOperator::LenEq,
        value: json!(5),
    };

    let sql = WhereSqlGenerator::to_sql_for_db(&clause, DatabaseType::PostgreSQL)
        .expect("LenEq should work on PostgreSQL");

    // Should use array_length function
    assert!(
        sql.contains("array_length") || sql.contains("="),
        "PostgreSQL LenEq should use array_length or comparison, got: {}",
        sql
    );
}

/// Test LenEq on MySQL
#[test]
fn test_array_len_eq_mysql() {
    let clause = WhereClause::Field {
        path: vec!["tags".to_string()],
        operator: WhereOperator::LenEq,
        value: json!(5),
    };

    let sql = WhereSqlGenerator::to_sql_for_db(&clause, DatabaseType::MySQL)
        .expect("LenEq should work on MySQL");

    // Should use JSON_LENGTH
    assert!(
        sql.contains("JSON_LENGTH") || sql.contains("json_length"),
        "MySQL LenEq should use JSON_LENGTH, got: {}",
        sql
    );
}

/// Test LenEq on SQLite
#[test]
fn test_array_len_eq_sqlite() {
    let clause = WhereClause::Field {
        path: vec!["tags".to_string()],
        operator: WhereOperator::LenEq,
        value: json!(5),
    };

    let sql = WhereSqlGenerator::to_sql_for_db(&clause, DatabaseType::SQLite)
        .expect("LenEq should work on SQLite");

    // Should use json_array_length
    assert!(
        sql.contains("json_array_length"),
        "SQLite LenEq should use json_array_length, got: {}",
        sql
    );
}

/// Test all 6 array length operators on PostgreSQL
#[test]
fn test_all_array_length_operators_postgresql() {
    let len_operators = vec![
        (WhereOperator::LenEq, "LenEq", "="),
        (WhereOperator::LenNeq, "LenNeq", "!="),
        (WhereOperator::LenGt, "LenGt", ">"),
        (WhereOperator::LenGte, "LenGte", ">="),
        (WhereOperator::LenLt, "LenLt", "<"),
        (WhereOperator::LenLte, "LenLte", "<="),
    ];

    for (operator, op_name, expected_op) in len_operators {
        let clause = WhereClause::Field {
            path: vec!["items".to_string()],
            operator,
            value: json!(3),
        };

        let result = WhereSqlGenerator::to_sql_for_db(&clause, DatabaseType::PostgreSQL);
        assert!(
            result.is_ok(),
            "Operator {} should work on PostgreSQL, got: {:?}",
            op_name,
            result
        );

        let sql = result.unwrap();
        assert!(
            sql.contains(expected_op) || sql.contains("array_length"),
            "Operator {} should contain '{}' or 'array_length', got: {}",
            op_name,
            expected_op,
            sql
        );
    }
}

// ============================================================================
// RED PHASE: Full-Text Search Operator Tests
// ============================================================================

/// Test Matches (FTS) on PostgreSQL
#[test]
fn test_fts_matches_postgresql() {
    let clause = WhereClause::Field {
        path: vec!["content".to_string()],
        operator: WhereOperator::Matches,
        value: json!("search query"),
    };

    let sql = WhereSqlGenerator::to_sql_for_db(&clause, DatabaseType::PostgreSQL)
        .expect("Matches should work on PostgreSQL");

    // Should use PostgreSQL FTS (to_tsvector, @@)
    assert!(
        sql.contains("tsvector") || sql.contains("@@") || sql.contains("tsquery"),
        "PostgreSQL Matches should use FTS syntax, got: {}",
        sql
    );
}

/// Test PlainQuery (simple text search) on MySQL
#[test]
fn test_fts_plain_query_mysql() {
    let clause = WhereClause::Field {
        path: vec!["content".to_string()],
        operator: WhereOperator::PlainQuery,
        value: json!("simple search"),
    };

    let sql = WhereSqlGenerator::to_sql_for_db(&clause, DatabaseType::MySQL)
        .expect("PlainQuery should work on MySQL");

    // Should use MATCH ... AGAINST
    assert!(
        sql.contains("MATCH") || sql.contains("AGAINST"),
        "MySQL PlainQuery should use MATCH AGAINST, got: {}",
        sql
    );
}

/// Test PhraseQuery (quoted phrase) on PostgreSQL
#[test]
fn test_fts_phrase_query_postgresql() {
    let clause = WhereClause::Field {
        path: vec!["content".to_string()],
        operator: WhereOperator::PhraseQuery,
        value: json!("exact phrase"),
    };

    let sql = WhereSqlGenerator::to_sql_for_db(&clause, DatabaseType::PostgreSQL)
        .expect("PhraseQuery should work on PostgreSQL");

    assert!(
        !sql.is_empty(),
        "PostgreSQL PhraseQuery should generate SQL, got: {}",
        sql
    );
}

/// Test WebsearchQuery on PostgreSQL
#[test]
fn test_fts_websearch_query_postgresql() {
    let clause = WhereClause::Field {
        path: vec!["content".to_string()],
        operator: WhereOperator::WebsearchQuery,
        value: json!("search term -exclude"),
    };

    let sql = WhereSqlGenerator::to_sql_for_db(&clause, DatabaseType::PostgreSQL)
        .expect("WebsearchQuery should work on PostgreSQL");

    assert!(
        !sql.is_empty(),
        "PostgreSQL WebsearchQuery should generate SQL, got: {}",
        sql
    );
}

/// Test that all 4 FTS operators work on PostgreSQL
#[test]
fn test_all_fts_operators_postgresql() {
    let fts_operators = vec![
        (WhereOperator::Matches, "Matches"),
        (WhereOperator::PlainQuery, "PlainQuery"),
        (WhereOperator::PhraseQuery, "PhraseQuery"),
        (WhereOperator::WebsearchQuery, "WebsearchQuery"),
    ];

    for (operator, op_name) in fts_operators {
        let clause = WhereClause::Field {
            path: vec!["title".to_string()],
            operator,
            value: json!("search text"),
        };

        let result = WhereSqlGenerator::to_sql_for_db(&clause, DatabaseType::PostgreSQL);
        assert!(
            result.is_ok(),
            "Operator {} should work on PostgreSQL, got: {:?}",
            op_name,
            result
        );

        let sql = result.unwrap();
        assert!(
            !sql.is_empty(),
            "Operator {} should generate non-empty SQL",
            op_name
        );
    }
}

/// Test array length operators across all databases
#[test]
fn test_array_length_all_databases() {
    let databases = vec![
        (DatabaseType::PostgreSQL, "PostgreSQL"),
        (DatabaseType::MySQL, "MySQL"),
        (DatabaseType::SQLite, "SQLite"),
        (DatabaseType::SQLServer, "SQL Server"),
    ];

    for (db_type, db_name) in databases {
        let clause = WhereClause::Field {
            path: vec!["items".to_string()],
            operator: WhereOperator::LenEq,
            value: json!(5),
        };

        let result = WhereSqlGenerator::to_sql_for_db(&clause, db_type);
        assert!(
            result.is_ok(),
            "LenEq should work on {}: {:?}",
            db_name,
            result
        );
    }
}

/// Test FTS operators across all databases
#[test]
fn test_fts_all_databases() {
    let databases = vec![
        (DatabaseType::PostgreSQL, "PostgreSQL"),
        (DatabaseType::MySQL, "MySQL"),
        (DatabaseType::SQLite, "SQLite"),
        (DatabaseType::SQLServer, "SQL Server"),
    ];

    for (db_type, db_name) in databases {
        let clause = WhereClause::Field {
            path: vec!["content".to_string()],
            operator: WhereOperator::Matches,
            value: json!("search term"),
        };

        let result = WhereSqlGenerator::to_sql_for_db(&clause, db_type);
        assert!(
            result.is_ok(),
            "Matches should work on {}: {:?}",
            db_name,
            result
        );
    }
}

/// Test database-specific SQL generation for array length
#[test]
fn test_array_length_database_specific_sql() {
    let clause = WhereClause::Field {
        path: vec!["tags".to_string()],
        operator: WhereOperator::LenGt,
        value: json!(2),
    };

    let pg_sql = WhereSqlGenerator::to_sql_for_db(&clause, DatabaseType::PostgreSQL)
        .expect("PostgreSQL");
    let mysql_sql = WhereSqlGenerator::to_sql_for_db(&clause, DatabaseType::MySQL)
        .expect("MySQL");
    let sqlite_sql = WhereSqlGenerator::to_sql_for_db(&clause, DatabaseType::SQLite)
        .expect("SQLite");

    // Each database should generate different SQL
    assert!(
        pg_sql != mysql_sql && mysql_sql != sqlite_sql,
        "Different databases should generate different SQL for array length"
    );

    // Verify database-specific functions
    assert!(
        pg_sql.contains("array_length"),
        "PostgreSQL should use array_length: {}",
        pg_sql
    );
    assert!(
        mysql_sql.contains("JSON_LENGTH") || mysql_sql.contains("json_length"),
        "MySQL should use JSON_LENGTH: {}",
        mysql_sql
    );
    assert!(
        sqlite_sql.contains("json_array_length"),
        "SQLite should use json_array_length: {}",
        sqlite_sql
    );
}

/// Test database-specific SQL generation for FTS
#[test]
fn test_fts_database_specific_sql() {
    let clause = WhereClause::Field {
        path: vec!["content".to_string()],
        operator: WhereOperator::PlainQuery,
        value: json!("search text"),
    };

    let pg_sql = WhereSqlGenerator::to_sql_for_db(&clause, DatabaseType::PostgreSQL)
        .expect("PostgreSQL");
    let mysql_sql = WhereSqlGenerator::to_sql_for_db(&clause, DatabaseType::MySQL)
        .expect("MySQL");

    // Each database should generate different SQL
    assert!(
        pg_sql != mysql_sql,
        "PostgreSQL and MySQL should generate different FTS SQL"
    );

    // Verify database-specific syntax
    assert!(
        pg_sql.contains("tsvector") || pg_sql.contains("tsquery"),
        "PostgreSQL should use tsvector/tsquery: {}",
        pg_sql
    );
    assert!(
        mysql_sql.contains("MATCH") || mysql_sql.contains("AGAINST"),
        "MySQL should use MATCH AGAINST: {}",
        mysql_sql
    );
}
