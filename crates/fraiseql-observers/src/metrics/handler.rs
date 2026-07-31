//! HTTP handler for Prometheus /metrics endpoint

use prometheus::{Encoder, TextEncoder};

use super::MetricsRegistry;

/// Render every metric in the `prometheus` default registry as Prometheus
/// text (#634).
///
/// The observer subsystem registers its series (`fraiseql_observer_*`) into the
/// `prometheus` crate's default registry, while `fraiseql-server` exposes
/// `/metrics` through the `metrics-exporter-prometheus` ecosystem — two
/// registries that never met, so observer metrics were silently absent from
/// the scrape even when compiled in. The server bridges by appending this
/// rendering to its own exporter output. Returns an error string description
/// on encoding failure so callers can decide loudness.
///
/// # Errors
///
/// Returns the `prometheus` encoder error when the gathered metric families
/// cannot be encoded (malformed registrations).
pub fn render_prometheus_text() -> Result<String, prometheus::Error> {
    // Ensure the registry (and its series) is initialised even if no event has
    // been processed yet — a scrape before the first event must still show the
    // observer series at zero rather than omitting them.
    let _ = MetricsRegistry::global();
    let encoder = TextEncoder::new();
    let metric_families = prometheus::gather();
    let mut buffer = vec![];
    encoder.encode(&metric_families, &mut buffer)?;
    Ok(String::from_utf8_lossy(&buffer).to_string())
}

/// Axum/HTTP handler for GET /metrics
///
/// Returns metrics in Prometheus text format
pub async fn metrics_handler() -> ([(String, String); 1], String) {
    match MetricsRegistry::global() {
        Ok(_metrics) => match render_prometheus_text() {
            Ok(metrics_text) => (
                [("content-type".to_string(), "text/plain; version=0.0.4".to_string())],
                metrics_text,
            ),
            Err(_) => (
                [("content-type".to_string(), "text/plain".to_string())],
                "Error encoding metrics".to_string(),
            ),
        },
        Err(e) => (
            [("content-type".to_string(), "text/plain".to_string())],
            format!("Error initializing metrics: {e}"),
        ),
    }
}
