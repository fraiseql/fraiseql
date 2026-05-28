//! Tests for the `streaming::csv` module.

#![allow(clippy::unwrap_used)] // Reason: tests follow the NDJSON sibling module's convention.

use axum::http::StatusCode;

use super::*;

#[test]
fn accepts_csv_true_for_exact_match() {
    let mut headers = HeaderMap::new();
    headers.insert("accept", HeaderValue::from_static("text/csv"));
    assert!(accepts_csv(&headers));
}

#[test]
fn accepts_csv_true_in_list() {
    let mut headers = HeaderMap::new();
    headers.insert("accept", HeaderValue::from_static("application/json, text/csv"));
    assert!(accepts_csv(&headers));
}

#[test]
fn accepts_csv_false_for_json() {
    let mut headers = HeaderMap::new();
    headers.insert("accept", HeaderValue::from_static("application/json"));
    assert!(!accepts_csv(&headers));
}

#[test]
fn accepts_csv_false_when_missing() {
    let headers = HeaderMap::new();
    assert!(!accepts_csv(&headers));
}

#[test]
fn accepts_csv_case_insensitive() {
    let mut headers = HeaderMap::new();
    headers.insert("accept", HeaderValue::from_static("Text/CSV"));
    assert!(accepts_csv(&headers));
}

#[test]
fn accepts_csv_ignores_quality_params() {
    let mut headers = HeaderMap::new();
    headers.insert("accept", HeaderValue::from_static("text/csv;q=0.9, application/json;q=0.8"));
    assert!(accepts_csv(&headers));
}

#[test]
fn accepts_csv_does_not_match_text_plain() {
    let mut headers = HeaderMap::new();
    headers.insert("accept", HeaderValue::from_static("text/plain"));
    assert!(!accepts_csv(&headers));
}

#[test]
fn validate_csv_rejects_count_exact() {
    let prefer = PreferHeader {
        count_exact: true,
        ..PreferHeader::default()
    };
    let pagination = PaginationParams::None;
    let err = validate_csv_request(&prefer, &pagination).unwrap_err();
    assert_eq!(err.status, StatusCode::BAD_REQUEST);
    assert!(err.message.contains("count not available"));
}

#[test]
fn validate_csv_rejects_count_planned() {
    let prefer = PreferHeader {
        count_planned: true,
        ..PreferHeader::default()
    };
    let pagination = PaginationParams::None;
    let err = validate_csv_request(&prefer, &pagination).unwrap_err();
    assert_eq!(err.status, StatusCode::BAD_REQUEST);
}

#[test]
fn validate_csv_rejects_count_estimated() {
    let prefer = PreferHeader {
        count_estimated: true,
        ..PreferHeader::default()
    };
    let pagination = PaginationParams::None;
    let err = validate_csv_request(&prefer, &pagination).unwrap_err();
    assert_eq!(err.status, StatusCode::BAD_REQUEST);
}

#[test]
fn validate_csv_rejects_cursor_pagination() {
    let prefer = PreferHeader::default();
    let pagination = PaginationParams::Cursor {
        first:  Some(10),
        after:  None,
        last:   None,
        before: None,
    };
    let err = validate_csv_request(&prefer, &pagination).unwrap_err();
    assert_eq!(err.status, StatusCode::BAD_REQUEST);
    assert!(err.message.contains("pagination not available"));
}

#[test]
fn validate_csv_rejects_offset_pagination() {
    let prefer = PreferHeader::default();
    let pagination = PaginationParams::Offset {
        limit:  10,
        offset: 5,
    };
    let err = validate_csv_request(&prefer, &pagination).unwrap_err();
    assert_eq!(err.status, StatusCode::BAD_REQUEST);
}

#[test]
fn validate_csv_allows_limit_only() {
    let prefer = PreferHeader::default();
    let pagination = PaginationParams::Offset {
        limit:  100,
        offset: 0,
    };
    assert!(validate_csv_request(&prefer, &pagination).is_ok());
}

#[test]
fn validate_csv_allows_no_pagination() {
    let prefer = PreferHeader::default();
    let pagination = PaginationParams::None;
    assert!(validate_csv_request(&prefer, &pagination).is_ok());
}

#[test]
fn csv_content_type_constant() {
    assert_eq!(CSV_CONTENT_TYPE, "text/csv");
}

#[test]
fn sanitize_filename_keeps_safe_chars() {
    assert_eq!(sanitize_filename("users"), "users");
    assert_eq!(sanitize_filename("user_profile"), "user_profile");
    assert_eq!(sanitize_filename("user-list"), "user-list");
    assert_eq!(sanitize_filename("Order99"), "Order99");
}

