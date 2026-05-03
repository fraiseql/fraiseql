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

use std::sync::Arc;

use axum::http::{HeaderMap, HeaderValue};
use bytes::Bytes;
use fraiseql_core::{
    db::traits::DatabaseAdapter,
    runtime::{Executor, QueryMatch},
    security::SecurityContext,
};
use futures::stream;

use super::{
    handler::{PreferHeader, ResolvedGetQuery, RestError, RestHandler, set_request_id},
    params::PaginationParams,
};
use helpers::{fetch_and_serialize_batch, StreamState};

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
        return Err(RestError::bad_request("count not available for streaming responses"));
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
    response_headers.insert("content-type", HeaderValue::from_static(NDJSON_CONTENT_TYPE));
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
                },
            }
        },
    );

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
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::unwrap_used)] // Reason: test code
mod tests {
    use axum::http::StatusCode;

    use super::*;

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
        headers
            .insert("accept", HeaderValue::from_static("application/json, application/x-ndjson"));
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
            first:  Some(10),
            after:  None,
            last:   None,
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
            limit:  10,
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
            limit:  100,
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
    // Content-Type header
    // -----------------------------------------------------------------------

    #[test]
    fn ndjson_content_type_constant() {
        assert_eq!(NDJSON_CONTENT_TYPE, "application/x-ndjson");
    }
}
