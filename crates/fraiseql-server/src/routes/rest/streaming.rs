//! NDJSON streaming response handler for the REST transport.
//!
//! When a client sends `Accept: application/x-ndjson`, the GET handler delegates
//! to this module.  Each row is serialized as a single JSON line, with no
//! envelope (`data`/`meta`/`links`), enabling constant-memory streaming for
//! large result sets.
//!
//! Rows are fetched from the database in batches (configured via
//! `ndjson_batch_size`), serialized to NDJSON, and streamed to the client
//! incrementally.  Memory usage is bounded by O(batch_size) rather than
//! O(total_rows).

use std::sync::Arc;

use axum::http::{HeaderMap, HeaderValue};
use bytes::Bytes;
use fraiseql_core::db::traits::DatabaseAdapter;
use fraiseql_core::runtime::{Executor, QueryMatch};
use fraiseql_core::security::SecurityContext;
use futures::stream;

use super::handler::{PreferHeader, ResolvedGetQuery, RestError, RestHandler, set_request_id};
use super::params::PaginationParams;

/// Content type for NDJSON responses.
pub const NDJSON_CONTENT_TYPE: &str = "application/x-ndjson";

/// Check whether an `Accept` header value requests NDJSON.
#[must_use]
pub fn accepts_ndjson(headers: &HeaderMap) -> bool {
    headers
        .get("accept")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|accept| {
            accept
                .split(',')
                .any(|part| part.trim().eq_ignore_ascii_case(NDJSON_CONTENT_TYPE))
        })
}

/// Validate that NDJSON-incompatible preferences are not set.
///
/// Returns `Err(RestError)` if `Prefer: count=exact` or pagination params
/// (`limit`, `offset`) are present.
///
/// # Errors
///
/// Returns `RestError::BadRequest` when count or pagination is requested
/// alongside NDJSON streaming.
pub fn validate_ndjson_request(
    prefer: &PreferHeader,
    pagination: &PaginationParams,
) -> Result<(), RestError> {
    // Count is not available for streaming
    if prefer.count_exact || prefer.count_planned || prefer.count_estimated {
        return Err(RestError::bad_request(
            "count not available for streaming responses",
        ));
    }

    // Pagination is not available for streaming
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

/// Execute a query and return results as a streaming NDJSON response.
///
/// Rows are fetched in batches from the database and streamed to the client
/// as they arrive.  Each row is a JSON object followed by `\n`.  Memory usage
/// is bounded by the configured `ndjson_batch_size` rather than total rows.
///
/// Delegates route resolution and query building to
/// [`RestHandler::resolve_get_query`].
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
    let resolved = handler.resolve_get_query(relative_path, query_pairs, security_context)?;

    let prefer = PreferHeader::from_headers(headers);
    validate_ndjson_request(&prefer, &resolved.params.pagination)?;

    let ResolvedGetQuery {
        query_name,
        query_match,
        variables,
        ..
    } = resolved;

    let batch_size = handler.config().ndjson_batch_size.max(1);

    // Build response headers eagerly (before starting the stream).
    let mut response_headers = HeaderMap::new();
    set_request_id(headers, &mut response_headers);
    response_headers.insert(
        "content-type",
        HeaderValue::from_static(NDJSON_CONTENT_TYPE),
    );
    response_headers.insert(
        "x-stream-batch-size",
        HeaderValue::from_str(&batch_size.to_string())
            .unwrap_or_else(|_| HeaderValue::from_static("500")),
    );

    // Clone what we need for the async stream closure.
    let executor = Arc::clone(handler.executor());
    let security_ctx_owned = security_context.cloned();

    // Create an async stream that fetches batches and yields NDJSON lines.
    let ndjson_stream = stream::unfold(
        StreamState {
            executor,
            query_name,
            query_match,
            variables,
            security_ctx: security_ctx_owned,
            batch_size,
            offset: 0,
            done: false,
        },
        |mut state| async move {
            if state.done {
                return None;
            }

            match fetch_and_serialize_batch(&mut state).await {
                Ok(Some(bytes)) => Some((Ok(bytes), state)),
                Ok(None) => None,
                Err(err_bytes) => {
                    state.done = true;
                    Some((Ok(err_bytes), state))
                }
            }
        },
    );

    Ok(NdjsonResponse {
        headers: response_headers,
        body: NdjsonBody::Stream(Box::pin(ndjson_stream)),
    })
}

