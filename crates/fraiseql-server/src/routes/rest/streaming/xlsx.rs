//! XLSX (Office Open XML spreadsheet) response handler for the REST transport.
//!
//! When a client sends
//! `Accept: application/vnd.openxmlformats-officedocument.spreadsheetml.sheet`,
//! the GET handler delegates to this module.
//!
//! Unlike CSV and NDJSON, XLSX is a ZIP container and cannot be true-streamed —
//! the central directory at the end of the archive is only known once the
//! workbook is finalised. The handler therefore buffers the workbook to a
//! [`tempfile::NamedTempFile`] (honouring [`ExportConfig::xlsx_temp_dir`]) and
//! sends the file's bytes as the response body once the build is complete.
//!
//! Resource controls:
//! - [`ExportConfig::xlsx_max_rows`] (default `100_000`) hard-caps the row count. Exports that
//!   would exceed the cap are rejected with `413 Payload Too Large` and a body that suggests using
//!   CSV instead.
//! - [`ExportConfig::max_concurrent_xlsx`] (default `10`) gates concurrent workbook builds via a
//!   semaphore. New requests beyond the cap are rejected with `503 Service Unavailable` and a
//!   `Retry-After: 1` header — the gate is enforced at the router-dispatch site so the rejection
//!   response can carry the right header.
//!
//! Gated behind the `export-xlsx` Cargo feature.

use std::sync::Arc;

use axum::http::{HeaderMap, HeaderValue, StatusCode};
use bytes::Bytes;
use fraiseql_core::{
    db::traits::DatabaseAdapter,
    runtime::{Executor, QueryMatch},
    security::SecurityContext,
};
use rust_xlsxwriter::Workbook;
use tempfile::NamedTempFile;

use super::super::{
    export_config::ExportConfig,
    handler::{PreferHeader, ResolvedGetQuery, RestError, RestHandler, set_request_id},
    params::PaginationParams,
};

/// Content type for XLSX responses.
pub const XLSX_CONTENT_TYPE: &str =
    "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet";

/// Maximum characters in a single XLSX cell (the Excel spec limit).
///
/// Strings longer than this are truncated and suffixed with `…` so the
/// workbook can still be opened. This matches typical Excel behaviour for
/// over-long cells.
const XLSX_MAX_CELL_CHARS: usize = 32_767;

/// Check whether an `Accept` header value requests XLSX.
#[must_use]
pub fn accepts_xlsx(headers: &HeaderMap) -> bool {
    headers.get("accept").and_then(|v| v.to_str().ok()).is_some_and(|accept| {
        accept.split(',').any(|part| {
            let media = part.split(';').next().unwrap_or(part).trim();
            media.eq_ignore_ascii_case(XLSX_CONTENT_TYPE)
        })
    })
}

/// Validate that XLSX-incompatible preferences are not set.
///
/// Same constraints as NDJSON / CSV: count and pagination are unavailable
/// because the workbook is built from the full filtered result set.
///
/// # Errors
///
/// Returns `RestError::BadRequest` when count or pagination is requested
/// alongside an XLSX export.
pub fn validate_xlsx_request(
    prefer: &PreferHeader,
    pagination: &PaginationParams,
) -> Result<(), RestError> {
    if prefer.count_exact || prefer.count_planned || prefer.count_estimated {
        return Err(RestError::bad_request("count not available for export responses"));
    }

    if let PaginationParams::Offset { offset, .. } = pagination {
        if *offset > 0 {
            return Err(RestError::bad_request(
                "pagination not available for export; use filters to narrow results",
            ));
        }
    }
    if matches!(pagination, PaginationParams::Cursor { .. }) {
        return Err(RestError::bad_request(
            "pagination not available for export; use filters to narrow results",
        ));
    }

    Ok(())
}

