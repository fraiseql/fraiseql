//! SQL identifier validation.

use std::sync::LazyLock;

use regex::Regex;

use super::types::{ErrorSeverity, ValidationError};

/// Pattern for safe SQL identifiers: up to three dot-separated segments
/// (`name`, `schema.name`, or `catalog.schema.name`).
/// Each segment must start with a letter or underscore, followed by alphanumerics/underscores.
static SAFE_IDENTIFIER: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^[A-Za-z_][A-Za-z0-9_]*(\.[A-Za-z_][A-Za-z0-9_]*){0,2}$")
        .expect("static regex is valid")
});

/// PostgreSQL's `NAMEDATALEN - 1`: the maximum byte length of a single identifier segment.
const PG_MAX_IDENTIFIER_BYTES: usize = 63;

/// Validates that `value` is a safe SQL identifier.
///
/// Accepts `[A-Za-z_][A-Za-z0-9_]*` with up to two schema dots
/// (e.g. `"v_user"`, `"public.v_user"`, or `"catalog.schema.table"`).
/// Rejects anything that could be SQL injection or cause a runtime syntax error.
///
/// Each dot-separated segment is limited to 63 bytes (PostgreSQL `NAMEDATALEN - 1`).
/// Identifiers exceeding this limit are silently truncated by PostgreSQL, which can
/// cause confusing "relation not found" errors at runtime.
///
/// # Arguments
/// - `value`: The string to validate (e.g. `"v_user"` or `"public.v_user"`)
/// - `field`: The TOML/decorator field name (`"sql_source"`, `"function_name"`)
/// - `path`: Human-readable location for the error (`"Query.users"`, `"Mutation.createPost"`)
///
/// # Errors
///
/// Returns a `ValidationError` if `value` is empty, exceeds the PostgreSQL identifier
/// length limit, or does not match the safe identifier pattern.
pub fn validate_sql_identifier(
    value: &str,
    field: &str,
    path: &str,
) -> std::result::Result<(), ValidationError> {
    if value.is_empty() {
        return Err(ValidationError {
            message: format!(
                "`{field}` at `{path}` must not be empty. \
                 Provide a view or function name such as \"v_user\" or \"public.v_user\"."
            ),
            path: path.to_string(),
            severity: ErrorSeverity::Error,
            suggestion: None,
        });
    }

    // Check each segment against PostgreSQL's NAMEDATALEN limit.
    for segment in value.split('.') {
        if segment.len() > PG_MAX_IDENTIFIER_BYTES {
            return Err(ValidationError {
                message: format!(
                    "`{field}` segment {segment:?} at `{path}` is {} bytes, \
                     which exceeds the PostgreSQL maximum of {PG_MAX_IDENTIFIER_BYTES} bytes. \
                     PostgreSQL silently truncates longer identifiers, causing \
                     \"relation not found\" errors at runtime.",
                    segment.len(),
                ),
                path: path.to_string(),
                severity: ErrorSeverity::Error,
                suggestion: Some("Shorten the identifier to 63 characters or fewer.".to_string()),
            });
        }
    }

    if !SAFE_IDENTIFIER.is_match(value) {
        return Err(ValidationError {
            message: format!(
                "`{field}` value {value:?} at `{path}` is not a valid SQL identifier. \
                 Only ASCII letters, digits, underscores, and up to two schema dots are \
                 allowed (1-3 segments). Valid examples: \"v_user\", \"public.v_user\", \
                 \"catalog.schema.table\"."
            ),
            path: path.to_string(),
            severity: ErrorSeverity::Error,
            suggestion: Some(
                "Remove semicolons, quotes, dashes, spaces, or any SQL syntax \
                 from the identifier value."
                    .to_string(),
            ),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable

    use super::*;

    #[test]
    fn test_valid_simple_identifier() {
        validate_sql_identifier("v_user", "sql_source", "Query.users")
            .unwrap_or_else(|e| panic!("expected Ok: {e:?}"));
    }

    #[test]
    fn test_valid_schema_qualified_identifier() {
        validate_sql_identifier("public.v_user", "sql_source", "Query.users")
            .unwrap_or_else(|e| panic!("expected Ok: {e:?}"));
    }

    #[test]
    fn test_empty_identifier_rejected() {
        let err = validate_sql_identifier("", "sql_source", "Query.users").unwrap_err();
        assert!(err.message.contains("must not be empty"));
    }

    #[test]
    fn test_identifier_exactly_63_bytes_accepted() {
        let ident = "a".repeat(63);
        validate_sql_identifier(&ident, "sql_source", "Query.x")
            .unwrap_or_else(|e| panic!("expected Ok: {e:?}"));
    }

    #[test]
    fn test_identifier_64_bytes_rejected() {
        let ident = "a".repeat(64);
        let err = validate_sql_identifier(&ident, "sql_source", "Query.x").unwrap_err();
        assert!(err.message.contains("exceeds the PostgreSQL maximum"));
        assert!(err.message.contains("63 bytes"));
    }

    #[test]
    fn test_schema_segment_64_bytes_rejected() {
        // The schema part (before the dot) is 64 chars — should fail on that segment.
        let schema_part = "a".repeat(64);
        let ident = format!("{schema_part}.v_user");
        let err = validate_sql_identifier(&ident, "sql_source", "Query.x").unwrap_err();
        assert!(err.message.contains("exceeds the PostgreSQL maximum"));
    }

    #[test]
    fn test_name_segment_64_bytes_rejected() {
        // The name part (after the dot) is 64 chars — should fail on that segment.
        let name_part = "a".repeat(64);
        let ident = format!("public.{name_part}");
        let err = validate_sql_identifier(&ident, "sql_source", "Query.x").unwrap_err();
        assert!(err.message.contains("exceeds the PostgreSQL maximum"));
    }

    #[test]
    fn test_valid_three_part_identifier() {
        assert!(validate_sql_identifier("catalog.schema.table", "sql_source", "Query.x").is_ok());
    }

    #[test]
    fn test_four_part_identifier_rejected() {
        let err = validate_sql_identifier("a.b.c.d", "sql_source", "Query.x").unwrap_err();
        assert!(err.message.contains("is not a valid SQL identifier"));
    }

    #[test]
    fn test_leading_dot_rejected() {
        let err = validate_sql_identifier(".foo", "sql_source", "Query.x").unwrap_err();
        assert!(err.message.contains("is not a valid SQL identifier"));
    }

    #[test]
    fn test_trailing_dot_rejected() {
        let err = validate_sql_identifier("foo.", "sql_source", "Query.x").unwrap_err();
        assert!(err.message.contains("is not a valid SQL identifier"));
    }

    #[test]
    fn test_double_dot_rejected() {
        let err = validate_sql_identifier("foo..bar", "sql_source", "Query.x").unwrap_err();
        assert!(err.message.contains("is not a valid SQL identifier"));
    }

    #[test]
    fn test_injection_attempt_rejected() {
        let err = validate_sql_identifier("v_user; DROP TABLE users", "sql_source", "Query.users")
            .unwrap_err();
        assert!(err.message.contains("is not a valid SQL identifier"));
    }
}
