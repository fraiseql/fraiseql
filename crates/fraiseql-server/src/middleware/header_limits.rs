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

#[cfg(test)]
mod tests {
    use axum::{Router, body::Body, middleware, routing::get};
    use http::Request;
    use tower::ServiceExt;

    use super::*;

    async fn ok_handler() -> &'static str {
        "ok"
    }

    fn test_app(max_count: usize, max_bytes: usize) -> Router {
        Router::new()
            .route("/", get(ok_handler))
            .layer(middleware::from_fn(move |req, next| {
                header_limits_middleware(req, next, max_count, max_bytes)
            }))
    }

    #[tokio::test]
    async fn accepts_request_within_limits() {
        let app = test_app(10, 4096);
        let req = Request::builder()
            .uri("/")
            .header("x-test", "value")
            .body(Body::empty())
            .expect("Reason: test request builder should not fail");

        let resp = app.oneshot(req).await.expect("Reason: oneshot should not fail in test");
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn rejects_too_many_headers() {
        let app = test_app(3, 65_536);
        let mut builder = Request::builder().uri("/");
        for i in 0..10 {
            builder = builder.header(format!("x-test-{i}"), "value");
        }
        let req = builder
            .body(Body::empty())
            .expect("Reason: test request builder should not fail");

        let resp = app.oneshot(req).await.expect("Reason: oneshot should not fail in test");
        assert_eq!(resp.status(), StatusCode::REQUEST_HEADER_FIELDS_TOO_LARGE);
    }

    #[tokio::test]
    async fn rejects_headers_too_large() {
        let app = test_app(100, 64); // 64-byte total limit
        let req = Request::builder()
            .uri("/")
            .header("x-large", "a]".repeat(100))
            .body(Body::empty())
            .expect("Reason: test request builder should not fail");

        let resp = app.oneshot(req).await.expect("Reason: oneshot should not fail in test");
        assert_eq!(resp.status(), StatusCode::REQUEST_HEADER_FIELDS_TOO_LARGE);
    }

    #[tokio::test]
    async fn accepts_at_exact_count_limit() {
        let app = test_app(5, 65_536);
        let mut builder = Request::builder().uri("/");
        // Add exactly 5 custom headers (host may be added automatically)
        for i in 0..5 {
            builder = builder.header(format!("x-h-{i}"), "v");
        }
        let req = builder
            .body(Body::empty())
            .expect("Reason: test request builder should not fail");

        let resp = app.oneshot(req).await.expect("Reason: oneshot should not fail in test");
        // With 5 custom headers, total is 5 which is at limit — should pass
        assert_eq!(resp.status(), StatusCode::OK);
    }
}
