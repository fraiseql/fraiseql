//! Shared utility functions for the `fraiseql-db` crate.

/// Convert a camelCase / `PascalCase` field name to `snake_case` for JSONB key
/// lookup.
///
/// FraiseQL exposes schema field names as camelCase for GraphQL spec compliance,
/// but the underlying JSONB keys (view `data` columns, mutation entity payloads)
/// are stored in their original `snake_case` form. This function reverses that
/// conversion so projection can read the stored key.
///
/// This is the **single, canonical** field-name → JSONB-key rule for the whole
/// engine: the SQL projection generators (this crate) and the Rust entity
/// projector (`fraiseql-core`) both call it, so they can never disagree on a
/// source key. It is acronym-aware — `"HTTPResponse"` → `"http_response"`, not
/// `"h_t_t_p_response"` — and idempotent: `to_snake_case("ip_address")` ==
/// `"ip_address"`.
///
/// # Examples
///
/// ```
/// use fraiseql_db::utils::to_snake_case;
///
/// assert_eq!(to_snake_case("userId"), "user_id");
/// assert_eq!(to_snake_case("createdAt"), "created_at");
/// assert_eq!(to_snake_case("HTTPResponse"), "http_response");
/// assert_eq!(to_snake_case("already_snake"), "already_snake");
/// ```
#[must_use]
pub fn to_snake_case(name: &str) -> String {
    // Already snake_case (no uppercase letters) — return as-is (idempotent).
    if !name.chars().any(char::is_uppercase) {
        return name.to_string();
    }

    let mut result = String::with_capacity(name.len() + 5);
    let mut prev_was_upper = false;
    let mut prev_was_lower = false;

    for (i, c) in name.chars().enumerate() {
        if c.is_uppercase() {
            // Insert a boundary underscore before an uppercase char when the
            // previous char was lowercase, or when leaving an acronym run into a
            // new word (prev upper, next lower) — e.g. "HTTPResponse".
            if i > 0 {
                let next_is_lower = name.chars().nth(i + 1).is_some_and(char::is_lowercase);
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

#[cfg(test)]
mod tests;
