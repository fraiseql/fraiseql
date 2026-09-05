//! Tests for the `streaming` module.

#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use axum::http::{HeaderMap, HeaderValue};
use serde_json::json;

use super::{
    helpers::{error_ndjson_line, ndjson_chunk},
    *,
};

// ---------------------------------------------------------------------------
// helpers tests
// ---------------------------------------------------------------------------

#[test]
fn error_ndjson_line_valid_json() {
    let line = error_ndjson_line("something went wrong");
    let s = String::from_utf8(line.to_vec()).unwrap();
    assert!(s.ends_with('\n'));
    let parsed: serde_json::Value = serde_json::from_str(s.trim()).unwrap();
    assert_eq!(parsed["error"], "something went wrong");
}

#[test]
fn error_ndjson_line_escapes_special_chars() {
    let line = error_ndjson_line("bad \"quote\" and \nnewline");
    let s = String::from_utf8(line.to_vec()).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(s.trim()).unwrap();
    assert!(parsed["error"].as_str().unwrap().contains("quote"));
}

#[test]
fn ndjson_format_one_object_per_line() {
    let rows = vec![
        json!({"id": 1, "name": "Alice"}),
        json!({"id": 2, "name": "Bob"}),
    ];

    let mut ndjson = Vec::new();
    for row in &rows {
        let mut line = serde_json::to_vec(row).unwrap();
        line.push(b'\n');
        ndjson.extend_from_slice(&line);
    }

    let output = String::from_utf8(ndjson).unwrap();
    let lines: Vec<&str> = output.trim_end().split('\n').collect();
    assert_eq!(lines.len(), 2);

    // Each line is valid JSON
    for line in &lines {
        let parsed: serde_json::Value = serde_json::from_str(line).unwrap();
        assert!(parsed.is_object());
    }
}

#[test]
fn ndjson_no_envelope() {
    let rows = vec![json!({"id": 1})];

    let mut ndjson = Vec::new();
    for row in &rows {
        let mut line = serde_json::to_vec(row).unwrap();
        line.push(b'\n');
        ndjson.extend_from_slice(&line);
    }

    let output = String::from_utf8(ndjson).unwrap();
    // No "data", "meta", or "links" wrapper
    assert!(!output.contains("\"data\""));
    assert!(!output.contains("\"meta\""));
    assert!(!output.contains("\"links\""));
}

#[test]
fn ndjson_select_fields_applied() {
    // When ?select=id,name is used, each row should only have those fields.
    // This is handled upstream by QueryMatch field selection, but verify format.
    let rows = [json!({"id": 1, "name": "Alice"})];

    let line = serde_json::to_string(&rows[0]).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&line).unwrap();
    assert!(parsed.get("id").is_some());
    assert!(parsed.get("name").is_some());
    assert!(parsed.get("email").is_none());
}

// ---------------------------------------------------------------------------
// mod tests (streaming handler)
// ---------------------------------------------------------------------------

#[test]
fn accepts_ndjson_true_for_exact_match() {
    let mut headers = HeaderMap::new();
    headers.insert("accept", HeaderValue::from_static("application/x-ndjson"));
    assert!(accepts_ndjson(&headers));
}

#[test]
fn accepts_ndjson_true_in_list() {
    let mut headers = HeaderMap::new();
    headers.insert("accept", HeaderValue::from_static("application/json, application/x-ndjson"));
    assert!(accepts_ndjson(&headers));
}

#[test]
fn accepts_ndjson_false_for_json() {
    let mut headers = HeaderMap::new();
    headers.insert("accept", HeaderValue::from_static("application/json"));
    assert!(!accepts_ndjson(&headers));
}

#[test]
fn accepts_ndjson_false_when_missing() {
    let headers = HeaderMap::new();
    assert!(!accepts_ndjson(&headers));
}

