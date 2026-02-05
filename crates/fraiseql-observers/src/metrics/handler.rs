//! HTTP handler for Prometheus /metrics endpoint

use prometheus::{Encoder, TextEncoder};

use super::MetricsRegistry;

/// Axum/HTTP handler for GET /metrics
///
/// Returns metrics in Prometheus text format
pub async fn metrics_handler() -> ([(String, String); 1], String) {
    match MetricsRegistry::global() {
        Ok(_metrics) => {
            let encoder = TextEncoder::new();
            let metric_families = prometheus::gather();
            let mut buffer = vec![];

            if encoder.encode(&metric_families, &mut buffer).is_ok() {
                let metrics_text = String::from_utf8_lossy(&buffer).to_string();
                (
                    [("content-type".to_string(), "text/plain; version=0.0.4".to_string())],
                    metrics_text,
                )
            } else {
                (
                    [("content-type".to_string(), "text/plain".to_string())],
                    "Error encoding metrics".to_string(),
                )
            }
        },
        Err(e) => (
            [("content-type".to_string(), "text/plain".to_string())],
            format!("Error initializing metrics: {e}"),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_metrics_handler_returns_text() {
        let (headers, body) = metrics_handler().await;
        assert_eq!(headers[0].0, "content-type", "Should return content-type header");
        assert!(body.contains("fraiseql_observer"), "Should contain observer metrics");
    }
}
