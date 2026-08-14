//! Helper functions for NDJSON streaming responses.
//!
//! Contains utilities for batch serialization, error formatting, and row extraction.

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
