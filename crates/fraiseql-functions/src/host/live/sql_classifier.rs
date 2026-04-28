//! SQL query classification for read-only enforcement.
//!
//! This module uses `sqlparser-rs` to parse SQL and classify it as either
//! read-only or explicitly rejected. The approach is whitelist-only:
//! only `SELECT` statements and `EXPLAIN` (without `ANALYZE`) are allowed.
//! Everything else is rejected by default.

use fraiseql_error::{FraiseQLError, Result};

/// Classification result for a SQL statement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SqlClassification {
    /// Statement is safe to execute (read-only).
    ReadOnly,
    /// Statement is not allowed.
    Rejected(RejectionReason),
}

/// Reasons why a SQL statement was rejected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RejectionReason {
    /// Write statement (INSERT, UPDATE, DELETE, MERGE).
    WriteStatement(String),
    /// DDL statement (CREATE, DROP, ALTER, TRUNCATE).
    DdlStatement(String),
    /// CTE containing writable statements.
    WritableCte,
    /// Privilege escalation (SET ROLE, SET SESSION AUTHORIZATION).
    PrivilegeEscalation,
    /// Procedural block (DO $$ ... $$).
    ProceduralBlock,
    /// Procedure call (CALL procedure()).
    ProcedureCall,
    /// COPY statement (can write).
    CopyStatement,
    /// EXPLAIN ANALYZE (actually executes the statement).
    ExplainAnalyze,
    /// Unknown statement type (not explicitly whitelisted).
    Unknown(String),
}

impl std::fmt::Display for RejectionReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::WriteStatement(stmt) => write!(f, "write statement not allowed: {}", stmt),
            Self::DdlStatement(stmt) => write!(f, "DDL statement not allowed: {}", stmt),
            Self::WritableCte => write!(f, "CTE with writable statement not allowed"),
            Self::PrivilegeEscalation => write!(f, "privilege escalation not allowed"),
            Self::ProceduralBlock => write!(f, "procedural block not allowed"),
            Self::ProcedureCall => write!(f, "procedure call not allowed"),
            Self::CopyStatement => write!(f, "COPY statement not allowed"),
            Self::ExplainAnalyze => write!(
                f,
                "EXPLAIN ANALYZE not allowed (executes the statement)"
            ),
            Self::Unknown(stmt) => write!(f, "unknown or disallowed statement: {}", stmt),
        }
    }
}

/// Classify a SQL statement as read-only or rejected.
///
/// Uses a whitelist-only approach: only `SELECT` queries and `EXPLAIN` (without `ANALYZE`)
/// are allowed. All other statements are rejected.
///
/// # Arguments
///
/// * `sql` - The SQL statement to classify
///
/// # Returns
///
/// - `Ok(SqlClassification::ReadOnly)` if the statement is safe
/// - `Ok(SqlClassification::Rejected(reason))` if the statement is not allowed
/// - `Err` if parsing fails
pub fn classify_sql(sql: &str) -> Result<SqlClassification> {
    use sqlparser::parser::Parser;
    use sqlparser::dialect::PostgreSqlDialect;

    let dialect = PostgreSqlDialect {};
    let statements = Parser::parse_sql(&dialect, sql).map_err(|e| {
        FraiseQLError::Validation {
            message: format!("invalid SQL: {}", e),
            path: None,
        }
    })?;

    // Check each statement in the batch
    for stmt in statements {
        let classification = classify_statement(&stmt)?;
        match classification {
            SqlClassification::ReadOnly => continue,
            SqlClassification::Rejected(reason) => return Ok(SqlClassification::Rejected(reason)),
        }
    }

    Ok(SqlClassification::ReadOnly)
}

