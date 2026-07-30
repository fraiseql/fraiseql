//! Helper functions for NDJSON streaming responses.
//!
//! Contains utilities for batch serialization, error formatting, and row extraction.

use std::sync::Arc;

use bytes::Bytes;
use fraiseql_core::{
    db::traits::DatabaseAdapter,
    runtime::{Executor, QueryMatch},
    security::SecurityContext,
};

use crate::routes::rest::handler::RestError;

/// Internal state for the streaming unfold loop.
pub(super) struct StreamState<A: DatabaseAdapter> {
    pub executor:     Arc<Executor<A>>,
    pub query_name:   String,
    pub query_match:  QueryMatch,
    pub variables:    serde_json::Value,
    pub security_ctx: Option<SecurityContext>,
    pub batch_size:   u64,
    /// Client-requested cap on the **total** rows exported (`?limit=`), if any.
    pub total_limit:  Option<u64>,
    /// Rows emitted so far, across every batch.
    pub emitted:      u64,
    pub offset:       u64,
    pub done:         bool,
}

/// Point a `QueryMatch` at one page of an export.
///
/// Pagination **must** be written into `query_match.arguments`.
/// `Executor::execute_query_direct` reads `limit`/`offset` from `arguments` only — its
/// `variables` parameter feeds `enforce_authz` and nothing else.
///
/// All three export loops (NDJSON here, CSV, XLSX) used to advance pagination by
/// mutating a clone of `variables`, so every batch re-issued the identical first-page
/// query (`#811`). That produced two failure modes from one bug: truncation when the
/// first page was short, and an endless duplicate-emitting loop when it was full.
///
/// Routing all three through this one function is the point — the previous code was
/// three independent copies of the same mistake.
pub(super) fn set_export_page(query_match: &mut QueryMatch, limit: u64, offset: u64) {
    query_match.arguments.insert("limit".to_string(), serde_json::json!(limit));
    query_match.arguments.insert("offset".to_string(), serde_json::json!(offset));
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

/// How many rows the next batch may fetch, or `None` when the export is complete.
///
/// Clamps the final batch so a `?limit=` that is not a multiple of the batch size still
/// yields exactly `limit` rows.
pub(super) const fn next_batch_size(
    batch_size: u64,
    total_limit: Option<u64>,
    emitted: u64,
) -> Option<u64> {
    match total_limit {
        Some(total) if emitted >= total => None,
        Some(total) => {
            let remaining = total - emitted;
            Some(if remaining < batch_size {
                remaining
            } else {
                batch_size
            })
        },
        None => Some(batch_size),
    }
}

/// Fetch the next batch of rows, serialize as NDJSON bytes, and advance the offset.
///
/// Returns:
/// - `Ok(Some(bytes))` — batch serialized successfully
/// - `Ok(None)` — no more rows (stream done)
/// - `Err(bytes)` — error serialized as NDJSON error line
pub(super) async fn fetch_and_serialize_batch<A: DatabaseAdapter>(
    state: &mut StreamState<A>,
) -> Result<Option<Bytes>, Bytes> {
    // Size this batch, honouring any client-supplied total cap.
    let Some(page) = next_batch_size(state.batch_size, state.total_limit, state.emitted) else {
        state.done = true;
        return Ok(None);
    };
    // #811: pagination goes into `arguments`, which is what the executor reads.
    set_export_page(&mut state.query_match, page, state.offset);

    // Kept as an owned local rather than borrowed from `state`: the loop below mutates
    // `state`, and a live borrow of `state.variables` across the await would conflict.
    let batch_vars = state.variables.clone();
    let vars_ref = if batch_vars.as_object().is_none_or(|m| m.is_empty()) {
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
            return Err(error_ndjson_line(&e.to_string()));
        },
    };

    let rows = match extract_rows(&result_value, &state.query_name) {
        Ok(r) => r,
        Err(e) => {
            state.done = true;
            return Err(error_ndjson_line(&e.message));
        },
    };

    if rows.is_empty() {
        state.done = true;
        return Ok(None);
    }

    // Serialize rows as NDJSON.
    let mut ndjson_bytes = Vec::new();
    for row in &rows {
        match serde_json::to_vec(row) {
            Ok(mut line) => {
                line.push(b'\n');
                ndjson_bytes.extend_from_slice(&line);
            },
            Err(e) => {
                state.done = true;
                // Yield what we have so far plus the error.
                ndjson_bytes.extend_from_slice(&error_ndjson_line(&e.to_string()));
                return Ok(Some(Bytes::from(ndjson_bytes)));
            },
        }
    }

    // Advance by the rows actually returned, not by the requested page size: if the
    // database returned a short page the export is finished anyway, and advancing by
    // the request would skip rows on any path that can return fewer than it asked for.
    let row_count = u64::try_from(rows.len()).unwrap_or(u64::MAX);
    state.emitted += row_count;
    state.offset += row_count;

    // A short page means the result set is exhausted. Reaching the client's `?limit=`
    // is handled by `next_batch_size` returning `None` on the following call.
    if row_count < page {
        state.done = true;
    }

    Ok(Some(Bytes::from(ndjson_bytes)))
}

/// Serialize an error as an NDJSON error line.
pub(super) fn error_ndjson_line(message: &str) -> Bytes {
    // Escape the message for safe JSON embedding.
    let escaped = serde_json::to_string(message).unwrap_or_else(|_| format!("\"{message}\""));
    Bytes::from(format!("{{\"error\":{escaped}}}\n"))
}

/// Extract rows from the executor result envelope.
///
/// The executor returns `{ "data": { "queryName": [...] } }`.
/// For a single resource, returns a one-element vec.
///
/// # Errors
///
/// Returns `RestError` if the result cannot be parsed.
pub(super) fn extract_rows(
    result: &serde_json::Value,
    query_name: &str,
) -> Result<Vec<serde_json::Value>, RestError> {
    let data = result
        .get("data")
        .and_then(|d| d.get(query_name))
        .ok_or_else(|| RestError::internal("Missing data in query result"))?;

    match data {
        serde_json::Value::Array(arr) => Ok(arr.clone()),
        // Single resource — wrap in a vec
        other => Ok(vec![other.clone()]),
    }
}
