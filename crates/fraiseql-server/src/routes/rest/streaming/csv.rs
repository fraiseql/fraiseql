//! CSV streaming response handler for the REST transport.
//!
//! When a client sends `Accept: text/csv`, the GET handler delegates to this
//! module. Like NDJSON, CSV is streamed in `O(batch_size)` memory rather than
//! buffering the full result set.
//!
//! Output format:
//! - Optional UTF-8 BOM (`\u{FEFF}`) at the start, controlled by [`ExportConfig::csv_include_bom`]
//!   (default `true` — Excel needs it).
//! - One header row whose columns are the top-level fields of the query result. If `?select=a,b,c`
//!   is provided the column order matches that list (paren-aware: `posts(id,title)` becomes a
//!   single `posts` column); otherwise columns come from `serde_json::Map` iteration, which is
//!   stable (alphabetical) under the default `serde_json` build used in this workspace.
//! - One row per result, RFC 4180 quoting, configurable delimiter via
//!   [`ExportConfig::csv_delimiter`].
//!
//! Scalar values map to their string form; `Null` becomes an empty cell;
//! arrays and objects (embedded relationships) are JSON-serialised into a
//! single cell.
//!
//! Gated behind the `export-csv` Cargo feature.

use std::sync::Arc;

use axum::http::{HeaderMap, HeaderValue};
use bytes::Bytes;
use fraiseql_core::{
    db::traits::DatabaseAdapter,
    runtime::{Executor, QueryMatch},
    security::SecurityContext,
};
use futures::stream;

use super::super::{
    export_config::ExportConfig,
    handler::{PreferHeader, ResolvedGetQuery, RestError, RestHandler, set_request_id},
    params::PaginationParams,
};

/// Content type for CSV responses.
pub const CSV_CONTENT_TYPE: &str = "text/csv";

/// Check whether an `Accept` header value requests CSV.
#[must_use]
pub fn accepts_csv(headers: &HeaderMap) -> bool {
    headers.get("accept").and_then(|v| v.to_str().ok()).is_some_and(|accept| {
        accept.split(',').any(|part| {
            // Strip any parameters (`;q=0.5`, `;charset=utf-8`, etc.).
            let media = part.split(';').next().unwrap_or(part).trim();
            media.eq_ignore_ascii_case(CSV_CONTENT_TYPE)
        })
    })
}

/// Validate that CSV-incompatible preferences are not set.
///
/// Same constraints as NDJSON: count and pagination are unavailable for
/// streaming responses.
///
/// # Errors
///
/// Returns `RestError::BadRequest` when count or pagination is requested
/// alongside CSV streaming.
pub fn validate_csv_request(
    prefer: &PreferHeader,
    pagination: &PaginationParams,
) -> Result<(), RestError> {
    if prefer.count_exact || prefer.count_planned || prefer.count_estimated {
        return Err(RestError::bad_request("count not available for streaming responses"));
    }

    if let PaginationParams::Offset { offset, .. } = pagination {
        if *offset > 0 {
            return Err(RestError::bad_request(
                "pagination not available for streaming; use filters to narrow results",
            ));
        }
    }
    if matches!(pagination, PaginationParams::Cursor { .. }) {
        return Err(RestError::bad_request(
            "pagination not available for streaming; use filters to narrow results",
        ));
    }

    Ok(())
}

/// Execute a query and return results as a streaming CSV response.
///
/// Mirrors [`super::handle_ndjson_get`]: rows are fetched in batches of
/// `rest_config.ndjson_batch_size`, serialised, and streamed to the client.
/// The first batch emits an optional UTF-8 BOM and a header row; subsequent
/// batches emit only data rows.
///
/// # Errors
///
/// Returns `RestError` on route resolution, parameter extraction, or initial
/// query setup failure. Errors that occur mid-stream are emitted as a final
/// CSV record `# error: <message>` (the leading `#` and the absence of the
/// configured delimiter make the line clearly distinguishable from data).
pub async fn handle_csv_get<A: DatabaseAdapter + 'static>(
    handler: &RestHandler<'_, A>,
    export_config: &ExportConfig,
    relative_path: &str,
    query_pairs: &[(&str, &str)],
    headers: &HeaderMap,
    security_context: Option<&SecurityContext>,
) -> Result<CsvResponse, RestError> {
    let resolved = handler.resolve_get_query(relative_path, query_pairs, security_context)?;

    let prefer = PreferHeader::from_headers(headers);
    validate_csv_request(&prefer, &resolved.params.pagination)?;

    let ResolvedGetQuery {
        query_name,
        query_match,
        variables,
        ..
    } = resolved;

    let mut response_headers = HeaderMap::new();
    set_request_id(headers, &mut response_headers);
    response_headers.insert("content-type", HeaderValue::from_static(CSV_CONTENT_TYPE));

    let filename = sanitize_filename(&query_name);
    let disposition = if filename.is_empty() {
        "attachment; filename=\"export.csv\"".to_string()
    } else {
        format!("attachment; filename=\"{filename}.csv\"")
    };
    response_headers.insert(
        "content-disposition",
        HeaderValue::from_str(&disposition)
            .unwrap_or_else(|_| HeaderValue::from_static("attachment; filename=\"export.csv\"")),
    );

    let batch_size = handler.config().ndjson_batch_size.max(1);
    let delimiter = ascii_delimiter(export_config.csv_delimiter);
    let select_columns = extract_select_columns(query_pairs);

    let executor = Arc::clone(handler.executor());
    let security_ctx_owned = security_context.cloned();

    let csv_stream = stream::unfold(
        CsvStreamState {
            executor,
            query_name,
            query_match,
            variables,
            security_ctx: security_ctx_owned,
            batch_size,
            offset: 0,
            done: false,
            delimiter,
            include_bom: export_config.csv_include_bom,
            select_columns,
            columns: None,
            header_emitted: false,
        },
        |mut state| async move {
            if state.done {
                return None;
            }
            match fetch_and_serialize_csv_batch(&mut state).await {
                Ok(Some(bytes)) => Some((Ok(bytes), state)),
                Ok(None) => None,
                Err(err_bytes) => {
                    state.done = true;
                    Some((Ok(err_bytes), state))
                },
            }
        },
    );

    Ok(CsvResponse {
        headers: response_headers,
        body:    CsvBody::Stream(Box::pin(csv_stream)),
    })
}

