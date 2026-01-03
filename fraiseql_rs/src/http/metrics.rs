//! HTTP metrics collection and Prometheus export
//!
//! Tracks HTTP-specific metrics including request counts, response codes,
//! authentication success/failure, security violations, and request duration histograms.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

/// HTTP request metrics with atomic counters
///
/// Thread-safe metrics collection for HTTP requests, responses, auth events,
/// and security violations. Supports export to Prometheus format.
#[derive(Debug)]
pub struct HttpMetrics {
    // =========================================================================
    // Request tracking
    // =========================================================================
    /// Total number of HTTP requests received
    pub total_requests: AtomicU64,

    /// Number of successful requests (no errors)
    pub successful_requests: AtomicU64,

    /// Number of failed requests (with errors)
    pub failed_requests: AtomicU64,

    // =========================================================================
    // Response code counters
    // =========================================================================
    /// 200 OK - Success
    pub status_200: AtomicU64,

    /// 400 Bad Request - Validation failure
    pub status_400: AtomicU64,

    /// 401 Unauthorized - Auth failure (invalid/expired token)
    pub status_401: AtomicU64,

    /// 403 Forbidden - CSRF violation or permission denied
    pub status_403: AtomicU64,

    /// 429 Too Many Requests - Rate limit violation
    pub status_429: AtomicU64,

    /// 500 Internal Server Error - Unexpected error
    pub status_500: AtomicU64,

    // =========================================================================
    // Authentication metrics
    // =========================================================================
    /// Successful authentication attempts
    pub auth_success: AtomicU64,

    /// Failed authentication attempts
    pub auth_failures: AtomicU64,

    /// Anonymous requests (no auth attempted)
    pub anonymous_requests: AtomicU64,

    // =========================================================================
    // Security metrics
    // =========================================================================
    /// Rate limit violations
    pub rate_limit_violations: AtomicU64,

    /// Query validation failures
    pub query_validation_failures: AtomicU64,

    /// CSRF token validation failures
    pub csrf_violations: AtomicU64,

    /// Invalid JWT token attempts
    pub invalid_tokens: AtomicU64,

    /// Failed attempts to access /metrics endpoint
    pub metrics_endpoint_auth_failures: AtomicU64,

    // =========================================================================
    // Performance metrics (histogram buckets in milliseconds)
    // =========================================================================
    /// Total request duration (sum in milliseconds)
    pub total_duration_ms: AtomicU64,

    /// Request duration histogram buckets (Prometheus defaults)
    /// Buckets: 5ms, 10ms, 25ms, 50ms, 75ms, 100ms, 250ms, 500ms, 750ms, 1000ms, 2500ms, 5000ms, 7500ms, 10000ms
    pub request_duration_buckets: [AtomicU64; 14],
}

impl HttpMetrics {
    /// Create new HTTP metrics collection
    #[must_use]
    pub fn new() -> Self {
        Self {
            total_requests: AtomicU64::new(0),
            successful_requests: AtomicU64::new(0),
            failed_requests: AtomicU64::new(0),
            status_200: AtomicU64::new(0),
            status_400: AtomicU64::new(0),
            status_401: AtomicU64::new(0),
            status_403: AtomicU64::new(0),
            status_429: AtomicU64::new(0),
            status_500: AtomicU64::new(0),
            auth_success: AtomicU64::new(0),
            auth_failures: AtomicU64::new(0),
            anonymous_requests: AtomicU64::new(0),
            rate_limit_violations: AtomicU64::new(0),
            query_validation_failures: AtomicU64::new(0),
            csrf_violations: AtomicU64::new(0),
            invalid_tokens: AtomicU64::new(0),
            metrics_endpoint_auth_failures: AtomicU64::new(0),
            total_duration_ms: AtomicU64::new(0),
            request_duration_buckets: [
                AtomicU64::new(0),
                AtomicU64::new(0),
                AtomicU64::new(0),
                AtomicU64::new(0),
                AtomicU64::new(0),
                AtomicU64::new(0),
                AtomicU64::new(0),
                AtomicU64::new(0),
                AtomicU64::new(0),
                AtomicU64::new(0),
                AtomicU64::new(0),
                AtomicU64::new(0),
                AtomicU64::new(0),
                AtomicU64::new(0),
            ],
        }
    }

