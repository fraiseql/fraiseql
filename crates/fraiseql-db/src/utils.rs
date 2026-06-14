//! Shared utility functions for the `fraiseql-db` crate.

use std::collections::HashSet;
use std::sync::{LazyLock, OnceLock};

/// Built-in acronyms whose internal digit boundary is **not** split by
/// [`to_snake_case`], so they round-trip atomically (`s3` ↔ `s3`, not `s_3`).
/// Lowercased; only `<word><digit>` shapes are relevant. Extend per project via
/// the `[fraiseql.naming] acronyms` config — see [`set_runtime_acronyms`].
static DEFAULT_ACRONYMS: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    [
        "s3", "ec2", "ipv4", "ipv6", "oauth1", "oauth2", "sha1", "sha256", "sha512", "md5",
        "base64", "utf8", "p256", "p384",
    ]
    .into_iter()
    .collect()
});

/// The effective acronym set (defaults ∪ project config), installed once at
/// startup by [`set_runtime_acronyms`]. When unset (tests, library use),
/// [`to_snake_case`] falls back to the built-in defaults.
static RUNTIME_ACRONYMS: OnceLock<HashSet<String>> = OnceLock::new();

/// Install the project's acronym additions (from `[fraiseql.naming] acronyms`) on
/// top of the built-in defaults.
///
/// Idempotent — only the first call wins, so the server (at boot) and the CLI (at
/// compile) each call it once. Terms are trimmed and lowercased; empties ignored.
pub fn set_runtime_acronyms(extra: &[String]) {
    let mut set: HashSet<String> = DEFAULT_ACRONYMS.iter().map(|s| (*s).to_string()).collect();
    for term in extra {
        let term = term.trim().to_ascii_lowercase();
        if !term.is_empty() {
            set.insert(term);
        }
    }
    let _ = RUNTIME_ACRONYMS.set(set);
}

/// Whether `candidate` (a lowercase `<word><digit>` token) is a registered acronym
/// in the effective set (runtime config if installed, else the built-in defaults).
fn is_registered_acronym(candidate: &str) -> bool {
    match RUNTIME_ACRONYMS.get() {
        Some(set) => set.contains(candidate),
        None => DEFAULT_ACRONYMS.contains(candidate),
    }
}

/// Does the lowercase word at `word_start` plus the digit run at `digit_start`
/// form a registered acronym (`s3`, `ipv4`, `oauth2`)? Used to suppress the
/// letter→digit split so the acronym stays whole.
fn acronym_spans_digit(chars: &[char], word_start: usize, digit_start: usize) -> bool {
    let mut end = digit_start;
    while end < chars.len() && chars[end].is_ascii_digit() {
        end += 1;
    }
    let candidate: String = chars[word_start..end].iter().collect::<String>().to_ascii_lowercase();
    is_registered_acronym(&candidate)
}

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
/// # Digit boundaries and acronyms
///
/// A digit segment is normally a word of its own, inverting `to_camel_case`'s
/// collapse (`phone_1` → `phone1`): this function reinserts the boundary so the
/// round trip is bijective — `"phone1"` → `"phone_1"`, `"dns1Id"` → `"dns_1_id"`.
///
/// Registered acronyms are the exception: a lowercase word plus a digit run that
/// matches the acronym registry stays whole — `"s3"` → `"s3"`, `"ipv4"` → `"ipv4"`,
/// `"s3Bucket"` → `"s3_bucket"`. The built-in defaults (`s3`, `ipv4`, `oauth2`, …)
/// are extended per project via `[fraiseql.naming] acronyms` (see
/// [`set_runtime_acronyms`]); an unregistered `oauth2`-shaped name still splits.
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
/// assert_eq!(to_snake_case("s3"), "s3"); // built-in acronym, stays whole
/// assert_eq!(to_snake_case("s3Bucket"), "s3_bucket");
/// assert_eq!(to_snake_case("already_snake"), "already_snake");
/// assert_eq!(to_snake_case("phone_1"), "phone_1"); // idempotent
/// ```
#[must_use]
pub fn to_snake_case(name: &str) -> String {
    let chars: Vec<char> = name.chars().collect();
    let mut result = String::with_capacity(name.len() + 5);
    let mut prev_was_upper = false;
    let mut prev_was_lower = false;
    let mut prev_was_digit = false;
    // Start index (in `chars`) of the current run of consecutive lowercase letters,
    // used to test a lowercase-word + digit run against the acronym registry.
    let mut lower_run_start = 0usize;

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
            // A digit after a lowercase letter normally opens a new word
            // ("phone1" → "phone_1"). Suppress that split when the lowercase word
            // plus this digit run is a registered acronym ("s3", "ipv4", "oauth2").
            if prev_was_lower && !acronym_spans_digit(&chars, lower_run_start, i) {
                result.push('_');
            }
            result.push(c);
            prev_was_upper = false;
            prev_was_lower = false;
            prev_was_digit = true;
        } else {
            if c.is_lowercase() {
                if !prev_was_lower {
                    lower_run_start = i;
                }
                prev_was_lower = true;
            } else {
                prev_was_lower = false;
            }
            result.push(c);
            prev_was_upper = false;
            prev_was_digit = false;
        }
    }

    result
}

#[cfg(test)]
mod tests;
