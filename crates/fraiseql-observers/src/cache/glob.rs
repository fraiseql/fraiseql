//! Pure Redis glob-pattern helpers for the cache-invalidation action (#428).
//!
//! The cache-invalidation transport (the `caching`-gated `RedisCacheInvalidator`)
//! turns an observer's `key_pattern` template into either a single literal key
//! (direct `UNLINK`) or a Redis `SCAN MATCH` glob. The security-critical rule is
//! **escape-then-substitute**:
//!
//! - The `key_pattern` *template* is authored by a trusted config author, so a `*` it contains is
//!   an intentional glob.
//! - The `{{ field }}` values come from the (untrusted) event payload, so a `*` in a value must NOT
//!   widen the match — it is glob-escaped *before* it is substituted into the template.
//!
//! Escaping after substitution would destroy the author's intended globs;
//! not escaping at all would let an event field inject a wildcard and wipe an
//! unrelated keyspace. Escape-then-substitute is the only order that is both
//! correct and safe, so it lives here as small, pure, unit-tested functions.
//!
//! These helpers are always compiled (not behind `caching`) so the standard
//! test leg exercises the escaping logic on every push; the Redis transport that
//! consumes them is `caching`-gated.
#![cfg_attr(not(feature = "caching"), allow(dead_code))] // Reason: consumed only by the `caching`-gated RedisCacheInvalidator; kept always-compiled so the security-critical glob escaping is covered by the standard (non-feature) test leg.

use serde_json::Value;

/// Backslash-escape every Redis glob metacharacter in `input`.
///
/// Redis `KEYS`/`SCAN MATCH` treat `*`, `?`, and `[`…`]` as glob operators and
/// `\` as the escape character. Escaping the full metaclass (`\ * ? [ ]`) means a
/// substituted event value is matched (or deleted) literally — `[` in particular
/// is the one that is easy to forget and would otherwise open a character class.
pub fn escape_redis_glob(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        if matches!(ch, '\\' | '*' | '?' | '[' | ']') {
            out.push('\\');
        }
        out.push(ch);
    }
    out
}

/// Returns `true` if `pattern` still contains an *unescaped* glob operator
/// (`*`, `?`, or `[`), walking left-to-right and honoring `\` escapes.
///
/// Used to decide the dispatch path after [`render_key_pattern`] has already
/// escaped the substituted values: any glob operator that survives must have
/// come from the trusted template literal, so the pattern is a real glob and
/// takes the `SCAN MATCH` path. No surviving operator means the pattern targets
/// a single concrete key and takes the cheaper direct-`UNLINK` path (no keyspace
/// walk). A lone `]` is not an operator on its own and never triggers a glob.
pub fn has_unescaped_glob(pattern: &str) -> bool {
    let mut chars = pattern.chars();
    while let Some(ch) = chars.next() {
        match ch {
            // A backslash escapes the next char (if any); neither is an operator.
            '\\' => {
                chars.next();
            },
            '*' | '?' | '[' => return true,
            _ => {},
        }
    }
    false
}

/// Render a `{{ key }}` template against a JSON object's top-level fields.
///
/// Mirrors the Slack/email text renderer, with one addition: when `escape` is
/// `true` each substituted *value* is passed through [`escape_redis_glob`] so a
/// wildcard in event data cannot alter the resulting Redis glob (escape-then-
/// substitute). Template literals are never escaped, preserving any glob the
/// config author wrote intentionally.
///
/// Call with `escape = true` to build the `SCAN MATCH` glob, and with
/// `escape = false` to build the literal key for a direct `UNLINK`.
pub fn render_key_pattern(template: &str, data: &Value, escape: bool) -> String {
    let mut rendered = template.to_string();
    if let Value::Object(map) = data {
        for (key, value) in map {
            let placeholder = format!("{{{{ {key} }}}}");
            let value_str = match value {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            let replacement = if escape {
                escape_redis_glob(&value_str)
            } else {
                value_str
            };
            rendered = rendered.replace(&placeholder, &replacement);
        }
    }
    rendered
}