    /// Record request completion with duration and status code
    pub fn record_request_end(&self, duration: Duration, status_code: u16) {
        let duration_ms = duration.as_millis() as u64;

        // Increment total request counter
        self.total_requests.fetch_add(1, Ordering::Relaxed);

        // Record status code
        match status_code {
            200 => {
                self.status_200.fetch_add(1, Ordering::Relaxed);
            }
            400 => {
                self.status_400.fetch_add(1, Ordering::Relaxed);
            }
            401 => {
                self.status_401.fetch_add(1, Ordering::Relaxed);
            }
            403 => {
                self.status_403.fetch_add(1, Ordering::Relaxed);
            }
            429 => {
                self.status_429.fetch_add(1, Ordering::Relaxed);
            }
            500 => {
                self.status_500.fetch_add(1, Ordering::Relaxed);
            }
            _ => {
                // Unexpected status code - record as 500
                self.status_500.fetch_add(1, Ordering::Relaxed);
            }
        }

        // Record success/failure
        if status_code == 200 {
            self.successful_requests.fetch_add(1, Ordering::Relaxed);
        } else {
            self.failed_requests.fetch_add(1, Ordering::Relaxed);
        }

        // Record duration
        self.total_duration_ms.fetch_add(duration_ms, Ordering::Relaxed);

        // Record duration histogram bucket
        // Buckets (in milliseconds): 5, 10, 25, 50, 75, 100, 250, 500, 750, 1000, 2500, 5000, 7500, 10000
        let bucket_thresholds = [5, 10, 25, 50, 75, 100, 250, 500, 750, 1000, 2500, 5000, 7500, 10000];

        for (i, &threshold) in bucket_thresholds.iter().enumerate() {
            if duration_ms <= threshold {
                // Increment this bucket and all following buckets (cumulative)
                for j in i..14 {
                    self.request_duration_buckets[j].fetch_add(1, Ordering::Relaxed);
                }
                break;
            }
        }

        // Always increment +Inf bucket (implicit in count)
    }

    /// Record successful authentication
    pub fn record_auth_success(&self) {
        self.auth_success.fetch_add(1, Ordering::Relaxed);
    }

    /// Record failed authentication attempt
    pub fn record_auth_failure(&self) {
        self.auth_failures.fetch_add(1, Ordering::Relaxed);
    }

    /// Record invalid JWT token attempt
    pub fn record_invalid_token(&self) {
        self.invalid_tokens.fetch_add(1, Ordering::Relaxed);
        self.record_auth_failure();
    }

    /// Record anonymous request (no auth attempted)
    pub fn record_anonymous_request(&self) {
        self.anonymous_requests.fetch_add(1, Ordering::Relaxed);
    }

    /// Record failed attempt to access /metrics endpoint
    pub fn record_metrics_auth_failure(&self) {
        self.metrics_endpoint_auth_failures.fetch_add(1, Ordering::Relaxed);
    }

    /// Record rate limit violation
    pub fn record_rate_limit_violation(&self) {
        self.rate_limit_violations.fetch_add(1, Ordering::Relaxed);
    }

    /// Record query validation failure
    pub fn record_query_validation_failure(&self) {
        self.query_validation_failures.fetch_add(1, Ordering::Relaxed);
    }

    /// Record CSRF token validation failure
    pub fn record_csrf_violation(&self) {
        self.csrf_violations.fetch_add(1, Ordering::Relaxed);
    }

