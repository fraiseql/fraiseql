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
/// use fraiseql_core::db::path_escape::escape_postgres_jsonb_segment;
/// assert_eq!(escape_postgres_jsonb_segment("user'name"), "user''name");
/// assert_eq!(escape_postgres_jsonb_segment("normal"), "normal");
/// ```
pub fn escape_postgres_jsonb_segment(segment: &str) -> String {
    segment.replace('\'', "''")
}

/// Escape a full JSON path for use in PostgreSQL JSONB operators.
///
/// # Example
/// ```
/// use fraiseql_core::db::path_escape::escape_postgres_jsonb_path;
/// let path = vec!["user".to_string(), "name".to_string()];
/// let result = escape_postgres_jsonb_path(&path);
/// // Ensures each segment is properly escaped
/// ```
pub fn escape_postgres_jsonb_path(path: &[String]) -> Vec<String> {
    path.iter()
        .map(|segment| escape_postgres_jsonb_segment(segment))
        .collect()
}

/// Escape a JSON path for MySQL JSON_EXTRACT/JSON_UNQUOTE.
///
/// MySQL JSON paths use dot notation: '$.field.subfield'
/// Single quotes must be escaped for SQL string literals.
///
/// # Example
/// ```
/// use fraiseql_core::db::path_escape::escape_mysql_json_path;
/// let path = vec!["user".to_string(), "name".to_string()];
/// let result = escape_mysql_json_path(&path);
/// assert_eq!(result, "$.user.name");
/// ```
pub fn escape_mysql_json_path(path: &[String]) -> String {
    let json_path = path.join(".");
    // Escape single quotes for SQL string literal
    format!("$.{}", json_path.replace('\'', "\\'"))
}

/// Escape a JSON path for SQLite json_extract.
///
/// SQLite JSON paths use dot notation: '$.field.subfield'
/// Single quotes must be escaped for SQL string literals.
pub fn escape_sqlite_json_path(path: &[String]) -> String {
    let json_path = path.join(".");
    format!("$.{}", json_path.replace('\'', "\\'"))
}

/// Escape a JSON path for SQL Server JSON_VALUE.
///
/// SQL Server JSON paths use dot notation: '$.field.subfield'
/// Single quotes must be escaped for SQL string literals.
pub fn escape_sqlserver_json_path(path: &[String]) -> String {
    let json_path = path.join(".");
    format!("$.{}", json_path.replace('\'', "''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_postgres_single_quote() {
        assert_eq!(escape_postgres_jsonb_segment("user'admin"), "user''admin");
    }

    #[test]
    fn test_postgres_multiple_quotes() {
        assert_eq!(escape_postgres_jsonb_segment("it's"), "it''s");
    }

    #[test]
    fn test_postgres_no_quote() {
        assert_eq!(escape_postgres_jsonb_segment("username"), "username");
    }

    #[test]
    fn test_postgres_path_vector() {
        let path = vec!["user'name".to_string(), "id".to_string()];
        let result = escape_postgres_jsonb_path(&path);
        assert_eq!(result[0], "user''name");
        assert_eq!(result[1], "id");
    }

    #[test]
    fn test_mysql_single_quote() {
        let result = escape_mysql_json_path(&["user'admin".to_string()]);
        assert_eq!(result, "$.user\\'admin");
    }

    #[test]
    fn test_sqlite_single_quote() {
        let result = escape_sqlite_json_path(&["user'admin".to_string()]);
        assert_eq!(result, "$.user\\'admin");
    }

    #[test]
    fn test_sqlserver_single_quote() {
        let result = escape_sqlserver_json_path(&["user'admin".to_string()]);
        assert_eq!(result, "$.user''admin");
    }

    #[test]
    fn test_all_databases_empty_path() {
        let empty_path: Vec<String> = vec![];
        let pg_result = escape_postgres_jsonb_path(&empty_path);
        let mysql_result = escape_mysql_json_path(&empty_path);
        let sqlite_result = escape_sqlite_json_path(&empty_path);
        let sqlserver_result = escape_sqlserver_json_path(&empty_path);

        assert_eq!(pg_result.len(), 0);
        assert_eq!(mysql_result, "$.");
        assert_eq!(sqlite_result, "$.");
        assert_eq!(sqlserver_result, "$.");
    }
}
