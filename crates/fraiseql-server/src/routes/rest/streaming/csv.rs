//! CSV streaming response handler for the REST transport.
//!
//! When a client sends `Accept: text/csv`, the GET handler delegates to this
//! module. Like NDJSON, CSV is streamed in `O(batch_size)` memory rather than
//! buffering the full result set.
//!
//! Cycle 2 (this code): wires `Accept: text/csv` content negotiation and
//! produces a response with the correct headers (`content-type: text/csv`,
//! `content-disposition: attachment; filename="<query>.csv"`). The body is
//! intentionally empty — Cycle 3 will replace it with the real serializer.
//!
//! Gated behind the `export-csv` Cargo feature.

use axum::http::{HeaderMap, HeaderValue};
use bytes::Bytes;
use fraiseql_core::{db::traits::DatabaseAdapter, security::SecurityContext};
use futures::stream;

use super::super::{
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
/// **Cycle 2 stub**: route resolution and request validation are wired, but
/// the body is empty. Cycle 3 will fill in the `csv::Writer`-backed
/// serializer. The headers returned now (`content-type: text/csv`,
/// `content-disposition: attachment; ...`) are final.
///
/// # Errors
///
/// Returns `RestError` on route resolution, parameter extraction, or initial
/// query setup failure.
pub async fn handle_csv_get<A: DatabaseAdapter + 'static>(
    handler: &RestHandler<'_, A>,
    relative_path: &str,
    query_pairs: &[(&str, &str)],
    headers: &HeaderMap,
    security_context: Option<&SecurityContext>,
) -> Result<CsvResponse, RestError> {
    let resolved = handler.resolve_get_query(relative_path, query_pairs, security_context)?;

    let prefer = PreferHeader::from_headers(headers);
    validate_csv_request(&prefer, &resolved.params.pagination)?;

    let ResolvedGetQuery { query_name, .. } = resolved;

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

    // Cycle 2: empty body stub. Cycle 3 will plug in the serializer.
    let empty_stream = stream::empty::<Result<Bytes, std::convert::Infallible>>();

    Ok(CsvResponse {
        headers: response_headers,
        body:    CsvBody::Stream(Box::pin(empty_stream)),
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
}