/// Classify a single parsed statement.
fn classify_statement(stmt: &sqlparser::ast::Statement) -> Result<SqlClassification> {
    use sqlparser::ast::Statement;

    match stmt {
        // Only SELECT queries are allowed
        Statement::Query(_) => Ok(SqlClassification::ReadOnly),

        // EXPLAIN is allowed, but not EXPLAIN ANALYZE (which executes the statement)
        Statement::Explain {
            describe_alias: _,
            analyze,
            verbose: _,
            statement: _,
            format: _,
        } => {
            if *analyze {
                Ok(SqlClassification::Rejected(RejectionReason::ExplainAnalyze))
            } else {
                Ok(SqlClassification::ReadOnly)
            }
        }

        // Explicitly reject write statements
        Statement::Insert { .. } => {
            Ok(SqlClassification::Rejected(RejectionReason::WriteStatement(
                "INSERT".to_string(),
            )))
        }
        Statement::Update { .. } => {
            Ok(SqlClassification::Rejected(RejectionReason::WriteStatement(
                "UPDATE".to_string(),
            )))
        }
        Statement::Delete { .. } => {
            Ok(SqlClassification::Rejected(RejectionReason::WriteStatement(
                "DELETE".to_string(),
            )))
        }

        // Reject DDL
        Statement::CreateTable { .. } => {
            Ok(SqlClassification::Rejected(RejectionReason::DdlStatement(
                "CREATE TABLE".to_string(),
            )))
        }
        Statement::CreateView { .. } => {
            Ok(SqlClassification::Rejected(RejectionReason::DdlStatement(
                "CREATE VIEW".to_string(),
            )))
        }
        Statement::CreateIndex { .. } => {
            Ok(SqlClassification::Rejected(RejectionReason::DdlStatement(
                "CREATE INDEX".to_string(),
            )))
        }
        Statement::CreateSchema { .. } => {
            Ok(SqlClassification::Rejected(RejectionReason::DdlStatement(
                "CREATE SCHEMA".to_string(),
            )))
        }
        Statement::CreateRole { .. } => {
            Ok(SqlClassification::Rejected(RejectionReason::DdlStatement(
                "CREATE ROLE".to_string(),
            )))
        }
        Statement::CreateExtension { .. } => {
            Ok(SqlClassification::Rejected(RejectionReason::DdlStatement(
                "CREATE EXTENSION".to_string(),
            )))
        }
        Statement::CreateSecret { .. } => {
            Ok(SqlClassification::Rejected(RejectionReason::DdlStatement(
                "CREATE SECRET".to_string(),
            )))
        }
        Statement::CreateVirtualTable { .. } => {
            Ok(SqlClassification::Rejected(RejectionReason::DdlStatement(
                "CREATE VIRTUAL TABLE".to_string(),
            )))
        }
        Statement::Drop { .. } => {
            Ok(SqlClassification::Rejected(RejectionReason::DdlStatement(
                "DROP".to_string(),
            )))
        }
        Statement::DropFunction { .. } => {
            Ok(SqlClassification::Rejected(RejectionReason::DdlStatement(
                "DROP FUNCTION".to_string(),
            )))
        }
        Statement::DropSecret { .. } => {
            Ok(SqlClassification::Rejected(RejectionReason::DdlStatement(
                "DROP SECRET".to_string(),
            )))
        }
        Statement::AlterTable { .. } => {
            Ok(SqlClassification::Rejected(RejectionReason::DdlStatement(
                "ALTER TABLE".to_string(),
            )))
        }
        Statement::AlterIndex { .. } => {
            Ok(SqlClassification::Rejected(RejectionReason::DdlStatement(
                "ALTER INDEX".to_string(),
            )))
        }
        Statement::AlterView { .. } => {
            Ok(SqlClassification::Rejected(RejectionReason::DdlStatement(
                "ALTER VIEW".to_string(),
            )))
        }
        Statement::AlterRole { .. } => {
            Ok(SqlClassification::Rejected(RejectionReason::DdlStatement(
                "ALTER ROLE".to_string(),
            )))
        }
        Statement::Truncate { .. } => {
            Ok(SqlClassification::Rejected(RejectionReason::DdlStatement(
                "TRUNCATE".to_string(),
            )))
        }

        // Reject privilege escalation
        Statement::SetVariable { .. } => {
            Ok(SqlClassification::Rejected(
                RejectionReason::PrivilegeEscalation,
            ))
        }
        Statement::SetRole { .. } => {
            Ok(SqlClassification::Rejected(
                RejectionReason::PrivilegeEscalation,
            ))
        }
        Statement::SetTimeZone { .. } => {
            Ok(SqlClassification::Rejected(
                RejectionReason::PrivilegeEscalation,
            ))
        }
        Statement::SetNames { .. } => {
            Ok(SqlClassification::Rejected(
                RejectionReason::PrivilegeEscalation,
            ))
        }
        Statement::SetNamesDefault { .. } => {
            Ok(SqlClassification::Rejected(
                RejectionReason::PrivilegeEscalation,
            ))
        }

        // Reject procedure calls
        Statement::Call(_) => {
            Ok(SqlClassification::Rejected(RejectionReason::ProcedureCall))
        }

        // Reject COPY
        Statement::Copy { .. } => {
            Ok(SqlClassification::Rejected(RejectionReason::CopyStatement))
        }
        Statement::CopyIntoSnowflake { .. } => {
            Ok(SqlClassification::Rejected(RejectionReason::CopyStatement))
        }

        // Reject ANALYZE (Hive statement that may cause writes)
        Statement::Analyze { .. } => {
            Ok(SqlClassification::Rejected(RejectionReason::Unknown(
                "ANALYZE statement not allowed".to_string(),
            )))
        }

        // Reject other potentially dangerous statements
        Statement::Install { .. } => {
            Ok(SqlClassification::Rejected(RejectionReason::Unknown(
                "INSTALL not allowed".to_string(),
            )))
        }
        Statement::Load { .. } => {
            Ok(SqlClassification::Rejected(RejectionReason::Unknown(
                "LOAD not allowed".to_string(),
            )))
        }
        Statement::Directory { .. } => {
            Ok(SqlClassification::Rejected(RejectionReason::Unknown(
                "DIRECTORY not allowed".to_string(),
            )))
        }
        Statement::AttachDatabase { .. } => {
            Ok(SqlClassification::Rejected(RejectionReason::Unknown(
                "ATTACH DATABASE not allowed".to_string(),
            )))
        }
        Statement::AttachDuckDBDatabase { .. } => {
            Ok(SqlClassification::Rejected(RejectionReason::Unknown(
                "ATTACH DUCKDB DATABASE not allowed".to_string(),
            )))
        }
        Statement::DetachDuckDBDatabase { .. } => {
            Ok(SqlClassification::Rejected(RejectionReason::Unknown(
                "DETACH DUCKDB DATABASE not allowed".to_string(),
            )))
        }
        Statement::Declare { .. } => {
            Ok(SqlClassification::Rejected(RejectionReason::Unknown(
                "DECLARE not allowed".to_string(),
            )))
        }
        Statement::Close { .. } => {
            Ok(SqlClassification::Rejected(RejectionReason::Unknown(
                "CLOSE not allowed".to_string(),
            )))
        }
        Statement::Fetch { .. } => {
            Ok(SqlClassification::Rejected(RejectionReason::Unknown(
                "FETCH not allowed".to_string(),
            )))
        }
        Statement::Flush { .. } => {
            Ok(SqlClassification::Rejected(RejectionReason::Unknown(
                "FLUSH not allowed".to_string(),
            )))
        }
        Statement::Discard { .. } => {
            Ok(SqlClassification::Rejected(RejectionReason::Unknown(
                "DISCARD not allowed".to_string(),
            )))
        }
        Statement::StartTransaction { .. } => {
            Ok(SqlClassification::Rejected(RejectionReason::Unknown(
                "START TRANSACTION not allowed".to_string(),
            )))
        }
        Statement::Msck { .. } => {
            Ok(SqlClassification::Rejected(RejectionReason::Unknown(
                "MSCK not allowed".to_string(),
            )))
        }

        // Reject everything else by default (whitelist-only approach)
        _ => Ok(SqlClassification::Rejected(RejectionReason::Unknown(
            format!("{:?}", stmt).chars().take(50).collect(),
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classify_select_is_readonly() {
        let result = classify_sql("SELECT * FROM users");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), SqlClassification::ReadOnly);
    }

    #[test]
    fn test_classify_select_with_where_is_readonly() {
        let result = classify_sql("SELECT id, name FROM users WHERE active = true");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), SqlClassification::ReadOnly);
    }

    #[test]
    fn test_classify_insert_is_rejected() {
        let result = classify_sql("INSERT INTO users (name) VALUES ('Alice')");
        assert!(result.is_ok());
        match result.unwrap() {
            SqlClassification::Rejected(RejectionReason::WriteStatement(_)) => (),
            other => panic!("expected WriteStatement, got {:?}", other),
        }
    }

    #[test]
    fn test_classify_update_is_rejected() {
        let result = classify_sql("UPDATE users SET name = 'Bob' WHERE id = 1");
        assert!(result.is_ok());
        match result.unwrap() {
            SqlClassification::Rejected(RejectionReason::WriteStatement(_)) => (),
            other => panic!("expected WriteStatement, got {:?}", other),
        }
    }

    #[test]
    fn test_classify_delete_is_rejected() {
        let result = classify_sql("DELETE FROM users WHERE id = 1");
        assert!(result.is_ok());
        match result.unwrap() {
            SqlClassification::Rejected(RejectionReason::WriteStatement(_)) => (),
            other => panic!("expected WriteStatement, got {:?}", other),
        }
    }

    #[test]
    fn test_classify_create_table_is_rejected() {
        let result = classify_sql("CREATE TABLE new_table (id INT, name TEXT)");
        assert!(result.is_ok());
        match result.unwrap() {
            SqlClassification::Rejected(RejectionReason::DdlStatement(_)) => (),
            other => panic!("expected DdlStatement, got {:?}", other),
        }
    }

    #[test]
    fn test_classify_drop_is_rejected() {
        let result = classify_sql("DROP TABLE users");
        assert!(result.is_ok());
        match result.unwrap() {
            SqlClassification::Rejected(RejectionReason::DdlStatement(_)) => (),
            other => panic!("expected DdlStatement, got {:?}", other),
        }
    }

    #[test]
    fn test_classify_truncate_is_rejected() {
        let result = classify_sql("TRUNCATE users");
        assert!(result.is_ok());
        match result.unwrap() {
            SqlClassification::Rejected(RejectionReason::DdlStatement(_)) => (),
            other => panic!("expected DdlStatement, got {:?}", other),
        }
    }

    #[test]
    fn test_classify_explain_without_analyze_is_readonly() {
        let result = classify_sql("EXPLAIN SELECT * FROM users");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), SqlClassification::ReadOnly);
    }

    #[test]
    fn test_classify_explain_analyze_is_rejected() {
        let result = classify_sql("EXPLAIN ANALYZE SELECT * FROM users");
        assert!(result.is_ok());
        match result.unwrap() {
            SqlClassification::Rejected(RejectionReason::ExplainAnalyze) => (),
            other => panic!("expected ExplainAnalyze, got {:?}", other),
        }
    }

    #[test]
    fn test_classify_set_role_is_rejected() {
        let result = classify_sql("SET ROLE admin");
        assert!(result.is_ok());
        match result.unwrap() {
            SqlClassification::Rejected(RejectionReason::PrivilegeEscalation) => (),
            other => panic!("expected PrivilegeEscalation, got {:?}", other),
        }
    }

    #[test]
    fn test_classify_unknown_statement_is_rejected() {
        // Most DDL variants are explicitly handled, unknown ones fall through to Unknown
        let result = classify_sql("ANALYZE TABLE users");
        assert!(result.is_ok());
        match result.unwrap() {
            SqlClassification::Rejected(_) => (),
            other => panic!("expected Rejected, got {:?}", other),
        }
    }

    #[test]
    fn test_classify_call_is_rejected() {
        let result = classify_sql("CALL delete_all_users()");
        assert!(result.is_ok());
        match result.unwrap() {
            SqlClassification::Rejected(RejectionReason::ProcedureCall) => (),
            other => panic!("expected ProcedureCall, got {:?}", other),
        }
    }

    #[test]
    fn test_classify_copy_is_rejected() {
        let result = classify_sql("COPY users FROM '/tmp/data.csv'");
        assert!(result.is_ok());
        match result.unwrap() {
            SqlClassification::Rejected(RejectionReason::CopyStatement) => (),
            other => panic!("expected CopyStatement, got {:?}", other),
        }
    }

    #[test]
    fn test_classify_invalid_sql_returns_error() {
        let result = classify_sql("INVALID SYNTAX HERE");
        assert!(result.is_err());
    }
}