    /// Export metrics in Prometheus text format
    #[must_use]
    pub fn export_prometheus(&self) -> String {
        let mut output = String::new();

        // Request counts
        output.push_str("# HELP fraiseql_http_requests_total Total HTTP requests\n");
        output.push_str("# TYPE fraiseql_http_requests_total counter\n");
        output.push_str(&format!(
            "fraiseql_http_requests_total{{status=\"200\"}} {}\n",
            self.status_200.load(Ordering::Relaxed)
        ));
        output.push_str(&format!(
            "fraiseql_http_requests_total{{status=\"400\"}} {}\n",
            self.status_400.load(Ordering::Relaxed)
        ));
        output.push_str(&format!(
            "fraiseql_http_requests_total{{status=\"401\"}} {}\n",
            self.status_401.load(Ordering::Relaxed)
        ));
        output.push_str(&format!(
            "fraiseql_http_requests_total{{status=\"403\"}} {}\n",
            self.status_403.load(Ordering::Relaxed)
        ));
        output.push_str(&format!(
            "fraiseql_http_requests_total{{status=\"429\"}} {}\n",
            self.status_429.load(Ordering::Relaxed)
        ));
        output.push_str(&format!(
            "fraiseql_http_requests_total{{status=\"500\"}} {}\n",
            self.status_500.load(Ordering::Relaxed)
        ));

        // Success/failure counts
        output.push_str("# HELP fraiseql_http_successful_requests_total Successful HTTP requests\n");
        output.push_str("# TYPE fraiseql_http_successful_requests_total counter\n");
        output.push_str(&format!(
            "fraiseql_http_successful_requests_total {}\n",
            self.successful_requests.load(Ordering::Relaxed)
        ));

        output.push_str("# HELP fraiseql_http_failed_requests_total Failed HTTP requests\n");
        output.push_str("# TYPE fraiseql_http_failed_requests_total counter\n");
        output.push_str(&format!(
            "fraiseql_http_failed_requests_total {}\n",
            self.failed_requests.load(Ordering::Relaxed)
        ));

        // Authentication metrics
        output.push_str("# HELP fraiseql_http_auth_success_total Successful authentication attempts\n");
        output.push_str("# TYPE fraiseql_http_auth_success_total counter\n");
        output.push_str(&format!(
            "fraiseql_http_auth_success_total {}\n",
            self.auth_success.load(Ordering::Relaxed)
        ));

        output.push_str("# HELP fraiseql_http_auth_failures_total Failed authentication attempts\n");
        output.push_str("# TYPE fraiseql_http_auth_failures_total counter\n");
        output.push_str(&format!(
            "fraiseql_http_auth_failures_total {}\n",
            self.auth_failures.load(Ordering::Relaxed)
        ));

        output.push_str("# HELP fraiseql_http_anonymous_requests_total Anonymous requests\n");
        output.push_str("# TYPE fraiseql_http_anonymous_requests_total counter\n");
        output.push_str(&format!(
            "fraiseql_http_anonymous_requests_total {}\n",
            self.anonymous_requests.load(Ordering::Relaxed)
        ));

        // Invalid token metric
        output.push_str("# HELP fraiseql_http_invalid_tokens_total Invalid JWT token attempts\n");
        output.push_str("# TYPE fraiseql_http_invalid_tokens_total counter\n");
        output.push_str(&format!(
            "fraiseql_http_invalid_tokens_total {}\n",
            self.invalid_tokens.load(Ordering::Relaxed)
        ));

        // Security metrics
        output.push_str("# HELP fraiseql_http_rate_limit_violations_total Rate limit violations\n");
        output.push_str("# TYPE fraiseql_http_rate_limit_violations_total counter\n");
        output.push_str(&format!(
            "fraiseql_http_rate_limit_violations_total {}\n",
            self.rate_limit_violations.load(Ordering::Relaxed)
        ));

        output.push_str("# HELP fraiseql_http_query_validation_failures_total Query validation failures\n");
        output.push_str("# TYPE fraiseql_http_query_validation_failures_total counter\n");
        output.push_str(&format!(
            "fraiseql_http_query_validation_failures_total {}\n",
            self.query_validation_failures.load(Ordering::Relaxed)
        ));

        output.push_str("# HELP fraiseql_http_csrf_violations_total CSRF token violations\n");
        output.push_str("# TYPE fraiseql_http_csrf_violations_total counter\n");
        output.push_str(&format!(
            "fraiseql_http_csrf_violations_total {}\n",
            self.csrf_violations.load(Ordering::Relaxed)
        ));

        output.push_str("# HELP fraiseql_http_metrics_endpoint_auth_failures_total Failed /metrics endpoint auth attempts\n");
        output.push_str("# TYPE fraiseql_http_metrics_endpoint_auth_failures_total counter\n");
        output.push_str(&format!(
            "fraiseql_http_metrics_endpoint_auth_failures_total {}\n",
            self.metrics_endpoint_auth_failures.load(Ordering::Relaxed)
        ));

        // Request duration histogram
        output.push_str("# HELP fraiseql_http_request_duration_ms Request duration in milliseconds\n");
        output.push_str("# TYPE fraiseql_http_request_duration_ms histogram\n");

        let bucket_labels = ["5", "10", "25", "50", "75", "100", "250", "500", "750", "1000", "2500", "5000", "7500", "10000"];
        for (i, label) in bucket_labels.iter().enumerate() {
            output.push_str(&format!(
                "fraiseql_http_request_duration_ms_bucket{{le=\"{}\"}} {}\n",
                label,
                self.request_duration_buckets[i].load(Ordering::Relaxed)
            ));
        }

        // +Inf bucket
        output.push_str(&format!(
            "fraiseql_http_request_duration_ms_bucket{{le=\"+Inf\"}} {}\n",
            self.total_requests.load(Ordering::Relaxed)
        ));

        // Histogram count and sum
        output.push_str(&format!(
            "fraiseql_http_request_duration_ms_count {}\n",
            self.total_requests.load(Ordering::Relaxed)
        ));
        output.push_str(&format!(
            "fraiseql_http_request_duration_ms_sum {}\n",
            self.total_duration_ms.load(Ordering::Relaxed)
        ));

        output
    }
}

