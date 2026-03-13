/// Validates that a string is a safe SQL identifier.
///
/// Accepts only ASCII alphanumerics and underscores (no spaces, semicolons,
/// hyphens, or other characters). Used to guard against SQL injection when
/// schema-derived names such as view names or entity type names are
/// interpolated into raw SQL strings.
///
/// # Rules
/// - Non-empty
/// - Maximum 128 characters
/// - All characters are `[A-Za-z0-9_]`
///
/// # Examples
///
/// ```
/// use fraiseql_core::schema::is_safe_sql_identifier;
///
/// assert!(is_safe_sql_identifier("v_users"));
/// assert!(is_safe_sql_identifier("Order123"));
/// assert!(!is_safe_sql_identifier("users; DROP TABLE users"));
/// assert!(!is_safe_sql_identifier(""));
/// ```
pub fn is_safe_sql_identifier(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 128
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}
