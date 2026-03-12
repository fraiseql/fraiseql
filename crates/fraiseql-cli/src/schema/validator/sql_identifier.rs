//! SQL identifier validation.

use std::sync::LazyLock;

use regex::Regex;

use super::types::{ErrorSeverity, ValidationError};

/// Pattern for safe SQL identifiers: `schema.name` or just `name`.
/// Each part must start with a letter or underscore, followed by alphanumerics/underscores.
static SAFE_IDENTIFIER: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^[A-Za-z_][A-Za-z0-9_]*(\.[A-Za-z_][A-Za-z0-9_]*)?$")
        .expect("static regex is valid")
});

/// Validates that `value` is a safe SQL identifier.
///
/// Accepts `[A-Za-z_][A-Za-z0-9_]*` with an optional single schema dot
/// (e.g. `"v_user"` or `"public.v_user"`). Rejects anything that could be
/// SQL injection or cause a runtime syntax error.
///
/// # Arguments
/// - `value`: The string to validate (e.g. `"v_user"` or `"public.v_user"`)
/// - `field`: The TOML/decorator field name (`"sql_source"`, `"function_name"`)
/// - `path`: Human-readable location for the error (`"Query.users"`, `"Mutation.createPost"`)
///
/// # Errors
///
/// Returns a `ValidationError` if `value` is empty or does not match the safe
/// identifier pattern.
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
    if !SAFE_IDENTIFIER.is_match(value) {
        return Err(ValidationError {
            message:    format!(
                "`{field}` value {value:?} at `{path}` is not a valid SQL identifier. \
                 Only ASCII letters, digits, underscores, and an optional schema dot are \
                 allowed. Valid examples: \"v_user\", \"public.v_user\", \"fn_create_post\"."
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
