//! Metrics collection with SLO tracking.

use axum::{extract::Request, middleware::Next, response::Response};
use std::time::Instant;

#[cfg(feature = "metrics")]
use metrics::{counter, describe_counter, describe_gauge, describe_histogram, gauge, histogram};
#[cfg(feature = "metrics")]
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};

use crate::config::metrics::{MetricsConfig, SloConfig};
use fraiseql_error::RuntimeError;

/// Initialize metrics exporter with SLO buckets
///
/// # Errors
///
/// Returns an error if metrics initialization fails
#[cfg(feature = "metrics")]
pub fn init_metrics(config: &MetricsConfig) -> Result<PrometheusHandle, RuntimeError> {
    let builder = PrometheusBuilder::new();

    // Configure histogram buckets for latency SLOs
    let slo_buckets = vec![
        0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0,
    ];

    let handle = builder
        .set_buckets(&slo_buckets)
        .map_err(|e| RuntimeError::Internal {
            message: format!("Failed to configure metric buckets: {e}"),
            source: None,
        })?
        .install_recorder()
        .map_err(|e| RuntimeError::Internal {
            message: format!("Failed to install metrics: {e}"),
            source: None,
        })?;

    // Register standard metrics with descriptions
    describe_metrics();

    // Initialize SLO tracking metrics
    init_slo_metrics(&config.slos);

    Ok(handle)
}

#[cfg(not(feature = "metrics"))]
pub fn init_metrics(_config: &MetricsConfig) -> Result<(), RuntimeError> {
    Ok(())
}

#[cfg(feature = "metrics")]
fn describe_metrics() {
    // HTTP metrics
    describe_counter!("http_requests_total", "Total number of HTTP requests");
    describe_histogram!(
        "http_request_duration_seconds",
        "HTTP request duration in seconds"
    );

    // GraphQL metrics
    describe_counter!(
        "graphql_operations_total",
        "Total number of GraphQL operations"
    );
    describe_histogram!(
        "graphql_operation_duration_seconds",
        "GraphQL operation duration in seconds"
    );
    describe_counter!("graphql_errors_total", "Total number of GraphQL errors");

    // Webhook metrics
    describe_counter!(
        "webhook_events_total",
        "Total number of webhook events received"
    );
    describe_histogram!(
        "webhook_processing_duration_seconds",
        "Webhook processing duration in seconds"
    );

    // Auth metrics
    describe_counter!(
        "auth_operations_total",
        "Total number of authentication operations"
    );
    describe_histogram!(
        "auth_operation_duration_seconds",
        "Authentication operation duration in seconds"
    );

    // File metrics
    describe_counter!(
        "file_operations_total",
        "Total number of file operations"
    );
    describe_histogram!(
        "file_upload_duration_seconds",
        "File upload duration in seconds"
    );
    describe_histogram!("file_size_bytes", "File size in bytes");

    // Notification metrics
    describe_counter!(
        "notifications_total",
        "Total number of notifications sent"
    );
    describe_histogram!(
        "notification_duration_seconds",
        "Notification send duration in seconds"
    );

    // Observer metrics
    describe_counter!(
        "observer_events_total",
        "Total number of observer events processed"
    );
    describe_histogram!(
        "observer_action_duration_seconds",
        "Observer action duration in seconds"
    );

    // Database metrics
    describe_gauge!(
        "db_pool_connections_active",
        "Number of active database connections"
    );
    describe_gauge!(
        "db_pool_connections_idle",
        "Number of idle database connections"
    );
    describe_histogram!(
        "db_query_duration_seconds",
        "Database query duration in seconds"
    );

    // Rate limiting metrics
    describe_counter!("rate_limit_requests_total", "Total rate limit decisions");
    describe_gauge!("rate_limit_queue_depth", "Current rate limit queue depth");

    // Circuit breaker metrics
    describe_counter!(
        "circuit_breaker_state_changes_total",
        "Circuit breaker state changes"
    );
    describe_gauge!(
        "circuit_breaker_state",
        "Current circuit breaker state (0=closed, 1=open, 2=half-open)"
    );
}

#[cfg(feature = "metrics")]
fn init_slo_metrics(config: &SloConfig) {
    // SLO compliance metrics
    describe_gauge!(
        "slo_latency_target_seconds",
        "SLO latency target in seconds"
    );
    describe_counter!(
        "slo_latency_violations_total",
        "Total SLO latency violations"
    );
    describe_gauge!(
        "slo_error_budget_remaining",
        "Remaining SLO error budget (0-1)"
    );

    // Set initial targets
    gauge!("slo_latency_target_seconds", "component" => "graphql")
        .set(f64::from(config.latency_targets.graphql_p99_ms) / 1000.0);
    gauge!("slo_latency_target_seconds", "component" => "webhook")
        .set(f64::from(config.latency_targets.webhook_p99_ms) / 1000.0);
    gauge!("slo_latency_target_seconds", "component" => "auth")
        .set(f64::from(config.latency_targets.auth_p99_ms) / 1000.0);
    gauge!("slo_latency_target_seconds", "component" => "file_upload")
        .set(f64::from(config.latency_targets.file_upload_p99_ms) / 1000.0);
}

