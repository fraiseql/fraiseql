//! Prometheus metrics for observability.
//!
//! Tracks:
//! - GraphQL query execution time
//! - Query success/error rates
//! - Database query performance
//! - Connection pool statistics
//! - HTTP request/response metrics

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

/// Metrics collector for the server.
#[derive(Debug, Clone)]
pub struct MetricsCollector {
    /// Total GraphQL queries executed
    pub queries_total: Arc<AtomicU64>,

    /// Total successful queries
    pub queries_success: Arc<AtomicU64>,

    /// Total failed queries
    pub queries_error: Arc<AtomicU64>,

    /// Total query execution time (microseconds)
    pub queries_duration_us: Arc<AtomicU64>,

    /// Total database queries executed
    pub db_queries_total: Arc<AtomicU64>,

    /// Total database query time (microseconds)
    pub db_queries_duration_us: Arc<AtomicU64>,

    /// Total validation errors
    pub validation_errors_total: Arc<AtomicU64>,

    /// Total parse errors
    pub parse_errors_total: Arc<AtomicU64>,

    /// Total execution errors
    pub execution_errors_total: Arc<AtomicU64>,

    /// Total HTTP requests
    pub http_requests_total: Arc<AtomicU64>,

    /// Total HTTP 2xx responses
    pub http_responses_2xx: Arc<AtomicU64>,

    /// Total HTTP 4xx responses
    pub http_responses_4xx: Arc<AtomicU64>,

    /// Total HTTP 5xx responses
    pub http_responses_5xx: Arc<AtomicU64>,

    /// Cache hits
    pub cache_hits: Arc<AtomicU64>,

    /// Cache misses
    pub cache_misses: Arc<AtomicU64>,
}

impl MetricsCollector {
    /// Create new metrics collector.
    #[must_use]
    pub fn new() -> Self {
        Self {
            queries_total: Arc::new(AtomicU64::new(0)),
            queries_success: Arc::new(AtomicU64::new(0)),
            queries_error: Arc::new(AtomicU64::new(0)),
            queries_duration_us: Arc::new(AtomicU64::new(0)),
            db_queries_total: Arc::new(AtomicU64::new(0)),
            db_queries_duration_us: Arc::new(AtomicU64::new(0)),
            validation_errors_total: Arc::new(AtomicU64::new(0)),
            parse_errors_total: Arc::new(AtomicU64::new(0)),
            execution_errors_total: Arc::new(AtomicU64::new(0)),
            http_requests_total: Arc::new(AtomicU64::new(0)),
            http_responses_2xx: Arc::new(AtomicU64::new(0)),
            http_responses_4xx: Arc::new(AtomicU64::new(0)),
            http_responses_5xx: Arc::new(AtomicU64::new(0)),
            cache_hits: Arc::new(AtomicU64::new(0)),
            cache_misses: Arc::new(AtomicU64::new(0)),
        }
    }
}

impl Default for MetricsCollector {
    fn default() -> Self {
        Self::new()
    }
}

/// Guard for timing metrics.
pub struct TimingGuard {
    start: Instant,
    duration_atomic: Arc<AtomicU64>,
}

impl TimingGuard {
    /// Create new timing guard.
    pub fn new(duration_atomic: Arc<AtomicU64>) -> Self {
        Self {
            start: Instant::now(),
            duration_atomic,
        }
    }

    /// Record duration in microseconds and consume guard.
    pub fn record(self) {
        let duration_us = self.start.elapsed().as_micros() as u64;
        self.duration_atomic.fetch_add(duration_us, Ordering::Relaxed);
    }
}

/// Prometheus metrics output format.
#[derive(Debug)]
pub struct PrometheusMetrics {
    /// Total GraphQL queries executed
    pub queries_total: u64,
    /// Successful GraphQL queries
    pub queries_success: u64,
    /// Failed GraphQL queries
    pub queries_error: u64,
    /// Average query duration in milliseconds
    pub queries_avg_duration_ms: f64,
    /// Total database queries executed
    pub db_queries_total: u64,
    /// Average database query duration in milliseconds
    pub db_queries_avg_duration_ms: f64,
    /// Total validation errors
    pub validation_errors_total: u64,
    /// Total parse errors
    pub parse_errors_total: u64,
    /// Total execution errors
    pub execution_errors_total: u64,
    /// Total HTTP requests processed
    pub http_requests_total: u64,
    /// HTTP 2xx responses
    pub http_responses_2xx: u64,
    /// HTTP 4xx responses
    pub http_responses_4xx: u64,
    /// HTTP 5xx responses
    pub http_responses_5xx: u64,
    /// Cache hit count
    pub cache_hits: u64,
    /// Cache miss count
    pub cache_misses: u64,
    /// Cache hit ratio (0.0 to 1.0)
    pub cache_hit_ratio: f64,
}