#[test]
fn accepts_ndjson_case_insensitive() {
    let mut headers = HeaderMap::new();
    headers.insert("accept", HeaderValue::from_static("Application/X-NDJSON"));
    assert!(accepts_ndjson(&headers));
}

#[test]
fn ndjson_content_type_constant() {
    assert_eq!(NDJSON_CONTENT_TYPE, "application/x-ndjson");
}

// ---------------------------------------------------------------------------
// The export row source (#811, #958)
// ---------------------------------------------------------------------------

// The export total's "absent is not the default" rule (#811) is now asserted where the
// distinction is drawn — `params::tests`, against the resolved plan on the same request —
// rather than against a second reader of the raw query pairs (#1273).

/// One JSON document per line, no envelope.
#[test]
fn ndjson_chunk_writes_one_line_per_row() {
    let (bytes, failed) =
        ndjson_chunk(vec![Ok(json!({"id": 1})), Ok(json!({"id": 2, "name": "Bob"}))]);
    assert!(!failed);

    let text = String::from_utf8(bytes.to_vec()).unwrap();
    let lines: Vec<&str> = text.trim_end().split('\n').collect();
    assert_eq!(lines.len(), 2);
    for line in lines {
        assert!(serde_json::from_str::<serde_json::Value>(line).unwrap().is_object());
    }
    assert!(!text.contains("\"data\""), "NDJSON carries no response envelope");
}

/// A failure part-way through a group keeps the rows that preceded it and says
/// what went wrong. Dropping them, or ending silently, would leave a truncated
/// export that reads as a complete one — #958's `hasNext` problem in another
/// representation.
#[test]
fn ndjson_chunk_emits_preceding_rows_then_the_error() {
    let (bytes, failed) = ndjson_chunk(vec![
        Ok(json!({"id": 1})),
        Err(fraiseql_core::error::FraiseQLError::Database {
            message:   "connection reset".to_string(),
            sql_state: None,
        }),
        Ok(json!({"id": 3})),
    ]);
    assert!(failed, "a failed group must terminate the export");

    let text = String::from_utf8(bytes.to_vec()).unwrap();
    let lines: Vec<&str> = text.trim_end().split('\n').collect();
    assert_eq!(lines.len(), 2, "the row before the failure, then the error line");
    assert_eq!(serde_json::from_str::<serde_json::Value>(lines[0]).unwrap()["id"], 1);
    let err: serde_json::Value = serde_json::from_str(lines[1]).unwrap();
    assert!(err["error"].as_str().unwrap().contains("connection reset"));
}

// ---------------------------------------------------------------------------
// Formula-injection (CSV injection) guard — OWASP mitigation. Added during
// the v2.4.0 security audit of PR #328 (#269 REST exports); moved here from
// `streaming::csv::tests` with the guard itself (#920) so an XLSX-only build
// compiles and runs them too.
// ---------------------------------------------------------------------------

#[cfg(any(feature = "export-csv", feature = "export-xlsx"))]
mod formula_injection {
    use super::super::guard_formula_injection;

    #[test]
    fn guard_prefixes_equal_with_single_quote() {
        assert_eq!(
            guard_formula_injection("=HYPERLINK(\"http://evil\")"),
            "'=HYPERLINK(\"http://evil\")"
        );
    }

    #[test]
    fn guard_prefixes_plus() {
        assert_eq!(guard_formula_injection("+SUM(1+1)"), "'+SUM(1+1)");
    }

    #[test]
    fn guard_prefixes_minus() {
        assert_eq!(guard_formula_injection("-2+3"), "'-2+3");
    }

    #[test]
    fn guard_prefixes_at_sign() {
        // `@` triggers Excel's macro evaluation (e.g. legacy @SUM, @WEBSERVICE).
        assert_eq!(guard_formula_injection("@SUM(A1:A10)"), "'@SUM(A1:A10)");
    }

