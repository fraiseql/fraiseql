//! Metrics collection for operational monitoring
//!
//! Provides Prometheus-compatible metrics

use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};

/// Metrics collector
#[derive(Clone)]
pub struct MetricsCollector {
    request_count: Arc<Mutex<u64>>,
    error_count: Arc<Mutex<u64>>,
    total_duration_ms: Arc<Mutex<u64>>,
}

/// Metrics summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsSummary {
    /// Total requests processed
    pub request_count: u64,
    /// Total errors
    pub error_count: u64,
    /// Average response time
    pub avg_duration_ms: f64,
    /// Prometheus format output
    pub prometheus_lines: Vec<String>,
}

impl MetricsCollector {
    /// Create new metrics collector
    pub fn new() -> Self {
        Self {
            request_count: Arc::new(Mutex::new(0)),
            error_count: Arc::new(Mutex::new(0)),
            total_duration_ms: Arc::new(Mutex::new(0)),
        }
    }

    /// Record a request
    pub fn record_request(&self, duration_ms: u32, is_error: bool) {
        if let Ok(mut count) = self.request_count.lock() {
            *count += 1;
        }

        if is_error {
            if let Ok(mut errors) = self.error_count.lock() {
                *errors += 1;
            }
        }

        if let Ok(mut total) = self.total_duration_ms.lock() {
            *total += u64::from(duration_ms);
        }
    }

    /// Get summary
    pub fn summary(&self) -> MetricsSummary {
        let request_count = self.request_count.lock().map(|x| *x).unwrap_or(0);
        let error_count = self.error_count.lock().map(|x| *x).unwrap_or(0);
        let total_duration = self.total_duration_ms.lock().map(|x| *x).unwrap_or(0);

        let avg_duration_ms = if request_count > 0 {
            total_duration as f64 / request_count as f64
        } else {
            0.0
        };

        let mut prometheus_lines = Vec::new();
        prometheus_lines.push("# HELP graphql_requests_total Total GraphQL requests".to_string());
        prometheus_lines.push("# TYPE graphql_requests_total counter".to_string());
        prometheus_lines.push(format!("graphql_requests_total {{{}}}", request_count));

        prometheus_lines.push("# HELP graphql_errors_total Total GraphQL errors".to_string());
        prometheus_lines.push("# TYPE graphql_errors_total counter".to_string());
        prometheus_lines.push(format!("graphql_errors_total {{{}}}", error_count));

        prometheus_lines.push("# HELP graphql_duration_ms Average duration".to_string());
        prometheus_lines.push("# TYPE graphql_duration_ms gauge".to_string());
        prometheus_lines.push(format!("graphql_duration_ms {{{}}}", avg_duration_ms));

        MetricsSummary {
            request_count,
            error_count,
            avg_duration_ms,
            prometheus_lines,
        }
    }
}

impl Default for MetricsCollector {
    fn default() -> Self {
        Self::new()
    }
}

/// Format metrics for Prometheus export
pub fn metrics_summary(collector: &MetricsCollector) -> String {
    let summary = collector.summary();
    summary.prometheus_lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_collector() {
        let collector = MetricsCollector::new();
        collector.record_request(50, false);
        collector.record_request(100, false);

        let summary = collector.summary();
        assert_eq!(summary.request_count, 2);
        assert_eq!(summary.error_count, 0);
        assert!((summary.avg_duration_ms - 75.0).abs() < 0.1);
    }
}