/// Reduce a query name to characters safe inside an HTTP filename token.
///
/// Keeps ASCII alphanumerics plus `_` and `-`; drops everything else.
fn sanitize_filename(name: &str) -> String {
    name.chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
        .collect()
}

/// CSV streaming response.
pub struct CsvResponse {
    /// Response headers (content-type, content-disposition, request-id).
    pub headers: HeaderMap,
    /// CSV body — currently always a stream.
    pub body:    CsvBody,
}

/// Body of a CSV response.
#[non_exhaustive]
pub enum CsvBody {
    /// Streaming body (batched execution).
    Stream(
        std::pin::Pin<
            Box<dyn futures::Stream<Item = Result<Bytes, std::convert::Infallible>> + Send>,
        >,
    ),
}

impl CsvBody {
    /// Convert to an axum `Body`.
    pub fn into_body(self) -> axum::body::Body {
        match self {
            Self::Stream(stream) => axum::body::Body::from_stream(stream),
        }
    }
}

// ---------------------------------------------------------------------------
// Streaming internals
// ---------------------------------------------------------------------------

/// Internal state carried through the streaming unfold loop.
struct CsvStreamState<A: DatabaseAdapter> {
    executor:       Arc<Executor<A>>,
    query_name:     String,
    query_match:    QueryMatch,
    variables:      serde_json::Value,
    security_ctx:   Option<SecurityContext>,
    batch_size:     u64,
    offset:         u64,
    done:           bool,
    delimiter:      u8,
    include_bom:    bool,
    /// Column order parsed from `?select=`. `None` means "infer from the
    /// first row's keys (sorted)".
    select_columns: Option<Vec<String>>,
    /// Column list finalised on the first non-empty batch.
    columns:        Option<Vec<String>>,
    /// Tracks whether the header row has been written yet.
    header_emitted: bool,
}

/// Fetch the next batch and serialise it as CSV bytes.
///
/// On the first non-empty batch this writes the optional BOM and the header
/// row. Subsequent batches write only data rows.
async fn fetch_and_serialize_csv_batch<A: DatabaseAdapter>(
    state: &mut CsvStreamState<A>,
) -> Result<Option<Bytes>, Bytes> {
    let mut batch_vars = state.variables.clone();
    if let Some(obj) = batch_vars.as_object_mut() {
        obj.insert("limit".to_string(), serde_json::json!(state.batch_size));
        if state.offset > 0 {
            obj.insert("offset".to_string(), serde_json::json!(state.offset));
        }
    }
    let vars_ref = if batch_vars.as_object().is_none_or(serde_json::Map::is_empty) {
        None
    } else {
        Some(&batch_vars)
    };

    let result_value = match state
        .executor
        .execute_query_direct(&state.query_match, vars_ref, state.security_ctx.as_ref())
        .await
    {
        Ok(r) => r,
        Err(e) => {
            state.done = true;
            return Err(error_csv_line(&e.to_string()));
        },
    };

    let rows = match super::helpers::extract_rows(&result_value, &state.query_name) {
        Ok(r) => r,
        Err(e) => {
            state.done = true;
            return Err(error_csv_line(&e.message));
        },
    };

    if rows.is_empty() {
        // Emit a header-only response if no batch ever produced data and a
        // `?select=` column list is known. Otherwise terminate cleanly.
        if !state.header_emitted && state.offset == 0 {
            if let Some(cols) = state.select_columns.clone() {
                state.columns = Some(cols);
                let bytes = match serialize_batch(state, &[]) {
                    Ok(b) => b,
                    Err(err_bytes) => {
                        state.done = true;
                        return Err(err_bytes);
                    },
                };
                state.done = true;
                return Ok(Some(bytes));
            }
        }
        state.done = true;
        return Ok(None);
    }

    if state.columns.is_none() {
        state.columns = Some(determine_columns(state.select_columns.as_deref(), &rows));
    }

    let bytes = match serialize_batch(state, &rows) {
        Ok(b) => b,
        Err(err_bytes) => {
            state.done = true;
            return Err(err_bytes);
        },
    };

    #[allow(clippy::cast_possible_truncation)]
    // Reason: rows.len() fits in u64 in any realistic batch.
    let row_count = rows.len() as u64;
    if row_count < state.batch_size {
        state.done = true;
    } else {
        state.offset += state.batch_size;
    }

    Ok(Some(bytes))
}

