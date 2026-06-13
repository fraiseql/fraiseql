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
    /// Procedure call (CALL `procedure()`).
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
            Self::ExplainAnalyze => {
                write!(f, "EXPLAIN ANALYZE not allowed (executes the statement)")
            },
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
///
/// # Errors
///
/// Returns a validation error if the SQL cannot be parsed.
pub fn classify_sql(sql: &str) -> Result<SqlClassification> {
    use sqlparser::{dialect::PostgreSqlDialect, parser::Parser};

    let dialect = PostgreSqlDialect {};
    let statements = Parser::parse_sql(&dialect, sql).map_err(|e| FraiseQLError::Validation {
        message: format!("invalid SQL: {}", e),
        path:    None,
    })?;

    // Check each statement in the batch
    for stmt in statements {
        let classification = classify_statement(&stmt)?;
        match classification {
            SqlClassification::ReadOnly => {},
            SqlClassification::Rejected(reason) => return Ok(SqlClassification::Rejected(reason)),
        }
    }

    Ok(SqlClassification::ReadOnly)
}

/// Classify a single parsed statement.
fn classify_statement(stmt: &sqlparser::ast::Statement) -> Result<SqlClassification> {
    use sqlparser::ast::Statement;

    match stmt {
        // SELECT queries are allowed *only* if no part of the query tree contains a
        // data-modifying statement. PostgreSQL permits data-modifying CTEs
        // (`WITH t AS (DELETE FROM x RETURNING *) SELECT * FROM t`); such a query
        // parses as `Statement::Query` but must NOT be treated as read-only.
        Statement::Query(q) => {
            if query_is_data_modifying(q) {
                Ok(SqlClassification::Rejected(RejectionReason::WritableCte))
            } else {
                Ok(SqlClassification::ReadOnly)
            }
        },

        // EXPLAIN is allowed, but not EXPLAIN ANALYZE (which executes the statement)
        Statement::Explain { analyze, .. } => {
            if *analyze {
                Ok(SqlClassification::Rejected(RejectionReason::ExplainAnalyze))
            } else {
                Ok(SqlClassification::ReadOnly)
            }
        },

        // Explicitly reject write statements
        Statement::Insert { .. } => Ok(SqlClassification::Rejected(
            RejectionReason::WriteStatement("INSERT".to_string()),
        )),
        Statement::Update { .. } => Ok(SqlClassification::Rejected(
            RejectionReason::WriteStatement("UPDATE".to_string()),
        )),
        Statement::Delete { .. } => Ok(SqlClassification::Rejected(
            RejectionReason::WriteStatement("DELETE".to_string()),
        )),

        // Reject DDL
        Statement::CreateTable { .. } => Ok(SqlClassification::Rejected(
            RejectionReason::DdlStatement("CREATE TABLE".to_string()),
        )),
        Statement::CreateView { .. } => Ok(SqlClassification::Rejected(
            RejectionReason::DdlStatement("CREATE VIEW".to_string()),
        )),
        Statement::CreateIndex { .. } => Ok(SqlClassification::Rejected(
            RejectionReason::DdlStatement("CREATE INDEX".to_string()),
        )),
        Statement::CreateSchema { .. } => Ok(SqlClassification::Rejected(
            RejectionReason::DdlStatement("CREATE SCHEMA".to_string()),
        )),
        Statement::CreateRole { .. } => Ok(SqlClassification::Rejected(
            RejectionReason::DdlStatement("CREATE ROLE".to_string()),
        )),
        Statement::CreateExtension { .. } => Ok(SqlClassification::Rejected(
            RejectionReason::DdlStatement("CREATE EXTENSION".to_string()),
        )),
        Statement::CreateSecret { .. } => Ok(SqlClassification::Rejected(
            RejectionReason::DdlStatement("CREATE SECRET".to_string()),
        )),
        Statement::CreateVirtualTable { .. } => Ok(SqlClassification::Rejected(
            RejectionReason::DdlStatement("CREATE VIRTUAL TABLE".to_string()),
        )),
        Statement::Drop { .. } => {
            Ok(SqlClassification::Rejected(RejectionReason::DdlStatement("DROP".to_string())))
        },
        Statement::DropFunction { .. } => Ok(SqlClassification::Rejected(
            RejectionReason::DdlStatement("DROP FUNCTION".to_string()),
        )),
        Statement::DropSecret { .. } => Ok(SqlClassification::Rejected(
            RejectionReason::DdlStatement("DROP SECRET".to_string()),
        )),
        Statement::AlterTable { .. } => Ok(SqlClassification::Rejected(
            RejectionReason::DdlStatement("ALTER TABLE".to_string()),
        )),
        Statement::AlterIndex { .. } => Ok(SqlClassification::Rejected(
            RejectionReason::DdlStatement("ALTER INDEX".to_string()),
        )),
        Statement::AlterView { .. } => Ok(SqlClassification::Rejected(
            RejectionReason::DdlStatement("ALTER VIEW".to_string()),
        )),
        Statement::AlterRole { .. } => Ok(SqlClassification::Rejected(
            RejectionReason::DdlStatement("ALTER ROLE".to_string()),
        )),
        Statement::Truncate { .. } => Ok(SqlClassification::Rejected(
            RejectionReason::DdlStatement("TRUNCATE".to_string()),
        )),

        // Reject privilege escalation. sqlparser 0.62 unified all SET-like statements
        // (SetVariable, SetRole, SetTimeZone, SetNames, SetNamesDefault, plus session
        // params and transaction settings) under `Statement::Set(Set)`.
        Statement::Set(_) => Ok(SqlClassification::Rejected(RejectionReason::PrivilegeEscalation)),

        // Reject procedure calls
        Statement::Call(_) => Ok(SqlClassification::Rejected(RejectionReason::ProcedureCall)),

        // Reject COPY
        Statement::Copy { .. } | Statement::CopyIntoSnowflake { .. } => {
            Ok(SqlClassification::Rejected(RejectionReason::CopyStatement))
        },

        // Reject ANALYZE (Hive statement that may cause writes)
        Statement::Analyze { .. } => Ok(SqlClassification::Rejected(RejectionReason::Unknown(
            "ANALYZE statement not allowed".to_string(),
        ))),

        // Reject other potentially dangerous statements
        Statement::Install { .. } => Ok(SqlClassification::Rejected(RejectionReason::Unknown(
            "INSTALL not allowed".to_string(),
        ))),
        Statement::Load { .. } => Ok(SqlClassification::Rejected(RejectionReason::Unknown(
            "LOAD not allowed".to_string(),
        ))),
        Statement::Directory { .. } => Ok(SqlClassification::Rejected(RejectionReason::Unknown(
            "DIRECTORY not allowed".to_string(),
        ))),
        Statement::AttachDatabase { .. } => Ok(SqlClassification::Rejected(
            RejectionReason::Unknown("ATTACH DATABASE not allowed".to_string()),
        )),
        Statement::AttachDuckDBDatabase { .. } => Ok(SqlClassification::Rejected(
            RejectionReason::Unknown("ATTACH DUCKDB DATABASE not allowed".to_string()),
        )),
        Statement::DetachDuckDBDatabase { .. } => Ok(SqlClassification::Rejected(
            RejectionReason::Unknown("DETACH DUCKDB DATABASE not allowed".to_string()),
        )),
        Statement::Declare { .. } => Ok(SqlClassification::Rejected(RejectionReason::Unknown(
            "DECLARE not allowed".to_string(),
        ))),
        Statement::Close { .. } => Ok(SqlClassification::Rejected(RejectionReason::Unknown(
            "CLOSE not allowed".to_string(),
        ))),
        Statement::Fetch { .. } => Ok(SqlClassification::Rejected(RejectionReason::Unknown(
            "FETCH not allowed".to_string(),
        ))),
        Statement::Flush { .. } => Ok(SqlClassification::Rejected(RejectionReason::Unknown(
            "FLUSH not allowed".to_string(),
        ))),
        Statement::Discard { .. } => Ok(SqlClassification::Rejected(RejectionReason::Unknown(
            "DISCARD not allowed".to_string(),
        ))),
        Statement::StartTransaction { .. } => Ok(SqlClassification::Rejected(
            RejectionReason::Unknown("START TRANSACTION not allowed".to_string()),
        )),
        Statement::Msck { .. } => Ok(SqlClassification::Rejected(RejectionReason::Unknown(
            "MSCK not allowed".to_string(),
        ))),

        // Reject everything else by default (whitelist-only approach)
        _ => Ok(SqlClassification::Rejected(RejectionReason::Unknown(
            format!("{:?}", stmt).chars().take(50).collect(),
        ))),
    }
}

