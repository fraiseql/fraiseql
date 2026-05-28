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
//!   `Retry-After: 1` header.
//!
//! Gated behind the `export-xlsx` Cargo feature.

use std::sync::Arc;

use axum::http::{HeaderMap, HeaderValue};
use bytes::Bytes;
use fraiseql_core::{db::traits::DatabaseAdapter, security::SecurityContext};
use tokio::sync::Semaphore;

use super::super::{
    export_config::ExportConfig,
    handler::{PreferHeader, RestError, RestHandler, set_request_id},
    params::PaginationParams,
};

/// Content type for XLSX responses.
pub const XLSX_CONTENT_TYPE: &str =
    "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet";

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

/// Validate that XLSX-incompatible preferences are not set.
///
/// Same constraints as NDJSON / CSV: count and pagination are unavailable
/// because the workbook is built from the full filtered result set.
///
/// # Errors
///
/// Returns `RestError::BadRequest` when count or pagination is requested
/// alongside an XLSX export.
pub fn validate_xlsx_request(
    prefer: &PreferHeader,
    pagination: &PaginationParams,
) -> Result<(), RestError> {
    if prefer.count_exact || prefer.count_planned || prefer.count_estimated {
        return Err(RestError::bad_request("count not available for export responses"));
    }

    if let PaginationParams::Offset { offset, .. } = pagination {
        if *offset > 0 {
            return Err(RestError::bad_request(
                "pagination not available for export; use filters to narrow results",
            ));
        }
    }
    if matches!(pagination, PaginationParams::Cursor { .. }) {
        return Err(RestError::bad_request(
            "pagination not available for export; use filters to narrow results",
        ));
    }

    Ok(())
}