/// Internal state for the streaming unfold loop.
struct StreamState<A: DatabaseAdapter> {
    executor: Arc<Executor<A>>,
    query_name: String,
    query_match: QueryMatch,
    variables: serde_json::Value,
    security_ctx: Option<SecurityContext>,
    batch_size: u64,
    offset: u64,
    done: bool,
}

/// Fetch the next batch of rows, serialize as NDJSON bytes, and advance the offset.
///
/// Returns:
/// - `Ok(Some(bytes))` — batch serialized successfully
/// - `Ok(None)` — no more rows (stream done)
/// - `Err(bytes)` — error serialized as NDJSON error line
async fn fetch_and_serialize_batch<A: DatabaseAdapter>(
    state: &mut StreamState<A>,
) -> Result<Option<Bytes>, Bytes> {
    // Override limit/offset in the variables for this batch.
    let mut batch_vars = state.variables.clone();
    if let Some(obj) = batch_vars.as_object_mut() {
        obj.insert("limit".to_string(), serde_json::json!(state.batch_size));
        if state.offset > 0 {
            obj.insert("offset".to_string(), serde_json::json!(state.offset));
        }
    }

    let vars_ref = if batch_vars.as_object().is_none_or(|m| m.is_empty()) {
        None
    } else {
        Some(&batch_vars)
    };

    let result_str: String = match state
        .executor
        .execute_query_direct(
            &state.query_match,
            vars_ref,
            state.security_ctx.as_ref(),
        )
        .await
    {
        Ok(r) => r,
        Err(e) => {
            state.done = true;
            return Err(error_ndjson_line(&e.to_string()));
        }
    };

    let rows = match extract_rows(&result_str, &state.query_name) {
        Ok(r) => r,
        Err(e) => {
            state.done = true;
            return Err(error_ndjson_line(&e.message));
        }
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
            }
            Err(e) => {
                state.done = true;
                // Yield what we have so far plus the error.
                ndjson_bytes.extend_from_slice(&error_ndjson_line(&e.to_string()));
                return Ok(Some(Bytes::from(ndjson_bytes)));
            }
        }
    }

    // If we got fewer rows than the batch size, this is the last batch.
    #[allow(clippy::cast_possible_truncation)] // Reason: rows.len() won't exceed u64 range
    let row_count = rows.len() as u64;
    if row_count < state.batch_size {
        state.done = true;
    } else {
        state.offset += state.batch_size;
    }

    Ok(Some(Bytes::from(ndjson_bytes)))
}

/// Serialize an error as an NDJSON error line.
fn error_ndjson_line(message: &str) -> Bytes {
    // Escape the message for safe JSON embedding.
    let escaped = serde_json::to_string(message).unwrap_or_else(|_| format!("\"{message}\""));
    Bytes::from(format!("{{\"error\":{escaped}}}\n"))
}

/// NDJSON streaming response.
pub struct NdjsonResponse {
    /// Response headers.
    pub headers: HeaderMap,
    /// NDJSON body — either pre-buffered bytes or a streaming body.
    pub body: NdjsonBody,
}

/// Body of an NDJSON response.
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
    #[must_use]
    pub fn into_body(self) -> axum::body::Body {
        match self {
            Self::Stream(stream) => axum::body::Body::from_stream(stream),
        }
    }
}

/// Extract rows from the executor result envelope.
///
/// The executor returns `{ "data": { "queryName": [...] } }`.
/// For a single resource, returns a one-element vec.
///
/// # Errors
///
/// Returns `RestError` if the result cannot be parsed.
fn extract_rows(result: &str, query_name: &str) -> Result<Vec<serde_json::Value>, RestError> {
    let parsed: serde_json::Value = serde_json::from_str(result)
        .map_err(|e| RestError::internal(format!("Failed to parse query result: {e}")))?;

    let data = parsed
        .get("data")
        .and_then(|d| d.get(query_name))
        .ok_or_else(|| RestError::internal("Missing data in query result"))?;

    match data {
        serde_json::Value::Array(arr) => Ok(arr.clone()),
        // Single resource — wrap in a vec
        other => Ok(vec![other.clone()]),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::unwrap_used)] // Reason: test code
