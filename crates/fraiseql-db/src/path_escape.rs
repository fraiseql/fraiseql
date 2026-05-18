//! Escape utilities for JSON path SQL injection prevention.
//!
//! Different databases have different escaping requirements for JSON paths:
//! - PostgreSQL: Single quote in JSONB operators -> double it
//! - MySQL: Single quote in JSON_EXTRACT -> escape with backslash
//! - SQLite: Single quote in json_extract -> escape with backslash
//! - SQL Server: Single quote in JSON_VALUE -> double it

/// Escape a single path segment for use in PostgreSQL JSONB operators.
///
/// PostgreSQL JSONB operators (->,'->>',->) are literal string operators
/// where the right operand is interpreted as a JSON key string.
/// Single quotes within the string must be doubled for SQL escaping.
///
/// # Example
/// ```
/// use fraiseql_db::path_escape::escape_postgres_jsonb_segment;
/// assert_eq!(escape_postgres_jsonb_segment("user'name"), "user''name");
/// assert_eq!(escape_postgres_jsonb_segment("normal"), "normal");
/// ```
#[must_use] 
pub fn escape_postgres_jsonb_segment(segment: &str) -> String {
    segment.replace('\'', "''")
}

/// Escape a full JSON path for use in PostgreSQL JSONB operators.
///
/// # Example
/// ```
/// use fraiseql_db::path_escape::escape_postgres_jsonb_path;
/// let path = vec!["user".to_string(), "name".to_string()];
/// let result = escape_postgres_jsonb_path(&path);
/// // Ensures each segment is properly escaped
/// ```
#[must_use] 
pub fn escape_postgres_jsonb_path(path: &[String]) -> Vec<String> {
    path.iter().map(|segment| escape_postgres_jsonb_segment(segment)).collect()
}

/// Escape a JSON path for MySQL JSON_EXTRACT/JSON_UNQUOTE.
///
/// MySQL JSON paths use dot notation: '$.field.subfield'
/// Single quotes are doubled (`''`) rather than backslash-escaped so that the
/// path is safe even when the server runs with `NO_BACKSLASH_ESCAPES` mode.
///
/// # Example
/// ```
/// use fraiseql_db::path_escape::escape_mysql_json_path;
/// let path = vec!["user".to_string(), "name".to_string()];
/// let result = escape_mysql_json_path(&path);
/// assert_eq!(result, "$.user.name");
/// ```
#[must_use] 
pub fn escape_mysql_json_path(path: &[String]) -> String {
    let json_path = path.join(".");
    // Double single quotes for SQL string literal; safe under NO_BACKSLASH_ESCAPES.
    format!("$.{}", json_path.replace('\'', "''"))
}

/// Escape a JSON path for SQLite json_extract.
///
/// SQLite JSON paths use dot notation: '$.field.subfield'
/// Single quotes are doubled (`''`) rather than backslash-escaped so that the
/// path is safe regardless of SQLite compile-time escape settings.
#[must_use] 
pub fn escape_sqlite_json_path(path: &[String]) -> String {
    let json_path = path.join(".");
    // Double single quotes for SQL string literal; backslash escaping is not
    // a reliable cross-mode choice for SQLite.
    format!("$.{}", json_path.replace('\'', "''"))
}

/// Escape a JSON path for SQL Server JSON_VALUE.
///
/// SQL Server JSON paths use dot notation: '$.field.subfield'
/// Single quotes must be escaped for SQL string literals.
#[must_use] 
pub fn escape_sqlserver_json_path(path: &[String]) -> String {
    let json_path = path.join(".");
    format!("$.{}", json_path.replace('\'', "''"))
}

#[cfg(test)]
mod tests;
