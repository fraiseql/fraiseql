//! HTTP header count and size limit middleware.
//!
//! Rejects requests that exceed configured header count or total header byte
//! limits, preventing header-flooding `DoS` attacks that exhaust memory.

use axum::{
    body::Body,
    extract::Request,
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
};
use tracing::warn;

/// Middleware that enforces header count and total header byte size limits.
///
/// Returns 431 Request Header Fields Too Large when either limit is exceeded.
pub async fn header_limits_middleware(
    request: Request<Body>,
    next: Next,
    max_header_count: usize,
    max_header_bytes: usize,
) -> Response {
    let headers = request.headers();
    let header_count = headers.len();

    if header_count > max_header_count {
        warn!(header_count, max_header_count, "Request rejected: too many headers");
        return (StatusCode::REQUEST_HEADER_FIELDS_TOO_LARGE, "Too many request headers")
            .into_response();
    }

    let total_bytes: usize =
        headers.iter().map(|(name, value)| name.as_str().len() + value.len()).sum();

    if total_bytes > max_header_bytes {
        warn!(total_bytes, max_header_bytes, "Request rejected: headers too large");
        return (StatusCode::REQUEST_HEADER_FIELDS_TOO_LARGE, "Request headers too large")
            .into_response();
    }

    next.run(request).await
}