impl PrometheusMetrics {
    /// Generate Prometheus text format output.
    #[must_use]
    pub fn to_prometheus_format(&self) -> String {
        format!(
            r"# HELP fraiseql_graphql_queries_total Total GraphQL queries executed
# TYPE fraiseql_graphql_queries_total counter
fraiseql_graphql_queries_total {}

# HELP fraiseql_graphql_queries_success Total successful GraphQL queries
# TYPE fraiseql_graphql_queries_success counter
fraiseql_graphql_queries_success {}

# HELP fraiseql_graphql_queries_error Total failed GraphQL queries
# TYPE fraiseql_graphql_queries_error counter
fraiseql_graphql_queries_error {}

# HELP fraiseql_graphql_query_duration_ms Average query execution time in milliseconds
# TYPE fraiseql_graphql_query_duration_ms gauge
fraiseql_graphql_query_duration_ms {}

# HELP fraiseql_database_queries_total Total database queries executed
# TYPE fraiseql_database_queries_total counter
fraiseql_database_queries_total {}

# HELP fraiseql_database_query_duration_ms Average database query time in milliseconds
# TYPE fraiseql_database_query_duration_ms gauge
fraiseql_database_query_duration_ms {}

# HELP fraiseql_validation_errors_total Total validation errors
# TYPE fraiseql_validation_errors_total counter
fraiseql_validation_errors_total {}

# HELP fraiseql_parse_errors_total Total parse errors
# TYPE fraiseql_parse_errors_total counter
fraiseql_parse_errors_total {}

# HELP fraiseql_execution_errors_total Total execution errors
# TYPE fraiseql_execution_errors_total counter
fraiseql_execution_errors_total {}

# HELP fraiseql_http_requests_total Total HTTP requests
# TYPE fraiseql_http_requests_total counter
fraiseql_http_requests_total {}

# HELP fraiseql_http_responses_2xx Total 2xx HTTP responses
# TYPE fraiseql_http_responses_2xx counter
fraiseql_http_responses_2xx {}

# HELP fraiseql_http_responses_4xx Total 4xx HTTP responses
# TYPE fraiseql_http_responses_4xx counter
fraiseql_http_responses_4xx {}

# HELP fraiseql_http_responses_5xx Total 5xx HTTP responses
# TYPE fraiseql_http_responses_5xx counter
fraiseql_http_responses_5xx {}

# HELP fraiseql_cache_hits Total cache hits
# TYPE fraiseql_cache_hits counter
fraiseql_cache_hits {}

# HELP fraiseql_cache_misses Total cache misses
# TYPE fraiseql_cache_misses counter
fraiseql_cache_misses {}

# HELP fraiseql_cache_hit_ratio Cache hit ratio (0-1)
# TYPE fraiseql_cache_hit_ratio gauge
fraiseql_cache_hit_ratio {:.3}
",
            self.queries_total,
            self.queries_success,
            self.queries_error,
            self.queries_avg_duration_ms,
            self.db_queries_total,
            self.db_queries_avg_duration_ms,
            self.validation_errors_total,
            self.parse_errors_total,
            self.execution_errors_total,
            self.http_requests_total,
            self.http_responses_2xx,
            self.http_responses_4xx,
            self.http_responses_5xx,
            self.cache_hits,
            self.cache_misses,
            self.cache_hit_ratio,
        )
    }
}

