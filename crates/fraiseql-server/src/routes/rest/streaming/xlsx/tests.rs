//! Tests for the `streaming::xlsx` module.

#![allow(clippy::unwrap_used)] // Reason: tests follow the CSV sibling module's convention.

use serde_json::json;

use super::*;

// -----------------------------------------------------------------
// Cycle 6a: accept / validation / sanitisation
// -----------------------------------------------------------------

#[test]
fn xlsx_content_type_constant() {
    assert_eq!(
        XLSX_CONTENT_TYPE,
        "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"
    );
}

#[test]
fn accepts_xlsx_true_for_exact_match() {
    let mut headers = HeaderMap::new();
    headers.insert("accept", HeaderValue::from_static(XLSX_CONTENT_TYPE));
    assert!(accepts_xlsx(&headers));
}

#[test]
fn accepts_xlsx_true_in_list() {
    let mut headers = HeaderMap::new();
    let value = format!("application/json, {XLSX_CONTENT_TYPE}");
    headers.insert("accept", HeaderValue::from_str(&value).unwrap());
    assert!(accepts_xlsx(&headers));
}

#[test]
fn accepts_xlsx_false_for_json() {
    let mut headers = HeaderMap::new();
    headers.insert("accept", HeaderValue::from_static("application/json"));
    assert!(!accepts_xlsx(&headers));
}

#[test]
fn accepts_xlsx_false_for_csv() {
    let mut headers = HeaderMap::new();
    headers.insert("accept", HeaderValue::from_static("text/csv"));
    assert!(!accepts_xlsx(&headers));
}

#[test]
fn accepts_xlsx_false_when_missing() {
    let headers = HeaderMap::new();
    assert!(!accepts_xlsx(&headers));
}

#[test]
fn accepts_xlsx_case_insensitive() {
    let mut headers = HeaderMap::new();
    let upper = XLSX_CONTENT_TYPE.to_ascii_uppercase();
    headers.insert("accept", HeaderValue::from_str(&upper).unwrap());
    assert!(accepts_xlsx(&headers));
}

#[test]
fn accepts_xlsx_ignores_quality_params() {
    let mut headers = HeaderMap::new();
    let value = format!("{XLSX_CONTENT_TYPE};q=0.9, application/json;q=0.8");
    headers.insert("accept", HeaderValue::from_str(&value).unwrap());
    assert!(accepts_xlsx(&headers));
}

#[test]
fn accepts_xlsx_does_not_match_plain_xml() {
    let mut headers = HeaderMap::new();
    headers.insert("accept", HeaderValue::from_static("application/xml"));
    assert!(!accepts_xlsx(&headers));
}

#[test]
fn validate_xlsx_rejects_count_exact() {
    let prefer = PreferHeader {
        count_exact: true,
        ..PreferHeader::default()
    };
    let err = validate_xlsx_request(&prefer, &PaginationParams::None).unwrap_err();
    assert_eq!(err.status, StatusCode::BAD_REQUEST);
    assert!(err.message.contains("count not available"));
}

#[test]
fn validate_xlsx_rejects_count_planned() {
    let prefer = PreferHeader {
        count_planned: true,
        ..PreferHeader::default()
    };
    let err = validate_xlsx_request(&prefer, &PaginationParams::None).unwrap_err();
    assert_eq!(err.status, StatusCode::BAD_REQUEST);
}

#[test]
fn validate_xlsx_rejects_count_estimated() {
    let prefer = PreferHeader {
        count_estimated: true,
        ..PreferHeader::default()
    };
    let err = validate_xlsx_request(&prefer, &PaginationParams::None).unwrap_err();
    assert_eq!(err.status, StatusCode::BAD_REQUEST);
}

#[test]
fn validate_xlsx_rejects_cursor_pagination() {
    let prefer = PreferHeader::default();
    let pagination = PaginationParams::Cursor {
        first:  Some(10),
        after:  None,
        last:   None,
        before: None,
    };
    let err = validate_xlsx_request(&prefer, &pagination).unwrap_err();
    assert_eq!(err.status, StatusCode::BAD_REQUEST);
    assert!(err.message.contains("pagination not available"));
}

#[test]
fn validate_xlsx_rejects_offset_pagination() {
    let prefer = PreferHeader::default();
    let pagination = PaginationParams::Offset {
        limit:  10,
        offset: 5,
    };
    let err = validate_xlsx_request(&prefer, &pagination).unwrap_err();
    assert_eq!(err.status, StatusCode::BAD_REQUEST);
}

#[test]
fn validate_xlsx_allows_limit_only() {
    let prefer = PreferHeader::default();
    let pagination = PaginationParams::Offset {
        limit:  100,
        offset: 0,
    };
    assert!(validate_xlsx_request(&prefer, &pagination).is_ok());
}

