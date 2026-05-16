//! HTTP metrics middleware.
//!
//! Tracks HTTP request counts and response status codes.

use std::{
    sync::{Arc, atomic::Ordering},
    time::Instant,
};

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
/// ```text
/// // Requires: running Axum application with a MetricsCollector instance.
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

    // Execute the request with timing
    let start = Instant::now();
    let response = next.run(request).await;
    #[allow(clippy::cast_possible_truncation)]
    // Reason: microsecond counter cannot exceed u64 in any practical uptime
    let elapsed_us = start.elapsed().as_micros() as u64;
    metrics.http_request_duration.observe_us(elapsed_us);

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