impl Default for HttpMetrics {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_http_metrics_new() {
        let metrics = HttpMetrics::new();
        assert_eq!(metrics.total_requests.load(Ordering::Relaxed), 0);
        assert_eq!(metrics.status_200.load(Ordering::Relaxed), 0);
        assert_eq!(metrics.auth_success.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn test_record_request_success() {
        let metrics = HttpMetrics::new();
        metrics.record_request_end(Duration::from_millis(50), 200);

        assert_eq!(metrics.total_requests.load(Ordering::Relaxed), 1);
        assert_eq!(metrics.status_200.load(Ordering::Relaxed), 1);
        assert_eq!(metrics.successful_requests.load(Ordering::Relaxed), 1);
        assert_eq!(metrics.failed_requests.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn test_record_request_failure() {
        let metrics = HttpMetrics::new();
        metrics.record_request_end(Duration::from_millis(100), 500);

        assert_eq!(metrics.total_requests.load(Ordering::Relaxed), 1);
        assert_eq!(metrics.status_500.load(Ordering::Relaxed), 1);
        assert_eq!(metrics.successful_requests.load(Ordering::Relaxed), 0);
        assert_eq!(metrics.failed_requests.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn test_record_auth_success() {
        let metrics = HttpMetrics::new();
        metrics.record_auth_success();
        metrics.record_auth_success();

        assert_eq!(metrics.auth_success.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn test_record_auth_failure() {
        let metrics = HttpMetrics::new();
        metrics.record_auth_failure();
        metrics.record_auth_failure();
        metrics.record_auth_failure();

        assert_eq!(metrics.auth_failures.load(Ordering::Relaxed), 3);
    }

    #[test]
    fn test_record_invalid_token() {
        let metrics = HttpMetrics::new();
        metrics.record_invalid_token();

        assert_eq!(metrics.invalid_tokens.load(Ordering::Relaxed), 1);
        assert_eq!(metrics.auth_failures.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn test_record_anonymous_request() {
        let metrics = HttpMetrics::new();
        metrics.record_anonymous_request();
        metrics.record_anonymous_request();

        assert_eq!(metrics.anonymous_requests.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn test_record_metrics_auth_failure() {
        let metrics = HttpMetrics::new();
        metrics.record_metrics_auth_failure();

        assert_eq!(metrics.metrics_endpoint_auth_failures.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn test_record_rate_limit_violation() {
        let metrics = HttpMetrics::new();
        metrics.record_rate_limit_violation();
        metrics.record_rate_limit_violation();

        assert_eq!(metrics.rate_limit_violations.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn test_record_query_validation_failure() {
        let metrics = HttpMetrics::new();
        metrics.record_query_validation_failure();

        assert_eq!(metrics.query_validation_failures.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn test_record_csrf_violation() {
        let metrics = HttpMetrics::new();
        metrics.record_csrf_violation();

        assert_eq!(metrics.csrf_violations.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn test_duration_bucketing_under_5ms() {
        let metrics = HttpMetrics::new();
        metrics.record_request_end(Duration::from_millis(3), 200);

        // 3ms falls in all buckets >= 5ms
        assert_eq!(metrics.request_duration_buckets[0].load(Ordering::Relaxed), 1); // 5ms
        assert_eq!(metrics.request_duration_buckets[13].load(Ordering::Relaxed), 1); // 10000ms
    }

    #[test]
    fn test_duration_bucketing_5ms() {
        let metrics = HttpMetrics::new();
        metrics.record_request_end(Duration::from_millis(5), 200);

        assert_eq!(metrics.request_duration_buckets[0].load(Ordering::Relaxed), 1); // 5ms
        assert_eq!(metrics.request_duration_buckets[13].load(Ordering::Relaxed), 1); // 10000ms
    }

    #[test]
    fn test_duration_bucketing_50ms() {
        let metrics = HttpMetrics::new();
        metrics.record_request_end(Duration::from_millis(50), 200);

        // 50ms should be in bucket 3 (50ms) and all larger buckets
        assert_eq!(metrics.request_duration_buckets[0].load(Ordering::Relaxed), 1); // 5ms
        assert_eq!(metrics.request_duration_buckets[3].load(Ordering::Relaxed), 1); // 50ms
        assert_eq!(metrics.request_duration_buckets[13].load(Ordering::Relaxed), 1); // 10000ms
    }

    #[test]
    fn test_duration_bucketing_all_buckets() {
        let metrics = HttpMetrics::new();

        // Record requests at various durations
        metrics.record_request_end(Duration::from_millis(5), 200);
        metrics.record_request_end(Duration::from_millis(25), 200);
        metrics.record_request_end(Duration::from_millis(100), 200);
        metrics.record_request_end(Duration::from_millis(1000), 200);
        metrics.record_request_end(Duration::from_millis(10000), 200);

        // Verify histogram is populated
        assert!(metrics.request_duration_buckets[0].load(Ordering::Relaxed) > 0); // 5ms
        assert!(metrics.request_duration_buckets[2].load(Ordering::Relaxed) > 0); // 25ms
        assert!(metrics.request_duration_buckets[5].load(Ordering::Relaxed) > 0); // 100ms
        assert!(metrics.request_duration_buckets[9].load(Ordering::Relaxed) > 0); // 1000ms
        assert!(metrics.request_duration_buckets[13].load(Ordering::Relaxed) > 0); // 10000ms
    }

    #[test]
    fn test_metrics_prometheus_export_format() {
        let metrics = HttpMetrics::new();
        metrics.record_request_end(Duration::from_millis(50), 200);
        metrics.record_auth_success();
        metrics.record_rate_limit_violation();

        let output = metrics.export_prometheus();

        // Verify output contains expected metrics
        assert!(output.contains("fraiseql_http_requests_total{status=\"200\"}"));
        assert!(output.contains("fraiseql_http_auth_success_total"));
        assert!(output.contains("fraiseql_http_rate_limit_violations_total"));
        assert!(output.contains("fraiseql_http_request_duration_ms_bucket"));
        assert!(output.contains("# HELP"));
        assert!(output.contains("# TYPE"));
    }

    #[test]
    fn test_status_code_counters() {
        let metrics = HttpMetrics::new();

        metrics.record_request_end(Duration::from_millis(10), 200);
        metrics.record_request_end(Duration::from_millis(10), 400);
        metrics.record_request_end(Duration::from_millis(10), 401);
        metrics.record_request_end(Duration::from_millis(10), 403);
        metrics.record_request_end(Duration::from_millis(10), 429);
        metrics.record_request_end(Duration::from_millis(10), 500);

        assert_eq!(metrics.status_200.load(Ordering::Relaxed), 1);
        assert_eq!(metrics.status_400.load(Ordering::Relaxed), 1);
        assert_eq!(metrics.status_401.load(Ordering::Relaxed), 1);
        assert_eq!(metrics.status_403.load(Ordering::Relaxed), 1);
        assert_eq!(metrics.status_429.load(Ordering::Relaxed), 1);
        assert_eq!(metrics.status_500.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn test_concurrent_metrics_recording() {
        let metrics = std::sync::Arc::new(HttpMetrics::new());
        let mut handles = vec![];

        for _ in 0..10 {
            let m = metrics.clone();
            handles.push(std::thread::spawn(move || {
                for _ in 0..100 {
                    m.record_request_end(Duration::from_millis(50), 200);
                    m.record_auth_success();
                }
            }));
        }

        for handle in handles {
            handle.join().unwrap();
        }

        // 10 threads × 100 requests = 1000 total
        assert_eq!(metrics.total_requests.load(Ordering::Relaxed), 1000);
        assert_eq!(metrics.auth_success.load(Ordering::Relaxed), 1000);
    }

    #[test]
    fn test_metrics_default() {
        let metrics = HttpMetrics::default();
        assert_eq!(metrics.total_requests.load(Ordering::Relaxed), 0);
    }
}
