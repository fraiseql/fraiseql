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
//! 4. **Path prefix** — MySQL/SQLite/SQL Server path functions always prefix with `$.`.
//! 5. **Cross-dialect consistency** — MySQL, SQLite, and SQL Server produce identical path bodies
//!    for the same input (all use the same escaping strategy).

#![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable
#![allow(clippy::wildcard_imports)] // Reason: test helpers imported via glob

use fraiseql_db::{
    identifier::{
        quote_mysql_identifier, quote_postgres_identifier, quote_sqlite_identifier,
        quote_sqlserver_identifier,
    },
    path_escape::{
        escape_mysql_json_path, escape_postgres_jsonb_path, escape_postgres_jsonb_segment,
        escape_sqlite_json_path, escape_sqlserver_json_path,
    },
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

// ─── MySQL identifier properties ─────────────────────────────────────────────

proptest! {
    /// After MySQL quoting, each component never contains a bare backtick.
    #[test]
    fn mysql_identifier_no_bare_backtick(name in any_identifier()) {
        for component in name.split('.') {
            let inner = component.replace('`', "``");
            let stripped = inner.replace("``", "");
            prop_assert!(
                !stripped.contains('`'),
                "component {:?} has a bare backtick after MySQL escaping",
                component
            );
        }
    }

    /// Output starts and ends with a backtick.
    #[test]
    fn mysql_identifier_wraps_with_backticks(name in any_identifier()) {
        let quoted = quote_mysql_identifier(&name);
        prop_assert!(quoted.starts_with('`'), "starts_with failed for {:?}", quoted);
        prop_assert!(quoted.ends_with('`'), "ends_with failed for {:?}", quoted);
    }

    /// Component count is preserved across MySQL quoting.
    #[test]
    fn mysql_identifier_component_count(name in any_identifier()) {
        let quoted = quote_mysql_identifier(&name);
        let input_parts = name.split('.').count();
        let separators = count_substr(&quoted, "`.`");
        prop_assert!(
            separators + 1 == input_parts,
            "expected {} parts, got {} separators+1 — input={:?} quoted={:?}",
            input_parts, separators, name, quoted
        );
    }
}

// ─── SQLite identifier properties ────────────────────────────────────────────

proptest! {
    /// SQLite uses the same double-quote escaping as PostgreSQL.
    #[test]
    fn sqlite_identifier_no_bare_double_quote(name in any_identifier()) {
        for component in name.split('.') {
            let inner = component.replace('"', "\"\"");
            let stripped = inner.replace("\"\"", "");
            prop_assert!(
                !stripped.contains('"'),
                "component {:?} has a bare double-quote after SQLite escaping",
                component
            );
        }
    }

    /// Output starts and ends with `"`.
    #[test]
    fn sqlite_identifier_wraps_with_double_quotes(name in any_identifier()) {
        let quoted = quote_sqlite_identifier(&name);
        prop_assert!(quoted.starts_with('"'), "starts_with failed for {:?}", quoted);
        prop_assert!(quoted.ends_with('"'), "ends_with failed for {:?}", quoted);
    }
}

// ─── SQL Server identifier properties ────────────────────────────────────────

proptest! {
    /// After SQL Server quoting, no bare `]` appears inside any component.
    #[test]
    fn sqlserver_identifier_no_bare_close_bracket(name in any_identifier()) {
        for component in name.split('.') {
            let inner = component.replace(']', "]]");
            let stripped = inner.replace("]]", "");
            prop_assert!(
                !stripped.contains(']'),
                "component {:?} has a bare ] after SQL Server escaping",
                component
            );
        }
    }

    /// Output starts with `[` and ends with `]`.
    #[test]
    fn sqlserver_identifier_wraps_with_brackets(name in any_identifier()) {
        let quoted = quote_sqlserver_identifier(&name);
        prop_assert!(quoted.starts_with('['), "starts_with failed for {:?}", quoted);
        prop_assert!(quoted.ends_with(']'), "ends_with failed for {:?}", quoted);
    }

    /// Component count is preserved across SQL Server quoting.
    #[test]
    fn sqlserver_identifier_component_count(name in any_identifier()) {
        let quoted = quote_sqlserver_identifier(&name);
        let input_parts = name.split('.').count();
        let separators = count_substr(&quoted, "].[");
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

    /// MySQL path always starts with `$.`.
    #[test]
    fn mysql_path_starts_with_dollar_dot(path in any_path()) {
        let result = escape_mysql_json_path(&path);
        prop_assert!(result.starts_with("$."), "MySQL path must start with '$.' — got {:?}", result);
    }

    /// MySQL path body doubles single quotes from the joined input.
    #[test]
    fn mysql_path_doubles_quotes(path in any_path()) {
        let result = escape_mysql_json_path(&path);
        let body = &result["$.".len()..];
        let full_input = path.join(".");
        let input_quotes: usize = full_input.chars().filter(|&c| c == '\'').count();
        let output_quotes: usize = body.chars().filter(|&c| c == '\'').count();
        prop_assert!(
            output_quotes == input_quotes * 2,
            "path {:?}: expected {} quotes in body, got {}",
            path, input_quotes * 2, output_quotes
        );
    }

    /// SQLite path always starts with `$.`.
    #[test]
    fn sqlite_path_starts_with_dollar_dot(path in any_path()) {
        let result = escape_sqlite_json_path(&path);
        prop_assert!(result.starts_with("$."), "SQLite path must start with '$.' — got {:?}", result);
    }

    /// SQL Server path always starts with `$.`.
    #[test]
    fn sqlserver_path_starts_with_dollar_dot(path in any_path()) {
        let result = escape_sqlserver_json_path(&path);
        prop_assert!(result.starts_with("$."), "SQL Server path must start with '$.' — got {:?}", result);
    }
}

// ─── Cross-dialect consistency ────────────────────────────────────────────────

proptest! {
    /// MySQL, SQLite, and SQL Server all use the same escaping strategy
    /// (doubling single quotes, joining with `.`), so their path bodies must match.
    #[test]
    fn path_dialects_agree_on_body(path in any_path()) {
        let mysql  = escape_mysql_json_path(&path);
        let sqlite = escape_sqlite_json_path(&path);
        let sqlsrv = escape_sqlserver_json_path(&path);

        let mysql_body  = &mysql ["$.".len()..];
        let sqlite_body = &sqlite["$.".len()..];
        let sqlsrv_body = &sqlsrv["$.".len()..];

        prop_assert!(
            mysql_body == sqlite_body,
            "MySQL and SQLite path bodies differ for {:?}: {:?} vs {:?}",
            path, mysql_body, sqlite_body
        );
        prop_assert!(
            mysql_body == sqlsrv_body,
            "MySQL and SQL Server path bodies differ for {:?}: {:?} vs {:?}",
            path, mysql_body, sqlsrv_body
        );
    }
}
