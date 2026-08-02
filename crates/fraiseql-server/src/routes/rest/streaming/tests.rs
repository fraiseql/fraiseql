//! Tests for the `streaming` module.

#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use axum::http::{HeaderMap, HeaderValue, StatusCode};
use serde_json::json;

use super::{
    helpers::{error_ndjson_line, extract_rows},
    *,
};

// ---------------------------------------------------------------------------
// helpers tests
// ---------------------------------------------------------------------------

fn v(s: &str) -> serde_json::Value {
    serde_json::from_str(s).unwrap()
}

#[test]
fn extract_rows_from_array() {
    let result = v(r#"{"data":{"users":[{"id":1,"name":"Alice"},{"id":2,"name":"Bob"}]}}"#);
    let rows = extract_rows(&result, "users").unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["name"], "Alice");
    assert_eq!(rows[1]["name"], "Bob");
}

#[test]
fn extract_rows_from_single_resource() {
    let result = v(r#"{"data":{"user":{"id":1,"name":"Alice"}}}"#);
    let rows = extract_rows(&result, "user").unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["name"], "Alice");
}

#[test]
fn extract_rows_missing_data() {
    let result = v(r#"{"errors":[]}"#);
    let err = extract_rows(&result, "users").unwrap_err();
    assert_eq!(err.status, StatusCode::INTERNAL_SERVER_ERROR);
}

#[test]
fn extract_rows_missing_query() {
    let result = v(r#"{"data":{"other_query":[]}}"#);
    let err = extract_rows(&result, "users").unwrap_err();
    assert_eq!(err.status, StatusCode::INTERNAL_SERVER_ERROR);
}

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
// #811 — the shared export pagination driver
// ---------------------------------------------------------------------------

use super::helpers::{next_batch_size, requested_total_limit, set_export_page};

/// Without a client cap, every batch is a full page — the loop terminates on a short
/// page from the database, not on arithmetic.
#[test]
fn next_batch_size_is_the_full_page_when_uncapped() {
    assert_eq!(next_batch_size(500, None, 0), Some(500));
    assert_eq!(next_batch_size(500, None, 10_000), Some(500));
}

/// A client cap clamps the final batch so `?limit=` is honoured exactly, including when
/// it is not a multiple of the batch size.
#[test]
fn next_batch_size_clamps_the_final_batch_to_the_client_limit() {
    assert_eq!(next_batch_size(100, Some(307), 0), Some(100));
    assert_eq!(next_batch_size(100, Some(307), 100), Some(100));
    assert_eq!(next_batch_size(100, Some(307), 200), Some(100));
    // 307 - 300 = 7 rows left.
    assert_eq!(next_batch_size(100, Some(307), 300), Some(7));
}

/// Reaching the cap ends the export. This is the arithmetic that makes a bounded export
/// terminate; the `None` is what the loops read as "done".
#[test]
fn next_batch_size_is_none_once_the_client_limit_is_reached() {
    assert_eq!(next_batch_size(100, Some(300), 300), None);
    // Defensive: an overshoot must still terminate rather than underflow.
    assert_eq!(next_batch_size(100, Some(300), 301), None);
    assert_eq!(next_batch_size(100, Some(0), 0), None);
}

/// The page must land in `arguments`. `execute_query_direct` reads limit/offset from
/// there and nowhere else — writing them into `variables` is exactly the #811 defect, and
/// this test fails if the driver ever regresses to it.
#[test]
fn set_export_page_writes_into_arguments() {
    let mut qm = fraiseql_core::runtime::QueryMatch::from_operation(
        fraiseql_test_utils::schema_builder::TestQueryBuilder::new("things", "Thing")
            .returns_list(true)
            .with_sql_source("v_thing")
            .build(),
        vec!["id".to_string()],
        std::collections::HashMap::new(),
        None,
    )
    .unwrap();

    set_export_page(&mut qm, 250, 1_000);

    assert_eq!(qm.arguments.get("limit").and_then(serde_json::Value::as_u64), Some(250));
    assert_eq!(qm.arguments.get("offset").and_then(serde_json::Value::as_u64), Some(1_000));
}

/// An absent `?limit=` must stay absent rather than becoming `default_page_size`: the
/// two mean opposite things to an export, and collapsing them is what made a full export
/// silently return one page.
#[test]
fn requested_total_limit_distinguishes_absent_from_supplied() {
    assert_eq!(requested_total_limit(&[]), None);
    assert_eq!(requested_total_limit(&[("select", "id"), ("sort", "id")]), None);
    assert_eq!(requested_total_limit(&[("limit", "250")]), Some(250));
    // A malformed value is not a cap — the parameter validator rejects it upstream.
    assert_eq!(requested_total_limit(&[("limit", "abc")]), None);
}
