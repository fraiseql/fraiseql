//! Metrics collection for operational monitoring.
//!
//! Provides Prometheus-compatible metrics using lock-free atomic counters.

use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

use serde::{Deserialize, Serialize};

/// Metrics collector backed by atomic counters.
///
/// All fields use [`AtomicU64`] instead of `Mutex<u64>` to eliminate
/// lock contention on the hot `record_request` path. Reads use
/// `Ordering::Relaxed` (acceptable for approximate monitoring metrics);
/// the fetch-add uses `Ordering::Relaxed` for the same reason — counters
/// never need to synchronise happens-before relationships with other memory.
#[derive(Clone)]
pub struct MetricsCollector {
    request_count:     Arc<AtomicU64>,
    error_count:       Arc<AtomicU64>,
    total_duration_ms: Arc<AtomicU64>,
}

/// Metrics summary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsSummary {
    /// Total requests processed.
    pub request_count:    u64,
    /// Total errors.
    pub error_count:      u64,
    /// Average response time in milliseconds.
    pub avg_duration_ms:  f64,
    /// Prometheus-format output lines.
    pub prometheus_lines: Vec<String>,
}

impl MetricsCollector {
    /// Create a new zeroed metrics collector.
    pub fn new() -> Self {
        Self {
            request_count:     Arc::new(AtomicU64::new(0)),
            error_count:       Arc::new(AtomicU64::new(0)),
            total_duration_ms: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Record a completed request.
    ///
    /// This is the hot path; all operations are a single atomic fetch-add each.
    pub fn record_request(&self, duration_ms: u32, is_error: bool) {
        self.request_count.fetch_add(1, Ordering::Relaxed);
        if is_error {
            self.error_count.fetch_add(1, Ordering::Relaxed);
        }
        self.total_duration_ms
            .fetch_add(u64::from(duration_ms), Ordering::Relaxed);
    }

    /// Snapshot current metrics.
    ///
    /// Loads are `Ordering::Relaxed`; slight inconsistency between the three
    /// reads is acceptable for observability metrics.
    pub fn summary(&self) -> MetricsSummary {
        let request_count = self.request_count.load(Ordering::Relaxed);
        let error_count = self.error_count.load(Ordering::Relaxed);
        let total_duration = self.total_duration_ms.load(Ordering::Relaxed);

        let avg_duration_ms = if request_count > 0 {
            total_duration as f64 / request_count as f64
        } else {
            0.0
        };

        let prometheus_lines = vec![
            "# HELP graphql_requests_total Total GraphQL requests".to_string(),
            "# TYPE graphql_requests_total counter".to_string(),
            format!("graphql_requests_total {request_count}"),
            "# HELP graphql_errors_total Total GraphQL errors".to_string(),
            "# TYPE graphql_errors_total counter".to_string(),
            format!("graphql_errors_total {error_count}"),
            "# HELP graphql_duration_ms Average duration".to_string(),
            "# TYPE graphql_duration_ms gauge".to_string(),
            format!("graphql_duration_ms {avg_duration_ms}"),
        ];

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

/// Format metrics for Prometheus export.
pub fn metrics_summary(collector: &MetricsCollector) -> String {
    let summary = collector.summary();
    summary.prometheus_lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_initial_summary_is_zero() {
        let m = MetricsCollector::new();
        let s = m.summary();
        assert_eq!(s.request_count, 0);
        assert_eq!(s.error_count, 0);
        assert_eq!(s.avg_duration_ms, 0.0);
    }

    #[test]
    fn test_record_successful_request() {
        let m = MetricsCollector::new();
        m.record_request(42, false);
        let s = m.summary();
        assert_eq!(s.request_count, 1);
        assert_eq!(s.error_count, 0);
        assert!((s.avg_duration_ms - 42.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_record_error_request() {
        let m = MetricsCollector::new();
        m.record_request(10, true);
        let s = m.summary();
        assert_eq!(s.request_count, 1);
        assert_eq!(s.error_count, 1);
    }

    #[test]
    fn test_average_computed_correctly() {
        let m = MetricsCollector::new();
        m.record_request(100, false);
        m.record_request(200, false);
        let s = m.summary();
        assert!((s.avg_duration_ms - 150.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_clone_shares_counters() {
        let m1 = MetricsCollector::new();
        let m2 = m1.clone();
        m1.record_request(50, false);
        m2.record_request(50, true);
        // Both m1 and m2 point to the same Arc<AtomicU64>
        let s = m1.summary();
        assert_eq!(s.request_count, 2);
        assert_eq!(s.error_count, 1);
    }

    #[test]
    fn test_prometheus_lines_present() {
        let m = MetricsCollector::new();
        m.record_request(20, false);
        let s = m.summary();
        assert!(
            s.prometheus_lines
                .iter()
                .any(|l| l.starts_with("graphql_requests_total"))
        );
    }

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