/// Returns `true` if any part of the query tree contains a data-modifying
/// statement (INSERT / UPDATE / DELETE / MERGE), reachable via:
///
/// - the query's `WITH` clause (CTE bodies), including data-modifying CTEs such as `WITH t AS
///   (DELETE FROM x RETURNING *) SELECT * FROM t`
/// - the query body itself and nested set operations / parenthesised subqueries
/// - derived tables (parenthesised subqueries in `FROM`) and nested joins, which may themselves
///   contain data-modifying CTEs
///
/// In sqlparser's AST a data-modifying statement embedded in a query body is
/// represented as `SetExpr::Insert`, `SetExpr::Update`, `SetExpr::Delete`, or
/// `SetExpr::Merge`. `DELETE … RETURNING` inside a CTE is modelled as
/// `SetExpr::Delete`, not as a separate statement.
fn query_is_data_modifying(query: &sqlparser::ast::Query) -> bool {
    // Inspect every CTE body in the WITH clause.
    if let Some(with) = &query.with {
        if with.cte_tables.iter().any(|cte| query_is_data_modifying(&cte.query)) {
            return true;
        }
    }

    // Inspect the query body (and anything nested within it).
    set_expr_is_data_modifying(&query.body)
}

/// Returns `true` if a `SetExpr` is — or transitively contains — a data-modifying
/// statement.
fn set_expr_is_data_modifying(set_expr: &sqlparser::ast::SetExpr) -> bool {
    use sqlparser::ast::SetExpr;

    match set_expr {
        // A data-modifying statement appearing as a query body (e.g. inside a CTE).
        SetExpr::Insert(_) | SetExpr::Update(_) | SetExpr::Delete(_) | SetExpr::Merge(_) => true,

        // A parenthesised subquery — recurse into the nested query (which may carry
        // its own WITH clause / data-modifying body).
        SetExpr::Query(nested) => query_is_data_modifying(nested),

        // UNION / EXCEPT / INTERSECT — both operands must be inspected.
        SetExpr::SetOperation { left, right, .. } => {
            set_expr_is_data_modifying(left) || set_expr_is_data_modifying(right)
        },

        // A SELECT may contain derived-table subqueries in its FROM clause that
        // carry data-modifying CTEs of their own.
        SetExpr::Select(select) => select.from.iter().any(table_with_joins_is_data_modifying),

        // VALUES and TABLE cannot embed a data-modifying statement.
        SetExpr::Values(_) | SetExpr::Table(_) => false,
    }
}

/// Returns `true` if a `TableWithJoins` (a `FROM` entry and its joins) contains a
/// derived subquery with a data-modifying statement.
fn table_with_joins_is_data_modifying(twj: &sqlparser::ast::TableWithJoins) -> bool {
    table_factor_is_data_modifying(&twj.relation)
        || twj.joins.iter().any(|join| table_factor_is_data_modifying(&join.relation))
}

/// Returns `true` if a `TableFactor` is — or contains — a derived subquery with a
/// data-modifying statement.
fn table_factor_is_data_modifying(factor: &sqlparser::ast::TableFactor) -> bool {
    use sqlparser::ast::TableFactor;

    match factor {
        // A parenthesised subquery in FROM: `... FROM (WITH t AS (DELETE …) …) sub`.
        TableFactor::Derived { subquery, .. } => query_is_data_modifying(subquery),

        // A parenthesised join expression — recurse into the wrapped relation.
        TableFactor::NestedJoin {
            table_with_joins, ..
        } => table_with_joins_is_data_modifying(table_with_joins),

        // No other table factor can embed a data-modifying query body.
        _ => false,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)] // Reason: tests use unwrap for concise assertions
mod tests;
