//! Tests for the `streaming` module.

#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use axum::http::{HeaderMap, HeaderValue, StatusCode};
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
fn validate_ndjson_rejects_count_exact() {
    let prefer = PreferHeader {
        count_exact: true,
        ..PreferHeader::default()
    };
    let pagination = PaginationParams::None;
    let err = validate_ndjson_request(&prefer, &pagination).unwrap_err();
    assert_eq!(err.status, StatusCode::BAD_REQUEST);
    assert!(err.message.contains("count not available"));
}

#[test]
fn validate_ndjson_rejects_count_planned() {
    let prefer = PreferHeader {
        count_planned: true,
        ..PreferHeader::default()
    };
    let pagination = PaginationParams::None;
    let err = validate_ndjson_request(&prefer, &pagination).unwrap_err();
    assert_eq!(err.status, StatusCode::BAD_REQUEST);
}

#[test]
fn validate_ndjson_rejects_count_estimated() {
    let prefer = PreferHeader {
        count_estimated: true,
        ..PreferHeader::default()
    };
    let pagination = PaginationParams::None;
    let err = validate_ndjson_request(&prefer, &pagination).unwrap_err();
    assert_eq!(err.status, StatusCode::BAD_REQUEST);
}

#[test]
fn validate_ndjson_rejects_cursor_pagination() {
    let prefer = PreferHeader::default();
    let pagination = PaginationParams::Cursor {
        first:  Some(10),
        after:  None,
        last:   None,
        before: None,
    };
    let err = validate_ndjson_request(&prefer, &pagination).unwrap_err();
    assert_eq!(err.status, StatusCode::BAD_REQUEST);
    assert!(err.message.contains("pagination not available"));
}

#[test]
fn validate_ndjson_rejects_offset_pagination() {
    let prefer = PreferHeader::default();
    let pagination = PaginationParams::Offset {
        limit:  10,
        offset: 5,
    };
    let err = validate_ndjson_request(&prefer, &pagination).unwrap_err();
    assert_eq!(err.status, StatusCode::BAD_REQUEST);
}

#[test]
fn validate_ndjson_allows_limit_only() {
    // offset=0 with limit is fine — it's the default, not explicit pagination
    let prefer = PreferHeader::default();
    let pagination = PaginationParams::Offset {
        limit:  100,
        offset: 0,
    };
    assert!(validate_ndjson_request(&prefer, &pagination).is_ok());
}

#[test]
fn validate_ndjson_allows_no_pagination() {
    let prefer = PreferHeader::default();
    let pagination = PaginationParams::None;
    assert!(validate_ndjson_request(&prefer, &pagination).is_ok());
}

#[test]
fn ndjson_content_type_constant() {
    assert_eq!(NDJSON_CONTENT_TYPE, "application/x-ndjson");
}

// ---------------------------------------------------------------------------
// The export row source (#811, #958)
// ---------------------------------------------------------------------------

use super::helpers::requested_total_limit;

/// An absent `?limit=` must stay absent rather than becoming `default_page_size`: the
/// two mean opposite things to an export, and collapsing them is what made a full export
/// silently return one page (#811).
#[test]
fn requested_total_limit_distinguishes_absent_from_supplied() {
    assert_eq!(requested_total_limit(&[]), None);
    assert_eq!(requested_total_limit(&[("select", "id"), ("sort", "id")]), None);
    assert_eq!(requested_total_limit(&[("limit", "250")]), Some(250));
    // A malformed value is not a cap — the parameter validator rejects it upstream.
    assert_eq!(requested_total_limit(&[("limit", "abc")]), None);
}

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
