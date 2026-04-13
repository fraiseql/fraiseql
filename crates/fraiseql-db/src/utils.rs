//! Shared utility functions for the `fraiseql-db` crate.

/// Convert a camelCase field name to snake_case for JSONB key lookup.
///
/// FraiseQL converts schema field names from snake_case to camelCase for GraphQL
/// spec compliance. However, JSONB keys are stored in their original snake_case
/// form. This function reverses that conversion for JSON key access.
///
/// The conversion is idempotent: `to_snake_case("ip_address") == "ip_address"`.
///
/// # Examples
///
/// ```text
/// assert_eq!(to_snake_case("firstName"), "first_name");
/// assert_eq!(to_snake_case("id"), "id");
/// assert_eq!(to_snake_case("ip_address"), "ip_address");
/// ```
pub fn to_snake_case(name: &str) -> String {
    let mut result = String::new();
    for (i, ch) in name.chars().enumerate() {
        if ch.is_uppercase() && i > 0 {
            result.push('_');
            result.push(
                ch.to_lowercase()
                    .next()
                    // Reason: Unicode spec guarantees to_lowercase yields ≥ 1 char
                    .expect("char::to_lowercase always yields at least one char"),
            );
        } else {
            result.push(ch);
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_to_snake_case() {
        assert_eq!(to_snake_case("id"), "id");
        assert_eq!(to_snake_case("firstName"), "first_name");
        assert_eq!(to_snake_case("createdAt"), "created_at");
        assert_eq!(to_snake_case("userId"), "user_id");
        assert_eq!(to_snake_case("updatedAtTimestamp"), "updated_at_timestamp");
        assert_eq!(to_snake_case("ipAddress"), "ip_address");
    }

    #[test]
    fn test_to_snake_case_idempotent() {
        assert_eq!(to_snake_case("ip_address"), "ip_address");
        assert_eq!(to_snake_case("first_name"), "first_name");
        assert_eq!(to_snake_case("id"), "id");
    }
}