/// Execute a query and return an XLSX workbook as the response body.
///
/// Cycle 6a: stub implementation. Resolves the route, validates the request,
/// emits the correct `Content-Type` and `Content-Disposition` headers, and
/// returns an empty body. Real workbook building lands in Cycle 6b.
///
/// # Errors
///
/// Returns `RestError` on route resolution, parameter extraction, or initial
/// query setup failure.
pub async fn handle_xlsx_get<A: DatabaseAdapter + 'static>(
    handler: &RestHandler<'_, A>,
    _export_config: &ExportConfig,
    _xlsx_semaphore: &Arc<Semaphore>,
    relative_path: &str,
    query_pairs: &[(&str, &str)],
    headers: &HeaderMap,
    security_context: Option<&SecurityContext>,
) -> Result<XlsxResponse, RestError> {
    let resolved = handler.resolve_get_query(relative_path, query_pairs, security_context)?;

    let prefer = PreferHeader::from_headers(headers);
    validate_xlsx_request(&prefer, &resolved.params.pagination)?;

    let query_name = resolved.query_name;

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

    Ok(XlsxResponse {
        headers: response_headers,
        body:    XlsxBody::Bytes(Bytes::new()),
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
    /// Pre-buffered workbook bytes (in-memory or read from a temp file).
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

#[cfg(test)]
#[allow(clippy::unwrap_used)] // Reason: tests follow the CSV sibling module's convention.
mod tests {
    use axum::http::StatusCode;

    use super::*;

    #[test]
    fn xlsx_content_type_constant() {
        assert_eq!(
            XLSX_CONTENT_TYPE,
            "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"
        );
    }

    #[test]
    fn accepts_xlsx_true_for_exact_match() {
        let mut headers = HeaderMap::new();
        headers.insert("accept", HeaderValue::from_static(XLSX_CONTENT_TYPE));
        assert!(accepts_xlsx(&headers));
    }

    #[test]
    fn accepts_xlsx_true_in_list() {
        let mut headers = HeaderMap::new();
        let value = format!("application/json, {XLSX_CONTENT_TYPE}");
        headers.insert("accept", HeaderValue::from_str(&value).unwrap());
        assert!(accepts_xlsx(&headers));
    }

    #[test]
    fn accepts_xlsx_false_for_json() {
        let mut headers = HeaderMap::new();
        headers.insert("accept", HeaderValue::from_static("application/json"));
        assert!(!accepts_xlsx(&headers));
    }

    #[test]
    fn accepts_xlsx_false_for_csv() {
        let mut headers = HeaderMap::new();
        headers.insert("accept", HeaderValue::from_static("text/csv"));
        assert!(!accepts_xlsx(&headers));
    }

    #[test]
    fn accepts_xlsx_false_when_missing() {
        let headers = HeaderMap::new();
        assert!(!accepts_xlsx(&headers));
    }

    #[test]
    fn accepts_xlsx_case_insensitive() {
        let mut headers = HeaderMap::new();
        let upper = XLSX_CONTENT_TYPE.to_ascii_uppercase();
        headers.insert("accept", HeaderValue::from_str(&upper).unwrap());
        assert!(accepts_xlsx(&headers));
    }

    #[test]
    fn accepts_xlsx_ignores_quality_params() {
        let mut headers = HeaderMap::new();
        let value = format!("{XLSX_CONTENT_TYPE};q=0.9, application/json;q=0.8");
        headers.insert("accept", HeaderValue::from_str(&value).unwrap());
        assert!(accepts_xlsx(&headers));
    }

    #[test]
    fn accepts_xlsx_does_not_match_plain_xml() {
        let mut headers = HeaderMap::new();
        headers.insert("accept", HeaderValue::from_static("application/xml"));
        assert!(!accepts_xlsx(&headers));
    }

    #[test]
    fn validate_xlsx_rejects_count_exact() {
        let prefer = PreferHeader {
            count_exact: true,
            ..PreferHeader::default()
        };
        let err = validate_xlsx_request(&prefer, &PaginationParams::None).unwrap_err();
        assert_eq!(err.status, StatusCode::BAD_REQUEST);
        assert!(err.message.contains("count not available"));
    }

    #[test]
    fn validate_xlsx_rejects_count_planned() {
        let prefer = PreferHeader {
            count_planned: true,
            ..PreferHeader::default()
        };
        let err = validate_xlsx_request(&prefer, &PaginationParams::None).unwrap_err();
        assert_eq!(err.status, StatusCode::BAD_REQUEST);
    }

    #[test]
    fn validate_xlsx_rejects_count_estimated() {
        let prefer = PreferHeader {
            count_estimated: true,
            ..PreferHeader::default()
        };
        let err = validate_xlsx_request(&prefer, &PaginationParams::None).unwrap_err();
        assert_eq!(err.status, StatusCode::BAD_REQUEST);
    }

    #[test]
    fn validate_xlsx_rejects_cursor_pagination() {
        let prefer = PreferHeader::default();
        let pagination = PaginationParams::Cursor {
            first:  Some(10),
            after:  None,
            last:   None,
            before: None,
        };
        let err = validate_xlsx_request(&prefer, &pagination).unwrap_err();
        assert_eq!(err.status, StatusCode::BAD_REQUEST);
        assert!(err.message.contains("pagination not available"));
    }

    #[test]
    fn validate_xlsx_rejects_offset_pagination() {
        let prefer = PreferHeader::default();
        let pagination = PaginationParams::Offset {
            limit:  10,
            offset: 5,
        };
        let err = validate_xlsx_request(&prefer, &pagination).unwrap_err();
        assert_eq!(err.status, StatusCode::BAD_REQUEST);
    }

    #[test]
    fn validate_xlsx_allows_limit_only() {
        let prefer = PreferHeader::default();
        let pagination = PaginationParams::Offset {
            limit:  100,
            offset: 0,
        };
        assert!(validate_xlsx_request(&prefer, &pagination).is_ok());
    }

    #[test]
    fn validate_xlsx_allows_no_pagination() {
        let prefer = PreferHeader::default();
        assert!(validate_xlsx_request(&prefer, &PaginationParams::None).is_ok());
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
