//! Field name case conversion (camelCase → `snake_case`).
//!
//! This module handles converting GraphQL field names (typically camelCase)
//! to PostgreSQL column names (typically `snake_case`).

/// The canonical field-name → `snake_case` JSONB-key rule.
///
/// Re-exported from `fraiseql-db` so the SQL projection generators and this
/// crate's Rust entity projector share **one** definition — eliminating the
/// historical drift where two copies disagreed on acronym field names
/// (`userID` → `user_id`, never `user_i_d`). See
/// [`fraiseql_db::utils::to_snake_case`].
pub use fraiseql_db::utils::{set_runtime_acronyms, to_snake_case};

/// Convert `snake_case` to camelCase.
///
/// This is the inverse of [`to_snake_case`]: a digit segment collapses onto the
/// previous word (`phone_1` → `phone1`), and `to_snake_case` reinserts the
/// boundary (`phone1` → `phone_1`), so the round trip is bijective. See
/// [`to_snake_case`] for the digit caveat (`oauth2`/`ipv4`/`s3`).
///
/// # Examples
///
/// ```
/// use fraiseql_core::utils::casing::to_camel_case;
///
/// assert_eq!(to_camel_case("user_id"), "userId");
/// assert_eq!(to_camel_case("created_at"), "createdAt");
/// assert_eq!(to_camel_case("http_response"), "httpResponse");
/// assert_eq!(to_camel_case("phone_1"), "phone1");
/// assert_eq!(to_camel_case("dns_1_id"), "dns1Id");
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
