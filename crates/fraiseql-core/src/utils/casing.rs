//! Field name case conversion (camelCase → `snake_case`).
//!
//! This module handles converting GraphQL field names (typically camelCase)
//! to PostgreSQL column names (typically `snake_case`).

/// Convert camelCase or `PascalCase` to `snake_case`.
///
/// Follows the standard GraphQL-to-SQL field name convention used across all
/// authoring languages: `camelCase` GraphQL names → `snake_case` column names.
///
/// # Examples
///
/// ```
/// use fraiseql_core::utils::casing::to_snake_case;
///
/// assert_eq!(to_snake_case("userId"), "user_id");
/// assert_eq!(to_snake_case("createdAt"), "created_at");
/// assert_eq!(to_snake_case("HTTPResponse"), "http_response");
/// assert_eq!(to_snake_case("already_snake"), "already_snake");
/// ```
#[must_use]
pub fn to_snake_case(s: &str) -> String {
    // If already snake_case (no uppercase letters), return as-is
    if !s.chars().any(char::is_uppercase) {
        return s.to_string();
    }

    let mut result = String::with_capacity(s.len() + 5);
    let mut prev_was_upper = false;
    let mut prev_was_lower = false;

    for (i, c) in s.chars().enumerate() {
        if c.is_uppercase() {
            // Add underscore before uppercase if:
            // 1. Not the first character
            // 2. Previous was lowercase OR next is lowercase (handles "HTTPResponse" →
            //    "http_response")
            if i > 0 {
                let next_is_lower = s.chars().nth(i + 1).is_some_and(char::is_lowercase);
                if prev_was_lower || (prev_was_upper && next_is_lower) {
                    result.push('_');
                }
            }
            result.push(c.to_ascii_lowercase());
            prev_was_upper = true;
            prev_was_lower = false;
        } else {
            result.push(c);
            prev_was_upper = false;
            prev_was_lower = c.is_lowercase();
        }
    }

    result
}

/// Convert `snake_case` to camelCase.
///
/// This is the reverse operation, used for output formatting.
///
/// # Examples
///
/// ```
/// use fraiseql_core::utils::casing::to_camel_case;
///
/// assert_eq!(to_camel_case("user_id"), "userId");
/// assert_eq!(to_camel_case("created_at"), "createdAt");
/// assert_eq!(to_camel_case("http_response"), "httpResponse");
/// assert_eq!(to_camel_case("alreadyCamel"), "alreadyCamel");
/// ```
#[must_use]
pub fn to_camel_case(s: &str) -> String {
    // If no underscores, assume already camelCase
    if !s.contains('_') {
        return s.to_string();
    }

    let mut result = String::with_capacity(s.len());
    let mut capitalize_next = false;

    for c in s.chars() {
        if c == '_' {
            capitalize_next = true;
        } else if capitalize_next {
            result.push(c.to_ascii_uppercase());
            capitalize_next = false;
        } else {
            result.push(c);
        }
    }

    result
}

/// Normalize a field path for database access.
///
/// This handles dotted paths like "user.profile.name" and converts each segment.
///
/// # Examples
///
/// ```
/// use fraiseql_core::utils::casing::normalize_field_path;
///
/// assert_eq!(normalize_field_path("userId"), "user_id");
/// assert_eq!(normalize_field_path("user.createdAt"), "user.created_at");
/// assert_eq!(normalize_field_path("device.sensor.currentValue"), "device.sensor.current_value");
/// ```
#[must_use]
pub fn normalize_field_path(path: &str) -> String {
    if !path.contains('.') {
        return to_snake_case(path);
    }

    path.split('.').map(to_snake_case).collect::<Vec<_>>().join(".")
}
