//! HTTP metrics endpoint for Prometheus scraping.
//!
//! Exposes metrics in Prometheus text format at `/metrics` endpoint.
//! This module provides HTTP server integration for metric collection.

use super::ObserverMetrics;
use prometheus::{Encoder, TextEncoder};

/// Configuration for the metrics HTTP endpoint.
#[derive(Debug, Clone)]
pub struct MetricsHttpConfig {
    /// Host to bind to (e.g., "127.0.0.1")
    pub host: String,
    /// Port to bind to (e.g., 9090)
    pub port: u16,
}

impl Default for MetricsHttpConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 9090,
        }
    }
}

/// Format metrics in Prometheus text format.
///
/// # Arguments
///
/// * `metrics` - The metrics instance to format
///
/// # Errors
///
/// Returns error if metrics cannot be encoded.
///
/// # Example
///
/// ```no_run
/// let formatted = format_metrics(&metrics)?;
/// println!("{}", formatted);
/// ```
pub fn format_metrics(metrics: &ObserverMetrics) -> Result<String, prometheus::Error> {
    let encoder = TextEncoder::new();
    let metric_families = metrics.registry.gather();
    let mut buffer = Vec::new();
    encoder.encode(&metric_families, &mut buffer)?;
    Ok(String::from_utf8_lossy(&buffer).to_string())
}

/// Create a metrics HTTP handler for use with axum.
///
/// This function returns a handler that can be mounted on an axum router.
/// The handler formats metrics in Prometheus text format.
///
/// # Example
///
/// ```no_run
/// use std::sync::Arc;
/// let metrics = Arc::new(ObserverMetrics::new(&registry)?);
/// let formatted = format_metrics(&metrics)?;
/// ```
#[must_use]
pub fn metrics_response(metrics: &ObserverMetrics) -> String {
    format_metrics(metrics).unwrap_or_else(|_| "# Failed to encode metrics\n".to_string())
}