/// Serialise one batch (possibly empty) into bytes, emitting BOM + header on
/// the first call.
fn serialize_batch<A: DatabaseAdapter>(
    state: &mut CsvStreamState<A>,
    rows: &[serde_json::Value],
) -> Result<Bytes, Bytes> {
    let columns = state
        .columns
        .as_ref()
        .ok_or_else(|| error_csv_line("internal error: columns not initialised"))?;

    let payload = write_csv_payload(
        columns,
        rows,
        state.delimiter,
        state.include_bom && !state.header_emitted,
        !state.header_emitted,
    )?;
    state.header_emitted = true;
    Ok(payload)
}

/// Stateless CSV chunk writer. Shared by [`serialize_batch`] and the unit
/// tests.
///
/// - `emit_bom`: prepend the UTF-8 BOM bytes.
/// - `emit_header`: write the column header row before any data rows.
///
/// csv-writer errors are converted into the same `# error:` line format used
/// for mid-stream failures, so the caller can pass them through the stream.
fn write_csv_payload(
    columns: &[String],
    rows: &[serde_json::Value],
    delimiter: u8,
    emit_bom: bool,
    emit_header: bool,
) -> Result<Bytes, Bytes> {
    let mut buf: Vec<u8> = Vec::new();
    if emit_bom {
        buf.extend_from_slice("\u{FEFF}".as_bytes());
    }

    {
        let mut wtr = csv::WriterBuilder::new().delimiter(delimiter).from_writer(&mut buf);

        if emit_header {
            wtr.write_record(columns.iter().map(String::as_str))
                .map_err(|e| error_csv_line(&e.to_string()))?;
        }

        for row in rows {
            let record: Vec<String> = columns
                .iter()
                .map(|c| value_to_csv_field(row.get(c).unwrap_or(&serde_json::Value::Null)))
                .collect();
            wtr.write_record(record.iter().map(String::as_str))
                .map_err(|e| error_csv_line(&e.to_string()))?;
        }

        wtr.flush().map_err(|e| error_csv_line(&e.to_string()))?;
    }

    Ok(Bytes::from(buf))
}

/// Build a clearly-marked error line for mid-stream failures.
///
/// The leading `# error:` keeps the line distinguishable from real CSV data
/// regardless of the configured delimiter — RFC 4180 has no comment syntax,
/// but consumers can grep this prefix to detect a truncated export.
fn error_csv_line(message: &str) -> Bytes {
    // Strip newlines so the marker stays on one line; readers tail-scanning
    // for it shouldn't have to handle multi-line errors.
    let one_line: String = message.chars().map(|c| if c == '\n' { ' ' } else { c }).collect();
    Bytes::from(format!("# error: {one_line}\n"))
}

/// Convert a JSON cell to its CSV string representation.
///
/// Scalar values use their natural string form; `Null` becomes empty; arrays
/// and objects (e.g. embedded relationships) are emitted as their JSON
/// representation inside a single cell. The csv writer handles quoting and
/// escaping based on the delimiter.
fn value_to_csv_field(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::Null => String::new(),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::String(s) => s.clone(),
        other => serde_json::to_string(other).unwrap_or_default(),
    }
}

/// Decide column ordering for the CSV output.
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
///
/// Returns `None` if the parameter is absent, empty, or contains no usable
/// top-level field names.
fn extract_select_columns(query_pairs: &[(&str, &str)]) -> Option<Vec<String>> {
    let raw = query_pairs.iter().find(|(k, _)| *k == "select").map(|(_, v)| *v)?;
    let cols = parse_select_top_level(raw);
    if cols.is_empty() { None } else { Some(cols) }
}

/// Paren-aware split of `?select=` into top-level column names.
///
/// `id,name,posts(id,title)` → `["id", "name", "posts"]`.
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
    // For `posts(id,title)` we only want the bare `posts` part.
    let head = trimmed.split('(').next().unwrap_or("").trim();
    if !head.is_empty() {
        cols.push(head.to_string());
    }
}

/// Coerce a `char` delimiter into the single byte the csv writer expects.
///
/// Falls back to comma when the configured delimiter is not a single ASCII
/// byte — `csv::WriterBuilder::delimiter` rejects multi-byte delimiters at
/// runtime, so this guard keeps the writer constructible.
const fn ascii_delimiter(c: char) -> u8 {
    if c.is_ascii() && c.len_utf8() == 1 {
        c as u8
    } else {
        b','
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)] // Reason: tests follow the NDJSON sibling module's convention
mod tests {
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
        headers
            .insert("accept", HeaderValue::from_static("text/csv;q=0.9, application/json;q=0.8"));
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
}