impl From<&MetricsCollector> for PrometheusMetrics {
    fn from(collector: &MetricsCollector) -> Self {
        let queries_total = collector.queries_total.load(Ordering::Relaxed);
        let queries_success = collector.queries_success.load(Ordering::Relaxed);
        let queries_error = collector.queries_error.load(Ordering::Relaxed);
        let queries_duration_us = collector.queries_duration_us.load(Ordering::Relaxed);

        let db_queries_total = collector.db_queries_total.load(Ordering::Relaxed);
        let db_queries_duration_us = collector.db_queries_duration_us.load(Ordering::Relaxed);

        let cache_hits = collector.cache_hits.load(Ordering::Relaxed);
        let cache_misses = collector.cache_misses.load(Ordering::Relaxed);
        let cache_total = cache_hits + cache_misses;

        Self {
            queries_total,
            queries_success,
            queries_error,
            queries_avg_duration_ms: if queries_total > 0 {
                (queries_duration_us as f64 / queries_total as f64) / 1000.0
            } else {
                0.0
            },
            db_queries_total,
            db_queries_avg_duration_ms: if db_queries_total > 0 {
                (db_queries_duration_us as f64 / db_queries_total as f64) / 1000.0
            } else {
                0.0
            },
            validation_errors_total: collector.validation_errors_total.load(Ordering::Relaxed),
            parse_errors_total: collector.parse_errors_total.load(Ordering::Relaxed),
            execution_errors_total: collector.execution_errors_total.load(Ordering::Relaxed),
            http_requests_total: collector.http_requests_total.load(Ordering::Relaxed),
            http_responses_2xx: collector.http_responses_2xx.load(Ordering::Relaxed),
            http_responses_4xx: collector.http_responses_4xx.load(Ordering::Relaxed),
            http_responses_5xx: collector.http_responses_5xx.load(Ordering::Relaxed),
            cache_hits,
            cache_misses,
            cache_hit_ratio: if cache_total > 0 {
                cache_hits as f64 / cache_total as f64
            } else {
                0.0
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_collector_creation() {
        let collector = MetricsCollector::new();
        assert_eq!(collector.queries_total.load(Ordering::Relaxed), 0);
        assert_eq!(collector.queries_success.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn test_metrics_increment() {
        let collector = MetricsCollector::new();
        collector.queries_total.fetch_add(5, Ordering::Relaxed);
        collector.queries_success.fetch_add(4, Ordering::Relaxed);
        collector.queries_error.fetch_add(1, Ordering::Relaxed);

        assert_eq!(collector.queries_total.load(Ordering::Relaxed), 5);
        assert_eq!(collector.queries_success.load(Ordering::Relaxed), 4);
        assert_eq!(collector.queries_error.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn test_prometheus_output_format() {
        let collector = MetricsCollector::new();
        collector.queries_total.store(100, Ordering::Relaxed);
        collector.queries_success.store(95, Ordering::Relaxed);
        collector.queries_error.store(5, Ordering::Relaxed);

        let metrics = PrometheusMetrics::from(&collector);
        let output = metrics.to_prometheus_format();

        assert!(output.contains("fraiseql_graphql_queries_total 100"));
        assert!(output.contains("fraiseql_graphql_queries_success 95"));
        assert!(output.contains("fraiseql_graphql_queries_error 5"));
        assert!(output.contains("# HELP"));
        assert!(output.contains("# TYPE"));
    }

    #[test]
    fn test_timing_guard() {
        let duration_atomic = Arc::new(AtomicU64::new(0));
        let guard = TimingGuard::new(duration_atomic.clone());

        // Add a small delay to ensure measurable time
        std::thread::sleep(std::time::Duration::from_micros(100));
        guard.record();

        let recorded = duration_atomic.load(Ordering::Relaxed);
        assert!(recorded >= 100);
        assert!(recorded < 1_000_000); // Should be less than 1 second
    }

    #[test]
    fn test_cache_hit_ratio_calculation() {
        let collector = MetricsCollector::new();
        collector.cache_hits.store(75, Ordering::Relaxed);
        collector.cache_misses.store(25, Ordering::Relaxed);

        let metrics = PrometheusMetrics::from(&collector);
        assert!((metrics.cache_hit_ratio - 0.75).abs() < 0.001);
    }

    #[test]
    fn test_average_duration_calculation() {
        let collector = MetricsCollector::new();
        collector.queries_total.store(10, Ordering::Relaxed);
        collector.queries_duration_us.store(50_000, Ordering::Relaxed); // 50ms total

        let metrics = PrometheusMetrics::from(&collector);
        assert!((metrics.queries_avg_duration_ms - 5.0).abs() < 0.01); // 5ms average
    }
}