    #[test]
    fn guard_prefixes_leading_tab() {
        // Tab + `=` is a documented OWASP variant — Excel treats it as
        // whitespace-prefixed formula start.
        assert_eq!(guard_formula_injection("\t=cmd|'/C calc'!A0"), "'\t=cmd|'/C calc'!A0");
    }

    #[test]
    fn guard_prefixes_leading_cr() {
        assert_eq!(guard_formula_injection("\rmalicious"), "'\rmalicious");
    }

    #[test]
    fn guard_passes_through_safe_strings() {
        // Common non-dangerous prefixes — alphanumerics, quotes, brackets,
        // currency symbols — must not be touched.
        for safe in ["Alice", "1234", "(NULL)", "$100.00", "false", "\"quoted\""] {
            assert_eq!(
                guard_formula_injection(safe),
                safe,
                "safe input {safe:?} must pass through unchanged"
            );
        }
    }

    #[test]
    fn guard_passes_through_empty_string() {
        assert_eq!(guard_formula_injection(""), "");
    }
}

// ---------------------------------------------------------------------------
// Header-column selection (#1274)
// ---------------------------------------------------------------------------

/// `export_columns` and `determine_columns` live in `helpers` because the CSV and XLSX
/// writers held byte-identical copies of the column rule and of the `?select=` parser it
/// used to consult (#1274). Their tests are here for the same reason: two copies of a
/// test are two places for one of them to be forgotten.
#[cfg(any(feature = "export-csv", feature = "export-xlsx"))]
mod export_columns {
    use fraiseql_core::{runtime::QueryMatch, schema::QueryDefinition};
    use serde_json::json;

    use super::super::helpers::{determine_columns, export_columns};

    /// A `QueryMatch` whose projection is `fields` — the shape `resolve_get_query` hands
    /// the export writers.
    fn match_projecting(fields: &[&str]) -> QueryMatch {
        QueryMatch::from_operation(
            QueryDefinition::new("posts", "Post"),
            fields.iter().map(|f| (*f).to_string()).collect(),
            std::collections::HashMap::new(),
            None,
        )
        .expect("a QueryMatch over a field list is infallible")
    }

    #[test]
    fn the_header_is_the_projection_in_projection_order() {
        assert_eq!(
            export_columns(&match_projecting(&["email", "id", "name"])),
            Some(vec!["email".to_string(), "id".to_string(), "name".to_string()]),
            "not sorted, not re-derived: the columns the rows were projected by, in order"
        );
    }

    /// The only case with no answer. `resolve_get_query` produces an empty projection
    /// only when the return type is absent from the schema — `RestFieldSpec::All`
    /// expands to the declared fields (#886), so an ordinary request always has one.
    #[test]
    fn an_empty_projection_has_no_header_to_offer() {
        assert!(export_columns(&match_projecting(&[])).is_none());
    }

    #[test]
    fn determine_columns_prefers_the_projection() {
        let rows = vec![json!({"id": 1, "name": "Alice", "email": "a@b"})];
        let select = vec!["email".to_string(), "id".to_string()];
        assert_eq!(determine_columns(Some(&select), &rows), vec!["email", "id"]);
    }

    #[test]
    fn determine_columns_falls_back_to_sorted_first_row_keys() {
        let rows = vec![json!({"id": 1, "name": "Alice"})];
        assert_eq!(determine_columns(None, &rows), vec!["id", "name"]);
    }

    /// The fallback sorts explicitly, so the header is alphabetical whatever order the
    /// keys were inserted in — `serde_json::Map` iterates in insertion order once any
    /// dependency turns on `preserve_order` (as `--all-features` does).
    #[test]
    fn determine_columns_fallback_is_sorted_regardless_of_key_insertion_order() {
        let rows = vec![json!({"name": "Alice", "email": "a@b", "id": 1})];
        assert_eq!(determine_columns(None, &rows), vec!["email", "id", "name"]);
    }

    #[test]
    fn determine_columns_is_empty_with_no_rows_and_no_projection() {
        assert!(determine_columns(None, &[]).is_empty());
    }
}