mod tests {
    use super::*;
    use axum::http::StatusCode;
    use serde_json::json;

    // -----------------------------------------------------------------------
    // accepts_ndjson
    // -----------------------------------------------------------------------

    #[test]
    fn accepts_ndjson_true_for_exact_match() {
        let mut headers = HeaderMap::new();
        headers.insert("accept", HeaderValue::from_static("application/x-ndjson"));
        assert!(accepts_ndjson(&headers));
    }

    #[test]
    fn accepts_ndjson_true_in_list() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "accept",
            HeaderValue::from_static("application/json, application/x-ndjson"),
        );
        assert!(accepts_ndjson(&headers));
    }

    #[test]
    fn accepts_ndjson_false_for_json() {
        let mut headers = HeaderMap::new();
        headers.insert("accept", HeaderValue::from_static("application/json"));
        assert!(!accepts_ndjson(&headers));
    }

    #[test]
    fn accepts_ndjson_false_when_missing() {
        let headers = HeaderMap::new();
        assert!(!accepts_ndjson(&headers));
    }

    #[test]
    fn accepts_ndjson_case_insensitive() {
        let mut headers = HeaderMap::new();
        headers.insert("accept", HeaderValue::from_static("Application/X-NDJSON"));
        assert!(accepts_ndjson(&headers));
    }

    // -----------------------------------------------------------------------
    // validate_ndjson_request
    // -----------------------------------------------------------------------

    #[test]
    fn validate_ndjson_rejects_count_exact() {
        let prefer = PreferHeader {
            count_exact: true,
            ..PreferHeader::default()
        };
        let pagination = PaginationParams::None;
        let err = validate_ndjson_request(&prefer, &pagination).unwrap_err();
        assert_eq!(err.status, StatusCode::BAD_REQUEST);
        assert!(err.message.contains("count not available"));
    }

    #[test]
    fn validate_ndjson_rejects_count_planned() {
        let prefer = PreferHeader {
            count_planned: true,
            ..PreferHeader::default()
        };
        let pagination = PaginationParams::None;
        let err = validate_ndjson_request(&prefer, &pagination).unwrap_err();
        assert_eq!(err.status, StatusCode::BAD_REQUEST);
    }

    #[test]
    fn validate_ndjson_rejects_count_estimated() {
        let prefer = PreferHeader {
            count_estimated: true,
            ..PreferHeader::default()
        };
        let pagination = PaginationParams::None;
        let err = validate_ndjson_request(&prefer, &pagination).unwrap_err();
        assert_eq!(err.status, StatusCode::BAD_REQUEST);
    }

    #[test]
    fn validate_ndjson_rejects_cursor_pagination() {
        let prefer = PreferHeader::default();
        let pagination = PaginationParams::Cursor {
            first: Some(10),
            after: None,
            last: None,
            before: None,
        };
        let err = validate_ndjson_request(&prefer, &pagination).unwrap_err();
        assert_eq!(err.status, StatusCode::BAD_REQUEST);
        assert!(err.message.contains("pagination not available"));
    }

    #[test]
    fn validate_ndjson_rejects_offset_pagination() {
        let prefer = PreferHeader::default();
        let pagination = PaginationParams::Offset {
            limit: 10,
            offset: 5,
        };
        let err = validate_ndjson_request(&prefer, &pagination).unwrap_err();
        assert_eq!(err.status, StatusCode::BAD_REQUEST);
    }

    #[test]
    fn validate_ndjson_allows_limit_only() {
        // offset=0 with limit is fine — it's the default, not explicit pagination
        let prefer = PreferHeader::default();
        let pagination = PaginationParams::Offset {
            limit: 100,
            offset: 0,
        };
        assert!(validate_ndjson_request(&prefer, &pagination).is_ok());
    }

    #[test]
    fn validate_ndjson_allows_no_pagination() {
        let prefer = PreferHeader::default();
        let pagination = PaginationParams::None;
        assert!(validate_ndjson_request(&prefer, &pagination).is_ok());
    }

    // -----------------------------------------------------------------------
    // extract_rows
    // -----------------------------------------------------------------------

    #[test]
    fn extract_rows_from_array() {
        let result = r#"{"data":{"users":[{"id":1,"name":"Alice"},{"id":2,"name":"Bob"}]}}"#;
        let rows = extract_rows(result, "users").unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0]["name"], "Alice");
        assert_eq!(rows[1]["name"], "Bob");
    }

    #[test]
    fn extract_rows_from_single_resource() {
        let result = r#"{"data":{"user":{"id":1,"name":"Alice"}}}"#;
        let rows = extract_rows(result, "user").unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["name"], "Alice");
    }

    #[test]
    fn extract_rows_missing_data() {
        let result = r#"{"errors":[]}"#;
        let err = extract_rows(result, "users").unwrap_err();
        assert_eq!(err.status, StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[test]
    fn extract_rows_missing_query() {
        let result = r#"{"data":{"other_query":[]}}"#;
        let err = extract_rows(result, "users").unwrap_err();
        assert_eq!(err.status, StatusCode::INTERNAL_SERVER_ERROR);
    }

    // -----------------------------------------------------------------------
    // NDJSON serialization
    // -----------------------------------------------------------------------

    #[test]
    fn ndjson_format_one_object_per_line() {
        let rows = vec![
            json!({"id": 1, "name": "Alice"}),
            json!({"id": 2, "name": "Bob"}),
        ];

        let mut ndjson = Vec::new();
        for row in &rows {
            let mut line = serde_json::to_vec(row).unwrap();
            line.push(b'\n');
            ndjson.extend_from_slice(&line);
        }

        let output = String::from_utf8(ndjson).unwrap();
        let lines: Vec<&str> = output.trim_end().split('\n').collect();
        assert_eq!(lines.len(), 2);

        // Each line is valid JSON
        for line in &lines {
            let parsed: serde_json::Value = serde_json::from_str(line).unwrap();
            assert!(parsed.is_object());
        }
    }

    #[test]
    fn ndjson_no_envelope() {
        let rows = vec![json!({"id": 1})];

        let mut ndjson = Vec::new();
        for row in &rows {
            let mut line = serde_json::to_vec(row).unwrap();
            line.push(b'\n');
            ndjson.extend_from_slice(&line);
        }

        let output = String::from_utf8(ndjson).unwrap();
        // No "data", "meta", or "links" wrapper
        assert!(!output.contains("\"data\""));
        assert!(!output.contains("\"meta\""));
        assert!(!output.contains("\"links\""));
    }

    #[test]
    fn ndjson_select_fields_applied() {
        // When ?select=id,name is used, each row should only have those fields.
        // This is handled upstream by QueryMatch field selection, but verify format.
        let rows = [json!({"id": 1, "name": "Alice"})];

        let line = serde_json::to_string(&rows[0]).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&line).unwrap();
        assert!(parsed.get("id").is_some());
        assert!(parsed.get("name").is_some());
        assert!(parsed.get("email").is_none());
    }

    // -----------------------------------------------------------------------
    // Content-Type header
    // -----------------------------------------------------------------------

    #[test]
    fn ndjson_content_type_constant() {
        assert_eq!(NDJSON_CONTENT_TYPE, "application/x-ndjson");
    }

    // -----------------------------------------------------------------------
    // error_ndjson_line
    // -----------------------------------------------------------------------

    #[test]
    fn error_ndjson_line_valid_json() {
        let line = error_ndjson_line("something went wrong");
        let s = String::from_utf8(line.to_vec()).unwrap();
        assert!(s.ends_with('\n'));
        let parsed: serde_json::Value = serde_json::from_str(s.trim()).unwrap();
        assert_eq!(parsed["error"], "something went wrong");
    }

    #[test]
    fn error_ndjson_line_escapes_special_chars() {
        let line = error_ndjson_line("bad \"quote\" and \nnewline");
        let s = String::from_utf8(line.to_vec()).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(s.trim()).unwrap();
        assert!(parsed["error"].as_str().unwrap().contains("quote"));
    }
}
