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

use axum::http::{HeaderMap, HeaderValue, StatusCode};
use bytes::Bytes;
use fraiseql_core::{
    db::traits::DatabaseAdapter, runtime::JsonRowStream, security::SecurityContext,
};
use futures::StreamExt as _;
use rust_xlsxwriter::Workbook;
use tempfile::NamedTempFile;

use super::{
    super::{
        export_config::ExportConfig,
        handler::{ResolvedGetQuery, RestError, RestHandler, set_request_id},
    },
    helpers::determine_columns,
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
        params.requested_pagination.export_total(),
    )
    .await?;

    let bytes = build_workbook(BuildContext {
        rows,
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
struct BuildContext {
    /// The export's rows, from its single statement (#958).
    rows:           JsonRowStream,
    max_rows:       u64,
    /// The projection's columns, from `helpers::export_columns` (#1274).
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
async fn build_workbook(ctx: BuildContext) -> Result<Bytes, RestError> {
    let temp_file = create_temp_file(ctx.temp_dir.as_deref())?;

    let mut workbook = Workbook::new();
    let worksheet = workbook.add_worksheet_with_constant_memory();

    let mut columns: Option<Vec<String>> = None;
    let mut rows_written: u64 = 0;

    let mut rows = ctx.rows;
    while let Some(row) = rows.next().await {
        let row = row.map_err(|e| RestError::internal(format!("XLSX export failed: {e}")))?;

        // Written on the first row rather than before the loop: the header is known up
        // front now (it is the projection), but the fallback for an empty projection
        // still needs a row to read its keys from, and an export with no rows must not
        // produce a sheet at all. Exactly as the CSV writer does.
        if columns.is_none() {
            let cols = determine_columns(ctx.select_columns.as_deref(), std::slice::from_ref(&row));
            write_header_row(worksheet, &cols)?;
            columns = Some(cols);
        }
        let active_columns = columns.as_ref().expect("columns initialised on the first row above");

        if rows_written >= ctx.max_rows {
            return Err(too_many_rows_error(ctx.max_rows));
        }
        let row_idx = u32::try_from(rows_written + 1)
            .map_err(|_| RestError::internal("XLSX row index overflow"))?;
        write_data_row(worksheet, row_idx, active_columns, &row)?;
        rows_written += 1;
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
    // String cells in XLSX are stored as `<inlineStr><t>...</t>` which
    // Excel treats as a literal value (no formula evaluation).  We still
    // apply the formula-injection guard as belt-and-suspenders against:
    //
    // 1. CSV-import paths that paste an XLSX cell into a different sheet, where the leading `=`
    //    *would* be parsed as a formula.
    // 2. Older Excel / LibreOffice versions that occasionally promote string cells starting with a
    //    sentinel to formula cells on save.
    // 3. Downstream tooling that round-trips through CSV (a common export chain for analytics
    //    pipelines).
    //
    // See `super::guard_formula_injection` for the threat model.
    use super::guard_formula_injection;
    match value {
        serde_json::Value::Null => Ok(()),
        serde_json::Value::Bool(b) => worksheet.write_boolean(row, col, *b).map(|_| ()),
        serde_json::Value::Number(n) => match n.as_f64() {
            Some(f) => worksheet.write_number(row, col, f).map(|_| ()),
            // Integers above f64 range fall back to a string cell so we
            // don't silently lose precision (Excel itself can't represent
            // 64-bit integers as numbers).
            None => worksheet
                .write_string(row, col, truncate_for_xlsx(&guard_formula_injection(&n.to_string())))
                .map(|_| ()),
        },
        serde_json::Value::String(s) => worksheet
            .write_string(row, col, truncate_for_xlsx(&guard_formula_injection(s)))
            .map(|_| ()),
        other => worksheet
            .write_string(
                row,
                col,
                truncate_for_xlsx(&guard_formula_injection(
                    &serde_json::to_string(other).unwrap_or_default(),
                )),
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

#[cfg(test)]
mod tests;
