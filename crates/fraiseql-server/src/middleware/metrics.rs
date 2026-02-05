//! HTTP metrics middleware.
//!
//! Tracks HTTP request counts and response status codes.

use std::sync::{Arc, atomic::Ordering};

use axum::{body::Body, extract::State, http::Request, middleware::Next, response::Response};

use crate::metrics_server::MetricsCollector;

/// Metrics middleware that tracks HTTP requests and response status codes.
///
/// # Metrics Tracked
///
/// - `http_requests_total`: Total number of HTTP requests
/// - `http_responses_2xx`: Successful responses (200-299)
/// - `http_responses_4xx`: Client errors (400-499)
/// - `http_responses_5xx`: Server errors (500-599)
///
/// # Example
///
/// ```rust,ignore
/// use axum::{Router, middleware};
/// use fraiseql_server::middleware::metrics_middleware;
///
/// let app = Router::new()
///     .route("/", get(handler))
///     .layer(middleware::from_fn_with_state(metrics.clone(), metrics_middleware));
/// ```
pub async fn metrics_middleware(
    State(metrics): State<Arc<MetricsCollector>>,
    request: Request<Body>,
    next: Next,
) -> Response {
    // Increment total requests counter
    metrics.http_requests_total.fetch_add(1, Ordering::Relaxed);

    // Execute the request
    let response = next.run(request).await;

    // Record response status
    let status = response.status();
    match status.as_u16() {
        200..=299 => {
            metrics.http_responses_2xx.fetch_add(1, Ordering::Relaxed);
        },
        400..=499 => {
            metrics.http_responses_4xx.fetch_add(1, Ordering::Relaxed);
        },
        500..=599 => {
            metrics.http_responses_5xx.fetch_add(1, Ordering::Relaxed);
        },
        _ => {
            // Other status codes (1xx, 3xx) - not tracked separately
        },
    }

    response
}

#[cfg(test)]
mod tests {
    use axum::{
        Router,
        body::Body,
        http::{Request, StatusCode},
        middleware,
        routing::get,
    };
    use tower::ServiceExt;

    use super::*;

    async fn ok_handler() -> StatusCode {
        StatusCode::OK
    }

    async fn bad_request_handler() -> StatusCode {
        StatusCode::BAD_REQUEST
    }

    async fn internal_error_handler() -> StatusCode {
        StatusCode::INTERNAL_SERVER_ERROR
    }

    #[tokio::test]
    async fn test_metrics_middleware_counts_requests() {
        let metrics = Arc::new(MetricsCollector::new());

        let app = Router::new()
            .route("/ok", get(ok_handler))
            .layer(middleware::from_fn_with_state(metrics.clone(), metrics_middleware));

        let request = Request::builder().uri("/ok").body(Body::empty()).unwrap();

        let _response = app.oneshot(request).await.unwrap();

        assert_eq!(metrics.http_requests_total.load(Ordering::Relaxed), 1);
        assert_eq!(metrics.http_responses_2xx.load(Ordering::Relaxed), 1);
    }

    #[tokio::test]
    async fn test_metrics_middleware_tracks_4xx() {
        let metrics = Arc::new(MetricsCollector::new());

        let app = Router::new()
            .route("/bad", get(bad_request_handler))
            .layer(middleware::from_fn_with_state(metrics.clone(), metrics_middleware));

        let request = Request::builder().uri("/bad").body(Body::empty()).unwrap();

        let _response = app.oneshot(request).await.unwrap();

        assert_eq!(metrics.http_requests_total.load(Ordering::Relaxed), 1);
        assert_eq!(metrics.http_responses_4xx.load(Ordering::Relaxed), 1);
        assert_eq!(metrics.http_responses_2xx.load(Ordering::Relaxed), 0);
    }

    #[tokio::test]
    async fn test_metrics_middleware_tracks_5xx() {
        let metrics = Arc::new(MetricsCollector::new());

        let app = Router::new()
            .route("/error", get(internal_error_handler))
            .layer(middleware::from_fn_with_state(metrics.clone(), metrics_middleware));

        let request = Request::builder().uri("/error").body(Body::empty()).unwrap();

        let _response = app.oneshot(request).await.unwrap();

        assert_eq!(metrics.http_requests_total.load(Ordering::Relaxed), 1);
        assert_eq!(metrics.http_responses_5xx.load(Ordering::Relaxed), 1);
    }
}