#[test]
fn validate_xlsx_allows_no_pagination() {
    let prefer = PreferHeader::default();
    assert!(validate_xlsx_request(&prefer, &PaginationParams::None).is_ok());
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
// Cycle 6b: column selection (mirror CSV)
// -----------------------------------------------------------------

#[test]
fn parse_select_top_level_basic() {
    assert_eq!(parse_select_top_level("id,name,email"), vec!["id", "name", "email"]);
}

#[test]
fn parse_select_top_level_strips_nested() {
    assert_eq!(parse_select_top_level("id,name,posts(id,title)"), vec!["id", "name", "posts"]);
}

#[test]
fn parse_select_top_level_handles_whitespace_and_empty_segments() {
    assert_eq!(parse_select_top_level(" id , name "), vec!["id", "name"]);
    assert_eq!(parse_select_top_level(",,id,,name,,"), vec!["id", "name"]);
    assert!(parse_select_top_level("").is_empty());
    assert!(parse_select_top_level(",,,").is_empty());
}

#[test]
fn extract_select_columns_finds_param() {
    let pairs: &[(&str, &str)] = &[("select", "id,name")];
    assert_eq!(extract_select_columns(pairs), Some(vec!["id".to_string(), "name".to_string()]));
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
    assert_eq!(cols, vec!["id", "name"]);
}

#[test]
fn determine_columns_empty_when_no_rows_and_no_select() {
    let rows: Vec<serde_json::Value> = Vec::new();
    assert!(determine_columns(None, &rows).is_empty());
}

// -----------------------------------------------------------------
// Cycle 6b: cell-writer + workbook round-trip
// -----------------------------------------------------------------

/// Build a workbook in memory by writing header + rows the same way the
/// production builder would. Returns the finalised XLSX bytes.
fn render_workbook(columns: &[String], rows: &[serde_json::Value]) -> Vec<u8> {
    let mut workbook = Workbook::new();
    let worksheet = workbook.add_worksheet();
    write_header_row(worksheet, columns).unwrap();
    for (idx, row) in rows.iter().enumerate() {
        #[allow(clippy::cast_possible_truncation)] // Reason: test rows fit in u32.
        let row_idx = (idx + 1) as u32;
        write_data_row(worksheet, row_idx, columns, row).unwrap();
    }
    workbook.save_to_buffer().unwrap()
}

#[test]
fn render_workbook_produces_valid_zip_magic() {
    let cols = vec!["id".to_string(), "name".to_string()];
    let rows = vec![json!({"id": 1, "name": "Alice"})];
    let bytes = render_workbook(&cols, &rows);
    // XLSX is a ZIP container; first 4 bytes are the local file header magic.
    assert_eq!(&bytes[..4], b"PK\x03\x04");
    // A non-trivial workbook is several KiB even for one row.
    assert!(bytes.len() > 1024, "workbook bytes too small: {}", bytes.len());
}

#[test]
fn render_workbook_empty_rows_still_valid() {
    let cols = vec!["id".to_string()];
    let bytes = render_workbook(&cols, &[]);
    assert_eq!(&bytes[..4], b"PK\x03\x04");
}

#[test]
fn render_workbook_handles_all_scalar_types() {
    let cols = vec![
        "s".into(),
        "n".into(),
        "b".into(),
        "z".into(),
        "arr".into(),
        "obj".into(),
    ];
    let rows = vec![json!({
        "s": "hello",
        "n": 42.5,
        "b": true,
        "z": null,
        "arr": [1, 2, 3],
        "obj": {"k": "v"},
    })];
    let bytes = render_workbook(&cols, &rows);
    // Each cell type round-trips into the workbook without error.
    assert_eq!(&bytes[..4], b"PK\x03\x04");
}

#[test]
fn render_workbook_handles_missing_field_as_blank() {
    // Row only has "id"; column list includes "name" which is missing.
    let cols = vec!["id".into(), "name".into()];
    let rows = vec![json!({"id": 7})];
    let bytes = render_workbook(&cols, &rows);
    assert_eq!(&bytes[..4], b"PK\x03\x04");
}

#[test]
fn truncate_for_xlsx_short_string_unchanged() {
    assert_eq!(truncate_for_xlsx("hello"), "hello");
    assert_eq!(truncate_for_xlsx(""), "");
}

#[test]
fn truncate_for_xlsx_long_string_capped_with_ellipsis() {
    let long = "a".repeat(XLSX_MAX_CELL_CHARS + 100);
    let out = truncate_for_xlsx(&long);
    assert_eq!(out.chars().count(), XLSX_MAX_CELL_CHARS);
    assert!(out.ends_with('…'));
}

#[test]
fn truncate_for_xlsx_at_limit_unchanged() {
    let exact = "a".repeat(XLSX_MAX_CELL_CHARS);
    assert_eq!(truncate_for_xlsx(&exact), exact);
}

#[test]
fn too_many_rows_error_carries_413_and_mentions_csv() {
    let err = too_many_rows_error(100_000);
    assert_eq!(err.status, StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(err.code, "XLSX_ROW_LIMIT_EXCEEDED");
    assert!(err.message.contains("100000"));
    assert!(err.message.contains("text/csv"));
}

#[test]
fn create_temp_file_default_dir_succeeds() {
    let tf = create_temp_file(None).unwrap();
    assert!(tf.path().exists(), "temp file should exist on disk");
    // The file is unlinked when `tf` drops.
}

#[test]
fn create_temp_file_custom_dir_uses_that_dir() {
    let dir = tempfile::tempdir().unwrap();
    let tf = create_temp_file(Some(dir.path())).unwrap();
    assert!(
        tf.path().starts_with(dir.path()),
        "temp file {:?} should live under custom dir {:?}",
        tf.path(),
        dir.path(),
    );
}