#[test]
fn sanitize_filename_strips_unsafe_chars() {
    assert_eq!(sanitize_filename("admin/secrets"), "adminsecrets");
    assert_eq!(sanitize_filename("../etc/passwd"), "etcpasswd");
    assert_eq!(sanitize_filename("user list"), "userlist");
    assert_eq!(sanitize_filename("a\"b"), "ab");
}

#[test]
fn sanitize_filename_empty_for_all_unsafe() {
    assert_eq!(sanitize_filename(""), "");
    assert_eq!(sanitize_filename("///"), "");
}

// -----------------------------------------------------------------
// Cycle 3: serialization helpers
// -----------------------------------------------------------------

use serde_json::json;

#[test]
fn value_to_csv_field_handles_scalars() {
    assert_eq!(value_to_csv_field(&json!(null)), "");
    assert_eq!(value_to_csv_field(&json!(true)), "true");
    assert_eq!(value_to_csv_field(&json!(false)), "false");
    assert_eq!(value_to_csv_field(&json!(42)), "42");
    assert_eq!(value_to_csv_field(&json!(2.5)), "2.5");
    assert_eq!(value_to_csv_field(&json!("hello")), "hello");
}

#[test]
fn value_to_csv_field_emits_json_for_array_and_object() {
    assert_eq!(value_to_csv_field(&json!([1, 2, 3])), "[1,2,3]");
    assert_eq!(value_to_csv_field(&json!({"a": 1})), r#"{"a":1}"#);
}

#[test]
fn parse_select_top_level_basic() {
    assert_eq!(parse_select_top_level("id,name,email"), vec!["id", "name", "email"]);
}

#[test]
fn parse_select_top_level_strips_nested() {
    // `posts(id,title)` becomes a single `posts` column.
    assert_eq!(parse_select_top_level("id,name,posts(id,title)"), vec!["id", "name", "posts"],);
}

#[test]
fn parse_select_top_level_handles_whitespace_and_empty_segments() {
    assert_eq!(parse_select_top_level(" id , name "), vec!["id", "name"]);
    assert_eq!(parse_select_top_level(",,id,,name,,"), vec!["id", "name"]);
    assert!(parse_select_top_level("").is_empty());
    assert!(parse_select_top_level(",,,").is_empty());
}

#[test]
fn parse_select_top_level_nested_in_nested() {
    assert_eq!(parse_select_top_level("a,b(c,d(e,f)),g"), vec!["a", "b", "g"],);
}

#[test]
fn extract_select_columns_finds_param() {
    let pairs: &[(&str, &str)] = &[("select", "id,name")];
    assert_eq!(extract_select_columns(pairs), Some(vec!["id".to_string(), "name".to_string()]),);
}

#[test]
fn extract_select_columns_missing_returns_none() {
    assert!(extract_select_columns(&[]).is_none());
    assert!(extract_select_columns(&[("sort", "id")]).is_none());
}

#[test]
fn extract_select_columns_empty_returns_none() {
    assert!(extract_select_columns(&[("select", "")]).is_none());
    assert!(extract_select_columns(&[("select", " , , ")]).is_none());
}

#[test]
fn determine_columns_prefers_select_list() {
    let rows = vec![json!({"id": 1, "name": "Alice", "email": "a@b"})];
    let select = vec!["email".to_string(), "id".to_string()];
    let cols = determine_columns(Some(&select), &rows);
    assert_eq!(cols, vec!["email", "id"]);
}

#[test]
fn determine_columns_falls_back_to_first_row_keys() {
    let rows = vec![json!({"id": 1, "name": "Alice"})];
    let cols = determine_columns(None, &rows);
    // serde_json::Map is alphabetically ordered without preserve_order;
    // see module docs.
    assert_eq!(cols, vec!["id", "name"]);
}

#[test]
fn determine_columns_empty_when_no_rows_and_no_select() {
    let rows: Vec<serde_json::Value> = Vec::new();
    assert!(determine_columns(None, &rows).is_empty());
}

#[test]
fn ascii_delimiter_accepts_ascii() {
    assert_eq!(ascii_delimiter(','), b',');
    assert_eq!(ascii_delimiter(';'), b';');
    assert_eq!(ascii_delimiter('\t'), b'\t');
    assert_eq!(ascii_delimiter('|'), b'|');
}

#[test]
fn ascii_delimiter_falls_back_for_non_ascii() {
    assert_eq!(ascii_delimiter('é'), b',');
    assert_eq!(ascii_delimiter('「'), b',');
}

#[test]
fn error_csv_line_prefix_and_trailing_newline() {
    let line = error_csv_line("boom");
    let s = String::from_utf8(line.to_vec()).unwrap();
    assert!(s.starts_with("# error: "));
    assert!(s.ends_with('\n'));
    assert!(s.contains("boom"));
}

#[test]
fn error_csv_line_squashes_newlines() {
    let line = error_csv_line("first\nsecond\nthird");
    let s = String::from_utf8(line.to_vec()).unwrap();
    // exactly one trailing newline
    assert_eq!(s.matches('\n').count(), 1);
    assert!(s.contains("first second third"));
}

// -----------------------------------------------------------------
// Cycle 3: end-to-end serialisation via the csv crate
// -----------------------------------------------------------------

/// Build a small CSV body the same way `serialize_batch` would, without
/// the executor-driven `CsvStreamState` machinery. Used by the format
/// tests below.
fn render_csv(
    rows: &[serde_json::Value],
    columns: &[String],
    delimiter: u8,
    include_bom: bool,
) -> Vec<u8> {
    let mut buf: Vec<u8> = Vec::new();
    if include_bom {
        buf.extend_from_slice("\u{FEFF}".as_bytes());
    }
    {
        let mut wtr = csv::WriterBuilder::new().delimiter(delimiter).from_writer(&mut buf);
        wtr.write_record(columns.iter().map(String::as_str)).unwrap();
        for row in rows {
            let record: Vec<String> = columns
                .iter()
                .map(|c| value_to_csv_field(row.get(c).unwrap_or(&serde_json::Value::Null)))
                .collect();
            wtr.write_record(record.iter().map(String::as_str)).unwrap();
        }
        wtr.flush().unwrap();
    }
    buf
}

#[test]
fn csv_emits_header_then_rows_in_select_order() {
    let rows = vec![
        json!({"id": 1, "name": "Alice", "email": "a@b"}),
        json!({"id": 2, "name": "Bob", "email": "b@c"}),
    ];
    let cols = vec!["email".to_string(), "id".to_string(), "name".to_string()];
    let body = render_csv(&rows, &cols, b',', false);
    let s = String::from_utf8(body).unwrap();
    let mut lines = s.lines();
    assert_eq!(lines.next(), Some("email,id,name"));
    assert_eq!(lines.next(), Some("a@b,1,Alice"));
    assert_eq!(lines.next(), Some("b@c,2,Bob"));
    assert!(lines.next().is_none());
}

#[test]
fn csv_quotes_values_containing_delimiter_or_quote() {
    let rows = vec![json!({"name": "Doe, John", "note": "He said \"hi\""})];
    let cols = vec!["name".to_string(), "note".to_string()];
    let body = render_csv(&rows, &cols, b',', false);
    let s = String::from_utf8(body).unwrap();
    // RFC 4180: embedded commas and quotes force quoting + doubled quotes
    assert!(s.contains("\"Doe, John\""));
    assert!(s.contains("\"He said \"\"hi\"\"\""));
}

#[test]
fn csv_quotes_values_containing_newlines() {
    let rows = vec![json!({"note": "line1\nline2"})];
    let cols = vec!["note".to_string()];
    let body = render_csv(&rows, &cols, b',', false);
    let s = String::from_utf8(body).unwrap();
    assert!(s.contains("\"line1\nline2\""));
}

#[test]
fn csv_uses_custom_delimiter() {
    let rows = vec![json!({"a": "x,y", "b": "z"})];
    let cols = vec!["a".to_string(), "b".to_string()];
    let body = render_csv(&rows, &cols, b';', false);
    let s = String::from_utf8(body).unwrap();
    // semicolon delimiter — the comma in the value is no longer special
    // and the cell shouldn't need quoting.
    assert!(s.contains("x,y;z"));
    assert!(s.starts_with("a;b\n"));
}

#[test]
fn csv_bom_emitted_when_enabled() {
    let rows = vec![json!({"id": 1})];
    let cols = vec!["id".to_string()];
    let body = render_csv(&rows, &cols, b',', true);
    assert_eq!(&body[..3], b"\xEF\xBB\xBF");
    let rest = std::str::from_utf8(&body[3..]).unwrap();
    assert!(rest.starts_with("id\n"));
}

#[test]
fn csv_bom_absent_when_disabled() {
    let rows = vec![json!({"id": 1})];
    let cols = vec!["id".to_string()];
    let body = render_csv(&rows, &cols, b',', false);
    assert_ne!(&body[..3.min(body.len())], b"\xEF\xBB\xBF");
    let s = std::str::from_utf8(&body).unwrap();
    assert!(s.starts_with("id\n"));
}

#[test]
fn csv_null_becomes_empty_cell() {
    let rows = vec![json!({"id": 1, "name": null})];
    let cols = vec!["id".to_string(), "name".to_string()];
    let body = render_csv(&rows, &cols, b',', false);
    let s = String::from_utf8(body).unwrap();
    // Empty cell between the comma and the line break.
    assert!(s.lines().nth(1) == Some("1,"));
}

#[test]
fn csv_missing_field_becomes_empty_cell() {
    // Row only has "id"; selected column "name" is missing.
    let rows = vec![json!({"id": 1})];
    let cols = vec!["id".to_string(), "name".to_string()];
    let body = render_csv(&rows, &cols, b',', false);
    let s = String::from_utf8(body).unwrap();
    assert!(s.lines().nth(1) == Some("1,"));
}

#[test]
fn csv_nested_object_emitted_as_json_string() {
    let rows = vec![json!({"id": 1, "posts": [{"id": 10, "title": "Hi"}]})];
    let cols = vec!["id".to_string(), "posts".to_string()];
    let body = render_csv(&rows, &cols, b',', false);
    let s = String::from_utf8(body).unwrap();
    // The embedded JSON contains a comma, so it must be quoted.
    assert!(s.contains(r#""[{""id"":10,""title"":""Hi""}]""#));
}

// -----------------------------------------------------------------
// Cycle 5: edge cases via the stateless `write_csv_payload` helper
// -----------------------------------------------------------------

#[test]
fn empty_rows_with_columns_emits_header_only() {
    let cols = vec!["id".to_string(), "name".to_string()];
    let payload = write_csv_payload(&cols, &[], b',', false, true).unwrap();
    let s = String::from_utf8(payload.to_vec()).unwrap();
    assert_eq!(s, "id,name\n");
}

#[test]
fn empty_rows_with_columns_and_bom_emits_bom_then_header() {
    let cols = vec!["id".to_string()];
    let payload = write_csv_payload(&cols, &[], b',', true, true).unwrap();
    assert_eq!(&payload[..3], b"\xEF\xBB\xBF");
    let s = std::str::from_utf8(&payload[3..]).unwrap();
    assert_eq!(s, "id\n");
}

#[test]
fn subsequent_batch_emits_only_data_no_header_no_bom() {
    // First batch: BOM + header + 1 row.
    let cols = vec!["id".to_string()];
    let first = write_csv_payload(&cols, &[json!({"id": 1})], b',', true, true).unwrap();
    assert!(first.starts_with(b"\xEF\xBB\xBF"));

    // Second batch: no BOM, no header, just the row.
    let second = write_csv_payload(&cols, &[json!({"id": 2})], b',', false, false).unwrap();
    let s = std::str::from_utf8(&second).unwrap();
    assert_eq!(s, "2\n");
    assert!(!s.contains("id"));
}

#[test]
fn fully_empty_payload_when_no_header_no_bom_no_rows() {
    let cols = vec!["id".to_string()];
    let payload = write_csv_payload(&cols, &[], b',', false, false).unwrap();
    assert!(payload.is_empty());
}

#[test]
fn write_csv_payload_honors_custom_delimiter_for_header() {
    let cols = vec!["a".to_string(), "b".to_string(), "c".to_string()];
    let payload = write_csv_payload(&cols, &[], b'\t', false, true).unwrap();
    let s = String::from_utf8(payload.to_vec()).unwrap();
    assert_eq!(s, "a\tb\tc\n");
}

#[test]
fn select_columns_drive_order_when_present() {
    // ?select=email,id,name → header is "email,id,name" regardless of
    // serde_json::Map iteration order.
    let rows = vec![json!({"id": 1, "email": "a@b", "name": "Alice"})];
    let select = vec!["email".to_string(), "id".to_string(), "name".to_string()];
    let cols = determine_columns(Some(&select), &rows);
    let payload = write_csv_payload(&cols, &rows, b',', false, true).unwrap();
    let s = String::from_utf8(payload.to_vec()).unwrap();
    assert!(s.starts_with("email,id,name\n"));
    assert!(s.contains("a@b,1,Alice"));
}

#[test]
fn no_select_falls_back_to_sorted_first_row_keys() {
    // Without ?select=, the column order comes from serde_json::Map
    // iteration which the workspace's default serde_json build sorts
    // alphabetically — deterministic and assertable.
    let rows = vec![json!({"name": "Alice", "email": "a@b", "id": 1})];
    let cols = determine_columns(None, &rows);
    let payload = write_csv_payload(&cols, &rows, b',', false, true).unwrap();
    let s = String::from_utf8(payload.to_vec()).unwrap();
    assert!(s.starts_with("email,id,name\n"));
}

#[test]
fn error_line_is_distinct_from_real_data() {
    // Mid-stream error: leading `# error:`, single line.
    let err = error_csv_line("connection reset by peer");
    let s = String::from_utf8(err.to_vec()).unwrap();
    // No comma-delimiter inside the marker, so a CSV reader won't
    // confuse it with a data row that happens to start with #.
    let first_field = s.split(',').next().unwrap_or("");
    assert!(first_field.starts_with("# error: "));
    assert!(first_field.contains("connection reset"));
}
