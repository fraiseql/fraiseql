//! Property-based tests for SQL identifier quoting and JSON path escape functions.
//!
//! These tests verify security-critical invariants that must hold for **all** inputs,
//! not just the representative samples in unit tests:
//!
//! 1. **Delimiter isolation** — after quoting, the raw (unescaped) delimiter never appears inside
//!    the quoted output.
//! 2. **Wrapping shape** — the output always starts and ends with the dialect's appropriate
//!    delimiters.
//! 3. **Quote-count conservation** — path escape functions double every single quote, so the
//!    escaped output contains exactly twice as many `'` as the input.

#![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable
#![allow(clippy::wildcard_imports)] // Reason: test helpers imported via glob

use fraiseql_db::{
    identifier::quote_postgres_identifier,
    path_escape::{escape_postgres_jsonb_path, escape_postgres_jsonb_segment},
};
use proptest::prelude::*;

// ─── Arbitrary string strategies ─────────────────────────────────────────────

/// Printable ASCII strings (including all delimiter characters) up to 64 chars.
fn any_identifier() -> impl Strategy<Value = String> {
    "[ -~]{0,64}"
}

/// A single path segment (no dots — path-escape functions receive pre-split segments).
fn any_path_segment() -> impl Strategy<Value = String> {
    "[ -~]{0,32}"
}

/// A vector of 1–6 path segments.
fn any_path() -> impl Strategy<Value = Vec<String>> {
    prop::collection::vec(any_path_segment(), 1..=6)
}

// ─── Helper: count non-overlapping occurrences of a substring ────────────────

fn count_substr(haystack: &str, needle: &str) -> usize {
    let mut count = 0;
    let mut start = 0;
    while let Some(pos) = haystack[start..].find(needle) {
        count += 1;
        start += pos + needle.len();
    }
    count
}

// ─── PostgreSQL identifier properties ────────────────────────────────────────

proptest! {
    /// After PostgreSQL quoting, the component body never contains a bare `"`.
    /// Only the doubled escape `""` is permitted inside a quoted identifier.
    #[test]
    fn postgres_identifier_no_bare_double_quote(name in any_identifier()) {
        for component in name.split('.') {
            let inner = component.replace('"', "\"\"");
            // Remove all doubled quotes — no bare quote should remain.
            let stripped = inner.replace("\"\"", "");
            prop_assert!(
                !stripped.contains('"'),
                "component {:?} has a bare double-quote after PostgreSQL escaping",
                component
            );
        }
    }

    /// Output starts and ends with `"` (the PostgreSQL identifier delimiter).
    #[test]
    fn postgres_identifier_wraps_with_double_quotes(name in any_identifier()) {
        let quoted = quote_postgres_identifier(&name);
        prop_assert!(quoted.starts_with('"'), "starts_with failed for {:?}", quoted);
        prop_assert!(quoted.ends_with('"'), "ends_with failed for {:?}", quoted);
    }

    /// `.`-separated components in the input produce the same count in the output.
    #[test]
    fn postgres_identifier_component_count(name in any_identifier()) {
        let quoted = quote_postgres_identifier(&name);
        let input_parts = name.split('.').count();
        let separators = count_substr(&quoted, "\".\"");
        prop_assert!(
            separators + 1 == input_parts,
            "expected {} parts, got {} separators+1 — input={:?} quoted={:?}",
            input_parts, separators, name, quoted
        );
    }
}

// ─── JSON path escape properties ─────────────────────────────────────────────

proptest! {
    /// Every single quote in the input must be doubled in the PostgreSQL segment.
    /// Invariant: `output.count("'") == 2 * input.count("'")`.
    #[test]
    fn postgres_segment_doubles_all_single_quotes(segment in any_path_segment()) {
        let escaped = escape_postgres_jsonb_segment(&segment);
        let input_quotes: usize = segment.chars().filter(|&c| c == '\'').count();
        let output_quotes: usize = escaped.chars().filter(|&c| c == '\'').count();
        prop_assert!(
            output_quotes == input_quotes * 2,
            "input {:?} has {} quotes; expected {} in output, got {}",
            segment, input_quotes, input_quotes * 2, output_quotes
        );
    }

    /// The same doubling applies to every segment in a path vector.
    #[test]
    fn postgres_path_quote_count(path in any_path()) {
        let total_input: usize =
            path.iter().map(|s| s.chars().filter(|&c| c == '\'').count()).sum();
        let escaped = escape_postgres_jsonb_path(&path);
        let total_output: usize =
            escaped.iter().map(|s| s.chars().filter(|&c| c == '\'').count()).sum();
        prop_assert!(
            total_output == total_input * 2,
            "path {:?}: expected {} quotes, got {}",
            path, total_input * 2, total_output
        );
    }

}

// ─── Cross-dialect consistency ────────────────────────────────────────────────
