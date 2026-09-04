//! Shared internals of the REST export representations.
//!
//! The row source every export opens (`export_rows`, #958), the export-total bound
//! (`requested_total_limit`), NDJSON batch serialisation and error formatting, and — since
//! #1274 — the header-column rule that CSV and XLSX both apply (`export_columns`,
//! `determine_columns`). The column rule lives here because both writers held
//! byte-identical copies of it, which is one place per writer for it to drift.

use std::sync::Arc;

use bytes::Bytes;
use fraiseql_core::{
    db::traits::DatabaseAdapter,
    runtime::{Executor, JsonRowStream, QueryMatch},
    security::SecurityContext,
};
use futures::StreamExt as _;

use crate::routes::rest::handler::RestError;

/// Open the row source behind every export representation (#958).
///
/// One statement over one portal, delivering rows as PostgreSQL produces them.
/// All three exports — NDJSON, CSV, XLSX — used to walk the same result set with
/// `LIMIT n OFFSET k` re-executions, which is `O(k)` per batch and gives each
/// batch its own snapshot: a concurrent insert or delete shifts rows across a
/// batch boundary, so an export silently emits one row twice and another not at
/// all. Nothing about a batch size fixes that; a single statement does.
///
/// # The two pagination arguments
///
/// `limit` is **removed** from the query's arguments rather than pushed into the
/// SQL. A client's `?limit=` bounds the export *total*, and `max_page_size` (#421)
/// bounds one page of an interactive read — pushing an export total through a page
/// guard would refuse every export larger than a page. The cap is applied to the
/// stream instead, which is the same bound with none of the confusion. `offset` is
/// left alone: it costs `O(offset)` once, not once per batch.
///
/// # Errors
///
/// Returns `RestError` when the read is refused before its first row —
/// authorization, a gated field, a missing principal for an RLS or tenant-scoped
/// query. A failure after that point cannot be an HTTP status any more (the
/// response has begun) and arrives as an `Err` item in the stream.
pub(super) async fn export_rows<A: DatabaseAdapter + 'static>(
    executor: &Arc<Executor<A>>,
    mut query_match: QueryMatch,
    variables: serde_json::Value,
    security_ctx: Option<SecurityContext>,
    total_limit: Option<u64>,
) -> Result<JsonRowStream, RestError> {
    query_match.arguments.remove("limit");

    let variables = if variables.as_object().is_none_or(serde_json::Map::is_empty) {
        None
    } else {
        Some(variables)
    };

    let rows = executor
        .stream_query_direct(query_match, variables, security_ctx)
        .await
        .map_err(RestError::from)?;

    Ok(match total_limit {
        Some(total) => Box::pin(rows.take(usize::try_from(total).unwrap_or(usize::MAX))),
        None => rows,
    })
}

/// The column list an export writes, taken from the **projection**.
///
/// This is the one place either export writer learns its header, and the answer is the
/// field list the rows are actually projected by — `QueryMatch::fields`, which
/// `resolve_get_query` builds from `params.field_selection` (expanding `All` to the
/// type's declared fields, per #886).
///
/// # Why not re-parse `?select=`
///
/// Both writers used to parse the raw `?select=` string a second time, and the two
/// parses disagreed (#1274). `RestParamExtractor::extract` classifies with an
/// assignment, so a repeated `?select=` resolves **last**-wins into the projection;
/// the header parser searched with `.find`, so it resolved **first**-wins. A request
/// naming two different fields therefore got a header for one and rows for the other,
/// and `write_csv_payload` renders a key the row lacks as an empty cell — a named
/// column, empty in every row, under a `200`.
///
/// A second parse of the same input is a second source of truth whichever way it
/// resolves; the projection is the only list the rows can be guaranteed to fill. It is
/// also what makes the paren-awareness the old parser carried unnecessary: since #1268
/// an export *refuses* a `?select=` naming an embed or a count, so no header can be
/// asked for one.
///
/// `None` means "no column list is known" — the projection is empty, which
/// `resolve_get_query` produces only when the return type is not in the schema. The
/// writers fall back to the first row's keys there, as they did before.
#[cfg(any(feature = "export-csv", feature = "export-xlsx"))]
pub(super) fn export_columns(query_match: &QueryMatch) -> Option<Vec<String>> {
    if query_match.fields.is_empty() {
        None
    } else {
        Some(query_match.fields.clone())
    }
}

/// Decide the column ordering an export writes.
///
/// Preference:
/// 1. The projection, via [`export_columns`].
/// 2. The first row's keys, sorted alphabetically.
///
/// The fallback sorts explicitly rather than leaning on `serde_json::Map` iteration
/// order: that order is alphabetical only for the default (`BTreeMap`) build and becomes
/// insertion order when any dependency enables the `preserve_order` feature (e.g. under
/// `--all-features`), which would silently change export column order. Sorting here keeps
/// the header deterministic regardless of `serde_json`'s feature resolution.
///
/// Shared by the CSV and XLSX writers, which held byte-identical copies of this and of
/// the `?select=` parser it used to take its first branch from (#1274). Two copies of a
/// header rule are two places for it to drift with no compiler signal.
#[cfg(any(feature = "export-csv", feature = "export-xlsx"))]
pub(super) fn determine_columns(
    select_columns: Option<&[String]>,
    rows: &[serde_json::Value],
) -> Vec<String> {
    if let Some(cols) = select_columns {
        return cols.to_vec();
    }
    rows.first()
        .and_then(serde_json::Value::as_object)
        .map(|m| {
            let mut cols: Vec<String> = m.keys().cloned().collect();
            cols.sort();
            cols
        })
        .unwrap_or_default()
}

/// The client's explicit `?limit=`, which bounds the export **total**.
///
/// Recovered from the raw query pairs because `parse_offset_pagination` collapses an
/// absent `?limit=` into `default_page_size`, making "the client asked for 100 rows" and
/// "the client asked for nothing" indistinguishable downstream. On a streaming export
/// those mean opposite things: the first is a cap, the second is "the whole table".
pub(super) fn requested_total_limit(query_pairs: &[(&str, &str)]) -> Option<u64> {
    query_pairs
        .iter()
        .find(|(k, _)| *k == "limit")
        .and_then(|(_, v)| v.parse().ok())
}

/// Serialise one group of streamed rows as NDJSON bytes.
///
/// Returns the bytes and whether the group ended in a failure, which is the
/// export's terminal condition: rows serialised before the failure still go out,
/// followed by the error line. A truncated export that says why is recoverable;
/// one that simply stops is indistinguishable from a complete one.
pub(super) fn ndjson_chunk<I>(rows: I) -> (Bytes, bool)
where
    I: IntoIterator<Item = fraiseql_core::error::Result<serde_json::Value>>,
{
    let mut bytes = Vec::new();
    for row in rows {
        match row
            .map_err(|e| e.to_string())
            .and_then(|row| serde_json::to_vec(&row).map_err(|e| e.to_string()))
        {
            Ok(mut line) => {
                line.push(b'\n');
                bytes.extend_from_slice(&line);
            },
            Err(message) => {
                bytes.extend_from_slice(&error_ndjson_line(&message));
                return (Bytes::from(bytes), true);
            },
        }
    }
    (Bytes::from(bytes), false)
}

/// Serialize an error as an NDJSON error line.
pub(super) fn error_ndjson_line(message: &str) -> Bytes {
    // Escape the message for safe JSON embedding.
    let escaped = serde_json::to_string(message).unwrap_or_else(|_| format!("\"{message}\""));
    Bytes::from(format!("{{\"error\":{escaped}}}\n"))
}