/// Execute a query and return an XLSX workbook as the response body.
///
/// The full result set is streamed batch-by-batch from the database and
/// written to a [`tempfile::NamedTempFile`] (honouring
/// [`ExportConfig::xlsx_temp_dir`]). When the last batch has been written the
/// workbook is finalised, the file is read back into memory, and the bytes
/// are returned. The temp file is unlinked when the [`NamedTempFile`] is
/// dropped at the end of this function.
///
/// Concurrency is bounded by the caller — `rest_get_handler` acquires the
/// XLSX semaphore permit before delegating here and holds it for the duration
/// of the build.
///
/// # Errors
///
/// - `RestError::BadRequest` when count or pagination are requested alongside XLSX.
/// - `RestError` with status `413 Payload Too Large` when the result set exceeds
///   [`ExportConfig::xlsx_max_rows`]. The message suggests using `Accept: text/csv` for larger
///   exports.
/// - `RestError::Internal` when the workbook build or temp-file I/O fails.
pub async fn handle_xlsx_get<A: DatabaseAdapter + 'static>(
    handler: &RestHandler<'_, A>,
    export_config: &ExportConfig,
    relative_path: &str,
    query_pairs: &[(&str, &str)],
    headers: &HeaderMap,
    security_context: Option<&SecurityContext>,
) -> Result<XlsxResponse, RestError> {
    let resolved = handler.resolve_get_query(relative_path, query_pairs, security_context)?;

    let prefer = PreferHeader::from_headers(headers);
    validate_xlsx_request(&prefer, &resolved.params.pagination)?;

    let ResolvedGetQuery {
        query_name,
        query_match,
        variables,
        ..
    } = resolved;

    let mut response_headers = HeaderMap::new();
    set_request_id(headers, &mut response_headers);
    response_headers.insert("content-type", HeaderValue::from_static(XLSX_CONTENT_TYPE));

    let filename = sanitize_filename(&query_name);
    let disposition = if filename.is_empty() {
        "attachment; filename=\"export.xlsx\"".to_string()
    } else {
        format!("attachment; filename=\"{filename}.xlsx\"")
    };
    response_headers.insert(
        "content-disposition",
        HeaderValue::from_str(&disposition)
            .unwrap_or_else(|_| HeaderValue::from_static("attachment; filename=\"export.xlsx\"")),
    );

    let batch_size = handler.config().ndjson_batch_size.max(1);
    let select_columns = extract_select_columns(query_pairs);

    let executor = Arc::clone(handler.executor());
    let security_ctx_owned = security_context.cloned();

    let bytes = build_workbook(BuildContext {
        executor,
        query_name: query_name.clone(),
        query_match,
        variables,
        security_ctx: security_ctx_owned,
        batch_size,
        max_rows: export_config.xlsx_max_rows,
        select_columns,
        temp_dir: export_config.xlsx_temp_dir.clone(),
    })
    .await?;

    Ok(XlsxResponse {
        headers: response_headers,
        body:    XlsxBody::Bytes(bytes),
    })
}

/// Reduce a query name to characters safe inside an HTTP filename token.
fn sanitize_filename(name: &str) -> String {
    name.chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
        .collect()
}

/// XLSX response.
pub struct XlsxResponse {
    /// Response headers (content-type, content-disposition, request-id).
    pub headers: HeaderMap,
    /// Workbook body — always pre-buffered (XLSX cannot stream).
    pub body:    XlsxBody,
}

/// Body of an XLSX response.
///
/// XLSX is a ZIP container; the body is always materialised in full before
/// being sent. The variant is `#[non_exhaustive]` so a future tempfile-backed
/// streaming variant can be added without breaking callers.
#[non_exhaustive]
pub enum XlsxBody {
    /// Pre-buffered workbook bytes (read from the build temp file).
    Bytes(Bytes),
}

impl XlsxBody {
    /// Convert to an axum `Body`.
    pub fn into_body(self) -> axum::body::Body {
        match self {
            Self::Bytes(bytes) => axum::body::Body::from(bytes),
        }
    }
}

// ---------------------------------------------------------------------------
// Workbook builder
// ---------------------------------------------------------------------------

/// Inputs to the workbook builder loop.
struct BuildContext<A: DatabaseAdapter> {
    executor:       Arc<Executor<A>>,
    query_name:     String,
    query_match:    QueryMatch,
    variables:      serde_json::Value,
    security_ctx:   Option<SecurityContext>,
    batch_size:     u64,
    max_rows:       u64,
    /// Column order from `?select=`, when supplied.
    select_columns: Option<Vec<String>>,
    /// Optional override for the temp-file directory.
    temp_dir:       Option<std::path::PathBuf>,
}