/// Middleware to record HTTP request metrics
pub async fn metrics_middleware(req: Request, next: Next) -> Response {
    #[cfg(feature = "metrics")]
    let start = Instant::now();
    #[cfg(feature = "metrics")]
    let method = req.method().to_string();
    #[cfg(feature = "metrics")]
    let path = normalize_path(req.uri().path());

    let response = next.run(req).await;

    #[cfg(feature = "metrics")]
    {
        let status = response.status().as_u16().to_string();
        let status_class = format!("{}xx", response.status().as_u16() / 100);
        let duration = start.elapsed().as_secs_f64();

        counter!(
            "http_requests_total",
            "method" => method.clone(),
            "path" => path.clone(),
            "status" => status.clone(),
            "status_class" => status_class
        )
        .increment(1);

        histogram!(
            "http_request_duration_seconds",
            "method" => method,
            "path" => path
        )
        .record(duration);
    }

    response
}

/// Normalize path for metrics (replace IDs with placeholders)
fn normalize_path(path: &str) -> String {
    // Simple normalization: replace numeric segments
    let parts: Vec<&str> = path.split('/').collect();
    let normalized: Vec<String> = parts
        .iter()
        .map(|&part| {
            if part.is_empty() {
                // Keep empty segments (like the leading / in paths)
                part.to_string()
            } else if part.chars().all(|c| c.is_ascii_digit()) {
                ":id".to_string()
            } else if is_uuid(part) {
                ":id".to_string()
            } else {
                part.to_string()
            }
        })
        .collect();
    normalized.join("/")
}

fn is_uuid(s: &str) -> bool {
    s.len() == 36
        && s.chars().enumerate().all(|(i, c)| {
            if i == 8 || i == 13 || i == 18 || i == 23 {
                c == '-'
            } else {
                c.is_ascii_hexdigit()
            }
        })
}

/// Record operation metrics with SLO tracking
pub struct OperationMetrics {
    component: &'static str,
    operation: String,
    start: Instant,
    slo_target_ms: u64,
}

impl OperationMetrics {
    #[must_use]
    pub fn new(component: &'static str, operation: impl Into<String>, slo_target_ms: u64) -> Self {
        Self {
            component,
            operation: operation.into(),
            start: Instant::now(),
            slo_target_ms,
        }
    }

    pub fn success(self) {
        self.record("success");
    }

    pub fn failure(self, error_type: &str) {
        self.record(error_type);
    }

    #[cfg(feature = "metrics")]
    fn record(self, status: &str) {
        let duration = self.start.elapsed();
        let duration_secs = duration.as_secs_f64();

        // Record duration histogram
        let histogram_name = format!("{}_operation_duration_seconds", self.component);
        histogram!(
            histogram_name,
            "operation" => self.operation.clone(),
            "status" => status.to_string()
        )
        .record(duration_secs);

        // Record total counter
        let counter_name = format!("{}_operations_total", self.component);
        counter!(
            counter_name,
            "operation" => self.operation.clone(),
            "status" => status.to_string()
        )
        .increment(1);

        // Check SLO violation
        if duration.as_millis() as u64 > self.slo_target_ms {
            counter!(
                "slo_latency_violations_total",
                "component" => self.component.to_string(),
                "operation" => self.operation
            )
            .increment(1);
        }
    }

    #[cfg(not(feature = "metrics"))]
    fn record(self, _status: &str) {
        // No-op when metrics feature is disabled
    }
}

/// Metrics endpoint handler
#[cfg(feature = "metrics")]
pub async fn metrics_handler(
    axum::extract::State(handle): axum::extract::State<PrometheusHandle>,
) -> String {
    handle.render()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_path() {
        assert_eq!(normalize_path("/users/123"), "/users/:id");
        assert_eq!(
            normalize_path("/files/550e8400-e29b-41d4-a716-446655440000"),
            "/files/:id"
        );
        assert_eq!(
            normalize_path("/api/v1/users/123/posts/456"),
            "/api/v1/users/:id/posts/:id"
        );
    }

    #[test]
    fn test_is_uuid() {
        assert!(is_uuid("550e8400-e29b-41d4-a716-446655440000"));
        assert!(!is_uuid("not-a-uuid"));
        assert!(!is_uuid("123"));
    }
}
