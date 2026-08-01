//! Escape utilities for JSON path SQL injection prevention.
//!
//! PostgreSQL JSONB operators take the path as a string literal: single quotes
//! are doubled. This escaper is defence in depth: the parse boundary
//! (`utils::validate_graphql_identifier`, enforced by the `where` and `orderBy`
//! parsers, #833) rejects any field name outside `[_A-Za-z][_0-9A-Za-z]*`
//! before a path reaches SQL generation.

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

#[cfg(test)]
mod tests;