/// Drive the executor batch loop and produce the workbook bytes.
///
/// Streams rows from the database in batches of `batch_size`, writes them to
/// the worksheet, and enforces `max_rows`. The workbook is built in
/// `constant_memory` mode so the in-progress worksheet data lives on disk
/// (inside `rust_xlsxwriter`) and peak heap stays bounded.
async fn build_workbook<A: DatabaseAdapter>(ctx: BuildContext<A>) -> Result<Bytes, RestError> {
    let temp_file = create_temp_file(ctx.temp_dir.as_deref())?;

    let mut workbook = Workbook::new();
    let worksheet = workbook.add_worksheet_with_constant_memory();

    let mut columns: Option<Vec<String>> = None;
    let mut rows_written: u64 = 0;
    let mut offset: u64 = 0;
    let mut done = false;

    while !done {
        let rows = fetch_batch(&ctx, offset).await?;

        if rows.is_empty() {
            done = true;
            continue;
        }

        if columns.is_none() {
            let cols = determine_columns(ctx.select_columns.as_deref(), &rows);
            write_header_row(worksheet, &cols)?;
            columns = Some(cols);
        }

        let active_columns =
            columns.as_ref().expect("columns initialised on first non-empty batch above");

        for row in &rows {
            if rows_written >= ctx.max_rows {
                return Err(too_many_rows_error(ctx.max_rows));
            }
            let row_idx = u32::try_from(rows_written + 1)
                .map_err(|_| RestError::internal("XLSX row index overflow"))?;
            write_data_row(worksheet, row_idx, active_columns, row)?;
            rows_written += 1;
        }

        #[allow(clippy::cast_possible_truncation)]
        // Reason: rows.len() fits in u64 in any realistic batch.
        let row_count = rows.len() as u64;
        if row_count < ctx.batch_size {
            done = true;
        } else {
            offset += ctx.batch_size;
        }
    }

    // Empty result set → header-less, single-sheet workbook is still a valid
    // file. Excel happily opens it.
    workbook
        .save(temp_file.path())
        .map_err(|e| RestError::internal(format!("XLSX save failed: {e}")))?;

    let bytes = tokio::fs::read(temp_file.path())
        .await
        .map_err(|e| RestError::internal(format!("XLSX temp-file read failed: {e}")))?;

    // `temp_file` drops here; the NamedTempFile cleanup deletes the underlying
    // path. Holding it until after the read prevents premature cleanup on
    // platforms (e.g. NFS) that block reads of unlinked files.
    drop(temp_file);

    Ok(Bytes::from(bytes))
}

fn create_temp_file(dir: Option<&std::path::Path>) -> Result<NamedTempFile, RestError> {
    let mut builder = tempfile::Builder::new();
    builder.prefix("fraiseql-xlsx-").suffix(".xlsx");
    let file = match dir {
        Some(d) => builder.tempfile_in(d),
        None => builder.tempfile(),
    };
    file.map_err(|e| RestError::internal(format!("XLSX temp-file create failed: {e}")))
}

async fn fetch_batch<A: DatabaseAdapter>(
    ctx: &BuildContext<A>,
    offset: u64,
) -> Result<Vec<serde_json::Value>, RestError> {
    let mut batch_vars = ctx.variables.clone();
    if let Some(obj) = batch_vars.as_object_mut() {
        obj.insert("limit".to_string(), serde_json::json!(ctx.batch_size));
        if offset > 0 {
            obj.insert("offset".to_string(), serde_json::json!(offset));
        }
    }
    let vars_ref = if batch_vars.as_object().is_none_or(serde_json::Map::is_empty) {
        None
    } else {
        Some(&batch_vars)
    };

    let result_value = ctx
        .executor
        .execute_query_direct(&ctx.query_match, vars_ref, ctx.security_ctx.as_ref())
        .await
        .map_err(|e| RestError::internal(format!("XLSX query execution failed: {e}")))?;

    super::helpers::extract_rows(&result_value, &ctx.query_name)
}

fn write_header_row(
    worksheet: &mut rust_xlsxwriter::Worksheet,
    columns: &[String],
) -> Result<(), RestError> {
    for (col_idx, name) in columns.iter().enumerate() {
        let col = u16::try_from(col_idx)
            .map_err(|_| RestError::internal("XLSX column index overflow"))?;
        worksheet
            .write_string(0, col, truncate_for_xlsx(name))
            .map_err(|e| RestError::internal(format!("XLSX header write failed: {e}")))?;
    }
    Ok(())
}

fn write_data_row(
    worksheet: &mut rust_xlsxwriter::Worksheet,
    row_idx: u32,
    columns: &[String],
    row: &serde_json::Value,
) -> Result<(), RestError> {
    for (col_idx, col_name) in columns.iter().enumerate() {
        let col = u16::try_from(col_idx)
            .map_err(|_| RestError::internal("XLSX column index overflow"))?;
        let value = row.get(col_name).unwrap_or(&serde_json::Value::Null);
        write_cell(worksheet, row_idx, col, value)?;
    }
    Ok(())
}

