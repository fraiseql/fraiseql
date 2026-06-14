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
/// # Digit boundaries
///
/// A digit segment is a word of its own, mirroring the inverse of
/// [`to_camel_case`](fraiseql_core::utils::casing::to_camel_case) (which collapses
/// `phone_1` → `phone1`). This function reconstructs the underscore so the round
/// trip is bijective:
/// - a digit after a lowercase letter gets a boundary: `"phone1"` → `"phone_1"`;
/// - an uppercase word after a digit gets one too: `"dns1Id"` → `"dns_1_id"`.
///
/// Consequence (matches FraiseQL v1): identifiers like `oauth2`, `ipv4`, `s3` are
/// read as a letter word + a digit word, so they reverse to `oauth_2`, `ipv_4`,
/// `s_3`. A field whose JSONB key is a literal `ipv4` (no underscore) therefore
/// will not round-trip — author the key as `ipv_4`, or give the field an explicit
/// GraphQL alias.
///
/// # Examples
///
/// ```
/// use fraiseql_db::utils::to_snake_case;
///
/// assert_eq!(to_snake_case("userId"), "user_id");
/// assert_eq!(to_snake_case("createdAt"), "created_at");
/// assert_eq!(to_snake_case("HTTPResponse"), "http_response");
/// assert_eq!(to_snake_case("phone1"), "phone_1");
/// assert_eq!(to_snake_case("dns1Id"), "dns_1_id");
/// assert_eq!(to_snake_case("already_snake"), "already_snake");
/// assert_eq!(to_snake_case("phone_1"), "phone_1"); // idempotent
/// ```
#[must_use]
pub fn to_snake_case(name: &str) -> String {
    let mut result = String::with_capacity(name.len() + 5);
    let mut prev_was_upper = false;
    let mut prev_was_lower = false;
    let mut prev_was_digit = false;

    let chars: Vec<char> = name.chars().collect();
    for (i, &c) in chars.iter().enumerate() {
        if c.is_uppercase() {
            // Insert a boundary underscore before an uppercase char when leaving a
            // lowercase letter, leaving a digit (e.g. "dns1Id"), or leaving an
            // acronym run into a new word (prev upper, next lower) — e.g.
            // "HTTPResponse".
            if i > 0 {
                let next_is_lower = chars.get(i + 1).is_some_and(|n| n.is_lowercase());
                if prev_was_lower || prev_was_digit || (prev_was_upper && next_is_lower) {
                    result.push('_');
                }
            }
            result.push(c.to_ascii_lowercase());
            prev_was_upper = true;
            prev_was_lower = false;
            prev_was_digit = false;
        } else if c.is_ascii_digit() {
            // Insert a boundary underscore before a digit that directly follows a
            // lowercase letter (letter→digit boundary): "phone1" → "phone_1".
            if prev_was_lower {
                result.push('_');
            }
            result.push(c);
            prev_was_upper = false;
            prev_was_lower = false;
            prev_was_digit = true;
        } else {
            result.push(c);
            prev_was_upper = false;
            prev_was_lower = c.is_lowercase();
            prev_was_digit = false;
        }
    }

    result
}

#[cfg(test)]
mod tests;
