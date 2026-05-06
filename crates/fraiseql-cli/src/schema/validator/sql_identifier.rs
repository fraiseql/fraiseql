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
            message:    format!(
                "`{field}` at `{path}` must not be empty. \
                 Provide a view or function name such as \"v_user\" or \"public.v_user\"."
            ),
            path:       path.to_string(),
            severity:   ErrorSeverity::Error,
            suggestion: None,
        });
    }

    // Check each segment against PostgreSQL's NAMEDATALEN limit.
    for segment in value.split('.') {
        if segment.len() > PG_MAX_IDENTIFIER_BYTES {
            return Err(ValidationError {
                message:    format!(
                    "`{field}` segment {segment:?} at `{path}` is {} bytes, \
                     which exceeds the PostgreSQL maximum of {PG_MAX_IDENTIFIER_BYTES} bytes. \
                     PostgreSQL silently truncates longer identifiers, causing \
                     \"relation not found\" errors at runtime.",
                    segment.len(),
                ),
                path:       path.to_string(),
                severity:   ErrorSeverity::Error,
                suggestion: Some("Shorten the identifier to 63 characters or fewer.".to_string()),
            });
        }
    }

    if !SAFE_IDENTIFIER.is_match(value) {
        return Err(ValidationError {
            message:    format!(
                "`{field}` value {value:?} at `{path}` is not a valid SQL identifier. \
                 Only ASCII letters, digits, underscores, and up to two schema dots are \
                 allowed (1-3 segments). Valid examples: \"v_user\", \"public.v_user\", \
                 \"catalog.schema.table\"."
            ),
            path:       path.to_string(),
            severity:   ErrorSeverity::Error,
            suggestion: Some(
                "Remove semicolons, quotes, dashes, spaces, or any SQL syntax \
                 from the identifier value."
                    .to_string(),
            ),
        });
    }
    Ok(())
}