/// Type-dispatched cell writer.
///
/// - `Null` → leave the cell blank.
/// - `Bool` → boolean cell.
/// - `Number` → numeric cell (`f64` precision).
/// - `String` → string cell (truncated to `XLSX_MAX_CELL_CHARS`).
/// - Array/Object → JSON-encoded into a single string cell.
fn write_cell(
    worksheet: &mut rust_xlsxwriter::Worksheet,
    row: u32,
    col: u16,
    value: &serde_json::Value,
) -> Result<(), RestError> {
    match value {
        serde_json::Value::Null => Ok(()),
        serde_json::Value::Bool(b) => worksheet.write_boolean(row, col, *b).map(|_| ()),
        serde_json::Value::Number(n) => match n.as_f64() {
            Some(f) => worksheet.write_number(row, col, f).map(|_| ()),
            // Integers above f64 range fall back to a string cell so we
            // don't silently lose precision (Excel itself can't represent
            // 64-bit integers as numbers).
            None => worksheet.write_string(row, col, truncate_for_xlsx(&n.to_string())).map(|_| ()),
        },
        serde_json::Value::String(s) => {
            worksheet.write_string(row, col, truncate_for_xlsx(s)).map(|_| ())
        },
        other => worksheet
            .write_string(
                row,
                col,
                truncate_for_xlsx(&serde_json::to_string(other).unwrap_or_default()),
            )
            .map(|_| ()),
    }
    .map_err(|e| RestError::internal(format!("XLSX cell write failed: {e}")))
}

/// Truncate a string to fit within Excel's per-cell character limit.
///
/// Strings under the limit are returned unchanged. Over-long strings are
/// shortened to `XLSX_MAX_CELL_CHARS - 1` characters and suffixed with `…`
/// so the truncation is visible inside Excel.
fn truncate_for_xlsx(s: &str) -> String {
    if s.chars().count() <= XLSX_MAX_CELL_CHARS {
        return s.to_string();
    }
    let mut out: String = s.chars().take(XLSX_MAX_CELL_CHARS - 1).collect();
    out.push('…');
    out
}

fn too_many_rows_error(max_rows: u64) -> RestError {
    RestError {
        status:  StatusCode::PAYLOAD_TOO_LARGE,
        code:    "XLSX_ROW_LIMIT_EXCEEDED",
        message: format!(
            "XLSX export exceeds the {max_rows}-row cap; request `Accept: text/csv` for larger \
             result sets"
        ),
        details: None,
    }
}

// ---------------------------------------------------------------------------
// Column selection (mirrors the CSV sibling)
// ---------------------------------------------------------------------------

/// Decide column ordering for the workbook.
///
/// Preference:
/// 1. `?select=` order, when supplied.
/// 2. First row's `serde_json::Map` iteration order (alphabetical under the workspace's default
///    `serde_json` build).
fn determine_columns(select_columns: Option<&[String]>, rows: &[serde_json::Value]) -> Vec<String> {
    if let Some(cols) = select_columns {
        return cols.to_vec();
    }
    rows.first()
        .and_then(|v| v.as_object())
        .map(|m| m.keys().cloned().collect())
        .unwrap_or_default()
}

/// Extract `?select=` top-level columns from the request's query pairs.
fn extract_select_columns(query_pairs: &[(&str, &str)]) -> Option<Vec<String>> {
    let raw = query_pairs.iter().find(|(k, _)| *k == "select").map(|(_, v)| *v)?;
    let cols = parse_select_top_level(raw);
    if cols.is_empty() { None } else { Some(cols) }
}

/// Paren-aware split of `?select=` into top-level column names.
fn parse_select_top_level(select_raw: &str) -> Vec<String> {
    let mut cols = Vec::new();
    let mut depth = 0_usize;
    let mut current = String::new();
    for c in select_raw.chars() {
        match c {
            '(' => {
                depth += 1;
                current.push(c);
            },
            ')' => {
                depth = depth.saturating_sub(1);
                current.push(c);
            },
            ',' if depth == 0 => {
                push_top_level(&mut cols, &current);
                current.clear();
            },
            _ => current.push(c),
        }
    }
    push_top_level(&mut cols, &current);
    cols
}

fn push_top_level(cols: &mut Vec<String>, current: &str) {
    let trimmed = current.trim();
    if trimmed.is_empty() {
        return;
    }
    let head = trimmed.split('(').next().unwrap_or("").trim();
    if !head.is_empty() {
        cols.push(head.to_string());
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)] // Reason: tests follow the CSV sibling module's convention.
mod tests {
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
}
