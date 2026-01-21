//! Request tracing and logging initialization.

use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter, Layer};

use crate::config::tracing::TracingConfig;
use fraiseql_error::RuntimeError;

/// Initialize tracing and logging
///
/// # Errors
///
/// Returns an error if tracing initialization fails
pub fn init_tracing(config: &TracingConfig) -> Result<(), RuntimeError> {
    // Create filter from RUST_LOG or config
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(&config.level));

    // Create fmt layer based on format
    let fmt_layer = if config.format == "json" {
        tracing_subscriber::fmt::layer()
            .json()
            .with_target(true)
            .with_thread_ids(true)
            .with_file(true)
            .with_line_number(true)
            .boxed()
    } else {
        tracing_subscriber::fmt::layer()
            .with_target(true)
            .with_thread_ids(true)
            .boxed()
    };

    // Initialize subscriber
    tracing_subscriber::registry()
        .with(filter)
        .with(fmt_layer)
        .init();

    Ok(())
}

/// Middleware to add request ID to tracing spans
pub async fn request_tracing_middleware(
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    // Get or generate request ID
    let request_id = req
        .headers()
        .get("X-Request-ID")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown");

    // Create span for this request
    let span = tracing::info_span!(
        "http_request",
        method = %req.method(),
        uri = %req.uri(),
        request_id = %request_id,
    );

    // Execute request within span
    let response = {
        let _enter = span.enter();
        next.run(req).await
    };

    response
}
