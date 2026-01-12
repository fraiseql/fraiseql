//! Request tracing middleware.

use tower_http::trace::{DefaultMakeSpan, DefaultOnResponse, TraceLayer};
use tracing::Level;

/// Create tracing layer.
///
/// Adds request/response tracing for observability.
///
/// # Features
///
/// - Logs incoming requests
/// - Logs response status and latency
/// - Adds trace IDs for request correlation
#[must_use]
pub fn trace_layer() -> TraceLayer<tower_http::classify::SharedClassifier<tower_http::classify::ServerErrorsAsFailures>> {
    TraceLayer::new_for_http()
        .make_span_with(DefaultMakeSpan::new().level(Level::INFO))
        .on_response(DefaultOnResponse::new().level(Level::INFO))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trace_layer_creation() {
        let _layer = trace_layer();
        // Basic test to ensure layer can be created
    }
}
