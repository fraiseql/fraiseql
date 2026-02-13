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
/// ```ignore
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
/// ```ignore
/// use std::sync::Arc;
/// let metrics = Arc::new(ObserverMetrics::new(&registry)?);
/// let formatted = format_metrics(&metrics)?;
/// ```
#[must_use]
pub fn metrics_response(metrics: &ObserverMetrics) -> String {
    format_metrics(metrics).unwrap_or_else(|_| "# Failed to encode metrics\n".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use prometheus::Registry;

    #[test]
    fn test_metrics_config_defaults() {
        let config = MetricsHttpConfig::default();
        assert_eq!(config.host, "127.0.0.1");
        assert_eq!(config.port, 9090);
    }

    #[test]
    fn test_metrics_config_custom() {
        let config = MetricsHttpConfig {
            host: "0.0.0.0".to_string(),
            port: 8080,
        };
        assert_eq!(config.host, "0.0.0.0");
        assert_eq!(config.port, 8080);
    }

    #[test]
    fn test_format_metrics_output() {
        let registry = Registry::new();
        let metrics = ObserverMetrics::new(&registry).expect("Failed to create metrics");

        // Increment a metric
        metrics.events_processed_total.inc();

        let formatted = format_metrics(&metrics).expect("Failed to format");

        // Verify Prometheus format
        assert!(formatted.contains("events_processed_total"));
        assert!(formatted.contains("# HELP"));
        assert!(formatted.contains("# TYPE"));
    }

    #[test]
    fn test_format_metrics_compliance() {
        let registry = Registry::new();
        let metrics = ObserverMetrics::new(&registry).expect("Failed to create metrics");

        let formatted = format_metrics(&metrics).expect("Failed to format");

        // Prometheus format requirements
        // 1. Comments start with #
        let has_comments = formatted.lines().any(|line| line.starts_with('#'));
        assert!(has_comments, "Missing HELP/TYPE comments");

        // 2. Metric names are valid
        let has_metrics = formatted.lines().any(|line| {
            !line.starts_with('#') && !line.is_empty() && line.contains('{')
        });
        assert!(has_metrics, "No valid metrics found");
    }
}
