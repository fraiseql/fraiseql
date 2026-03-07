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
    path.iter().map(|segment| escape_postgres_jsonb_segment(segment)).collect()
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

    // =========================================================================
    // Injection payload tests — 4 dialects × 10 payloads = 40 tests
    // =========================================================================

    // --- PostgreSQL segment escape ---

    #[test]
    fn test_postgres_injection_drop_table() {
        let payload = "'; DROP TABLE users; --";
        let escaped = escape_postgres_jsonb_segment(payload);
        // Single quotes must be doubled so they cannot break out of a SQL string literal
        // Payload starts with ' → becomes '' in the output
        assert!(escaped.starts_with("''"), "Opening single quote must be doubled for PostgreSQL");
        assert!(!escaped.starts_with("'\""), "Must not produce an unescaped sequence");
    }

    #[test]
    fn test_postgres_injection_or_1_eq_1() {
        let payload = "' OR '1'='1";
        let escaped = escape_postgres_jsonb_segment(payload);
        // All single quotes must be doubled — count of '' should match original ' count
        let original_quote_count = payload.chars().filter(|&c| c == '\'').count();
        let doubled_count = escaped.matches("''").count();
        assert_eq!(doubled_count, original_quote_count, "Every single quote must be doubled");
    }

    #[test]
    fn test_postgres_injection_double_quote_or() {
        let payload = r#"" OR "1"="1"#;
        let escaped = escape_postgres_jsonb_segment(payload);
        // No single quotes in payload — output must be identical (double quotes are not special in PG segment)
        assert_eq!(escaped, payload);
    }

    #[test]
    fn test_postgres_injection_backslash() {
        let payload = r"\";
        let escaped = escape_postgres_jsonb_segment(payload);
        // PostgreSQL does not treat backslash specially in dollar-quoted / JSONB operators; output is unchanged
        assert_eq!(escaped, payload);
    }

    #[test]
    fn test_postgres_injection_like_percent() {
        let payload = "%";
        let escaped = escape_postgres_jsonb_segment(payload);
        // No single quotes — output unchanged
        assert_eq!(escaped, payload);
    }

    #[test]
    fn test_postgres_injection_like_underscore() {
        let payload = "_";
        let escaped = escape_postgres_jsonb_segment(payload);
        assert_eq!(escaped, payload);
    }

    #[test]
    fn test_postgres_injection_xss_script_tag() {
        let payload = "<script>alert(1)</script>";
        let escaped = escape_postgres_jsonb_segment(payload);
        // No single quotes in XSS payload — output identical
        assert_eq!(escaped, payload);
    }

    #[test]
    fn test_postgres_injection_null_literal() {
        let payload = "NULL";
        let escaped = escape_postgres_jsonb_segment(payload);
        assert_eq!(escaped, "NULL");
    }

    #[test]
    fn test_postgres_injection_empty_string() {
        let payload = "";
        let escaped = escape_postgres_jsonb_segment(payload);
        assert_eq!(escaped, "");
    }

    #[test]
    fn test_postgres_injection_unicode_accents() {
        let payload = "François";
        let escaped = escape_postgres_jsonb_segment(payload);
        // No single quotes — output unchanged
        assert_eq!(escaped, "François");
    }

    // --- MySQL JSON path escape ---

    #[test]
    fn test_mysql_injection_drop_table() {
        let payload = "'; DROP TABLE users; --";
        let result = escape_mysql_json_path(&[payload.to_string()]);
        // MySQL uses backslash escaping: ' → \'
        // The result should contain \' (backslash then single quote) not bare '
        assert!(result.contains("\\'"), "Single quote must be backslash-escaped for MySQL");
        // Path must start with $.
        assert!(result.starts_with("$."), "MySQL path must start with $.");
    }

    #[test]
    fn test_mysql_injection_or_1_eq_1() {
        let payload = "' OR '1'='1";
        let result = escape_mysql_json_path(&[payload.to_string()]);
        // All 4 single quotes in "' OR '1'='1" must be escaped with backslash
        let original_quote_count = payload.chars().filter(|&c| c == '\'').count();
        let escaped_count = result.matches("\\'").count();
        assert_eq!(escaped_count, original_quote_count, "Every single quote must be backslash-escaped in MySQL");
    }

    #[test]
    fn test_mysql_injection_double_quote_or() {
        let payload = r#"" OR "1"="1"#;
        let result = escape_mysql_json_path(&[payload.to_string()]);
        // No single quotes — path contains original (double quotes are not special in MySQL JSON path string)
        assert!(result.starts_with("$."), "MySQL path must start with '$.'");
    }

    #[test]
    fn test_mysql_injection_backslash() {
        let payload = r"\";
        let result = escape_mysql_json_path(&[payload.to_string()]);
        assert!(result.starts_with("$."), "MySQL path must start with '$.'");
    }

    #[test]
    fn test_mysql_injection_like_percent() {
        let payload = "%";
        let result = escape_mysql_json_path(&[payload.to_string()]);
        assert_eq!(result, "$.%");
    }

    #[test]
    fn test_mysql_injection_like_underscore() {
        let payload = "_";
        let result = escape_mysql_json_path(&[payload.to_string()]);
        assert_eq!(result, "$._");
    }

    #[test]
    fn test_mysql_injection_xss_script_tag() {
        let payload = "<script>alert(1)</script>";
        let result = escape_mysql_json_path(&[payload.to_string()]);
        assert!(result.starts_with("$."), "MySQL path must start with '$.'");
        assert!(!result.contains("'; "), "Should not contain unescaped quotes");
    }

    #[test]
    fn test_mysql_injection_null_literal() {
        let payload = "NULL";
        let result = escape_mysql_json_path(&[payload.to_string()]);
        assert_eq!(result, "$.NULL");
    }

    #[test]
    fn test_mysql_injection_empty_segment() {
        // Single empty segment in path
        let result = escape_mysql_json_path(&[String::new()]);
        assert_eq!(result, "$.");
    }

    #[test]
    fn test_mysql_injection_unicode_accents() {
        let payload = "François";
        let result = escape_mysql_json_path(&[payload.to_string()]);
        assert_eq!(result, "$.François");
    }

    // --- SQLite JSON path escape ---

    #[test]
    fn test_sqlite_injection_drop_table() {
        let payload = "'; DROP TABLE users; --";
        let result = escape_sqlite_json_path(&[payload.to_string()]);
        // SQLite uses backslash escaping: ' → \' (same as MySQL)
        assert!(result.contains("\\'"), "Single quote must be backslash-escaped for SQLite");
        assert!(result.starts_with("$."), "SQLite path must start with $.");
    }

    #[test]
    fn test_sqlite_injection_or_1_eq_1() {
        let payload = "' OR '1'='1";
        let result = escape_sqlite_json_path(&[payload.to_string()]);
        let original_quote_count = payload.chars().filter(|&c| c == '\'').count();
        let escaped_count = result.matches("\\'").count();
        assert_eq!(escaped_count, original_quote_count, "Every single quote must be backslash-escaped in SQLite");
    }

    #[test]
    fn test_sqlite_injection_double_quote_or() {
        let payload = r#"" OR "1"="1"#;
        let result = escape_sqlite_json_path(&[payload.to_string()]);
        assert!(result.starts_with("$."), "SQLite path must start with '$.'");
    }

    #[test]
    fn test_sqlite_injection_backslash() {
        let payload = r"\";
        let result = escape_sqlite_json_path(&[payload.to_string()]);
        assert!(result.starts_with("$."), "SQLite path must start with '$.'");
    }

    #[test]
    fn test_sqlite_injection_like_percent() {
        let payload = "%";
        let result = escape_sqlite_json_path(&[payload.to_string()]);
        assert_eq!(result, "$.%");
    }

    #[test]
    fn test_sqlite_injection_like_underscore() {
        let payload = "_";
        let result = escape_sqlite_json_path(&[payload.to_string()]);
        assert_eq!(result, "$._");
    }

    #[test]
    fn test_sqlite_injection_xss_script_tag() {
        let payload = "<script>alert(1)</script>";
        let result = escape_sqlite_json_path(&[payload.to_string()]);
        assert!(result.starts_with("$."), "SQLite path must start with '$.'");
    }

    #[test]
    fn test_sqlite_injection_null_literal() {
        let payload = "NULL";
        let result = escape_sqlite_json_path(&[payload.to_string()]);
        assert_eq!(result, "$.NULL");
    }

    #[test]
    fn test_sqlite_injection_empty_segment() {
        let result = escape_sqlite_json_path(&[String::new()]);
        assert_eq!(result, "$.");
    }

    #[test]
    fn test_sqlite_injection_unicode_accents() {
        let payload = "François";
        let result = escape_sqlite_json_path(&[payload.to_string()]);
        assert_eq!(result, "$.François");
    }

    // --- SQL Server JSON path escape ---

    #[test]
    fn test_sqlserver_injection_drop_table() {
        let payload = "'; DROP TABLE users; --";
        let result = escape_sqlserver_json_path(&[payload.to_string()]);
        // SQL Server uses doubling: ' → '' (same as PostgreSQL)
        assert!(result.contains("''"), "Single quote must be doubled in SQL Server");
        assert!(result.starts_with("$."), "SQL Server path must start with $.");
    }

    #[test]
    fn test_sqlserver_injection_or_1_eq_1() {
        let payload = "' OR '1'='1";
        let result = escape_sqlserver_json_path(&[payload.to_string()]);
        let original_quote_count = payload.chars().filter(|&c| c == '\'').count();
        let doubled_count = result.matches("''").count();
        assert_eq!(doubled_count, original_quote_count, "Every single quote must be doubled in SQL Server");
    }

    #[test]
    fn test_sqlserver_injection_double_quote_or() {
        let payload = r#"" OR "1"="1"#;
        let result = escape_sqlserver_json_path(&[payload.to_string()]);
        assert!(result.starts_with("$."), "SQL Server path must start with '$.'");
    }

    #[test]
    fn test_sqlserver_injection_backslash() {
        let payload = r"\";
        let result = escape_sqlserver_json_path(&[payload.to_string()]);
        assert!(result.starts_with("$."), "SQL Server path must start with '$.'");
    }

    #[test]
    fn test_sqlserver_injection_like_percent() {
        let payload = "%";
        let result = escape_sqlserver_json_path(&[payload.to_string()]);
        assert_eq!(result, "$.%");
    }

    #[test]
    fn test_sqlserver_injection_like_underscore() {
        let payload = "_";
        let result = escape_sqlserver_json_path(&[payload.to_string()]);
        assert_eq!(result, "$._");
    }

    #[test]
    fn test_sqlserver_injection_xss_script_tag() {
        let payload = "<script>alert(1)</script>";
        let result = escape_sqlserver_json_path(&[payload.to_string()]);
        assert!(result.starts_with("$."), "SQL Server path must start with '$.'");
    }

    #[test]
    fn test_sqlserver_injection_null_literal() {
        let payload = "NULL";
        let result = escape_sqlserver_json_path(&[payload.to_string()]);
        assert_eq!(result, "$.NULL");
    }

    #[test]
    fn test_sqlserver_injection_empty_segment() {
        let result = escape_sqlserver_json_path(&[String::new()]);
        assert_eq!(result, "$.");
    }

    #[test]
    fn test_sqlserver_injection_unicode_accents() {
        let payload = "François";
        let result = escape_sqlserver_json_path(&[payload.to_string()]);
        assert_eq!(result, "$.François");
    }

    // --- Cross-dialect consistency checks ---

    #[test]
    fn test_postgres_segment_double_single_quote_roundtrip() {
        // A single quote in input → doubled in output
        let input = "it's";
        let escaped = escape_postgres_jsonb_segment(input);
        assert_eq!(escaped, "it''s");
    }

    #[test]
    fn test_mysql_vs_sqlite_same_escaping_for_single_quote() {
        let payload = "user'name";
        let mysql_result = escape_mysql_json_path(&[payload.to_string()]);
        let sqlite_result = escape_sqlite_json_path(&[payload.to_string()]);
        // Both use backslash escaping
        assert_eq!(mysql_result, sqlite_result, "MySQL and SQLite should escape single quotes identically");
    }

    #[test]
    fn test_sqlserver_vs_postgres_same_doubling_strategy() {
        let payload = "user'name";
        let pg_seg = escape_postgres_jsonb_segment(payload);
        let ss_result = escape_sqlserver_json_path(&[payload.to_string()]);
        // PostgreSQL doubles the quote in the segment; SQL Server doubles it in the path body
        assert!(pg_seg.contains("''"), "PostgreSQL should double the quote");
        assert!(ss_result.contains("''"), "SQL Server should double the quote");
    }

    #[test]
    fn test_postgres_path_multi_segment_escaping() {
        let path = vec!["user'name".to_string(), "field's".to_string()];
        let result = escape_postgres_jsonb_path(&path);
        assert_eq!(result[0], "user''name");
        assert_eq!(result[1], "field''s");
    }

    #[test]
    fn test_mysql_multi_segment_path_joins_with_dot() {
        let path = vec!["user".to_string(), "address".to_string(), "city".to_string()];
        let result = escape_mysql_json_path(&path);
        assert_eq!(result, "$.user.address.city");
    }
}
