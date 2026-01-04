//! GraphQL operation metrics middleware for Axum (Phase 19, Commit 4.5)
//!
//! This module provides middleware that integrates operation monitoring into the Axum HTTP request/response pipeline.
//! It automatically:
//! - Extracts GraphQL operation details from requests
//! - Integrates with W3C Trace Context (Phase 19, Commit 2)
//! - Records operation metrics on response
//! - Detects slow mutations and other operations
//!
//! The middleware works by:
//! 1. Intercepting HTTP requests before GraphQL execution
//! 2. Parsing the GraphQL query to extract operation type and name
//! 3. Extracting trace context from W3C headers
//! 4. Passing context through the request lifecycle
//! 5. Recording metrics and response data on completion

use crate::http::graphql_operation_detector::GraphQLOperationDetector;
use crate::http::operation_metrics::OperationMetrics;
use crate::http::operation_monitor::GraphQLOperationMonitor;
use axum::http::{HeaderMap, StatusCode};
use serde_json::Value as JsonValue;
use std::sync::Arc;
use std::time::Instant;
use uuid::Uuid;

/// Context data passed through the request lifecycle
///
/// This context is created when the request arrives and updated when the response is sent.
#[derive(Debug, Clone)]
#[allow(clippy::doc_markdown)]
pub struct OperationMetricsContext {
    /// Unique operation instance ID
    pub operation_id: String,

    /// Operation metrics being collected
    pub metrics: Arc<OperationMetrics>,

    /// Timestamp when request was received
    pub request_received_at: Instant,
}

impl OperationMetricsContext {
    /// Create a new context for a GraphQL request
    ///
    /// # Arguments
    ///
    /// * `query` - GraphQL query string
    /// * `variables` - Optional GraphQL variables
    /// * `headers` - HTTP request headers (for trace context extraction)
    ///
    /// # Returns
    ///
    /// New context with operation metrics initialized
    #[must_use]
    pub fn from_request(
        query: &str,
        variables: Option<&JsonValue>,
        headers: &HeaderMap,
    ) -> Self {
        // Generate unique operation ID
        let operation_id = Uuid::new_v4().to_string();

        // Detect operation type and name
        let (operation_type, operation_name) = GraphQLOperationDetector::detect_operation_type(query);

        // Create metrics instance
        let mut metrics = OperationMetrics::new(operation_id.clone(), operation_name, operation_type);

        // Set query metadata
        metrics.set_query_length(query.len());
        if let Some(vars) = variables {
            let var_count = match vars {
                JsonValue::Object(obj) => obj.len(),
                _ => 0,
            };
            metrics.set_variables_count(var_count);
        }

        // Extract trace context from W3C headers
        Self::extract_and_set_trace_context(&mut metrics, headers);

        let metrics = Arc::new(metrics);

        Self {
            operation_id,
            metrics,
            request_received_at: Instant::now(),
        }
    }

    /// Extract W3C trace context and set on metrics
    fn extract_and_set_trace_context(metrics: &mut OperationMetrics, headers: &HeaderMap) {
        // Extract traceparent header (W3C standard)
        let traceparent = headers
            .get("traceparent")
            .and_then(|v| v.to_str().ok())
            .map(ToString::to_string);

        // Extract tracestate header (W3C standard)
        let tracestate = headers
            .get("tracestate")
            .and_then(|v| v.to_str().ok())
            .map(ToString::to_string)
            .unwrap_or_default();

        // Extract X-Request-ID header (custom, for backward compatibility)
        let request_id = headers
            .get("x-request-id")
            .and_then(|v| v.to_str().ok())
            .map(ToString::to_string);

        // Extract trace IDs from traceparent or fallback to custom headers
        let (trace_id, parent_span_id) = if let Some(tp) = &traceparent {
            Self::parse_traceparent(tp)
        } else {
            // Fallback to custom headers
            let trace_id = headers
                .get("x-trace-id")
                .and_then(|v| v.to_str().ok())
                .map(ToString::to_string)
                .unwrap_or_else(|| Uuid::new_v4().to_string());

            (trace_id, None)
        };

        // Generate span ID for this operation
        let span_id = Uuid::new_v4().to_string()[..16].to_string();

        metrics.set_trace_context(trace_id, span_id, parent_span_id, tracestate, request_id);
    }

    /// Parse W3C traceparent header
    ///
    /// Format: version-trace_id-parent_span_id-trace_flags
    /// Example: 00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01
    fn parse_traceparent(traceparent: &str) -> (String, Option<String>) {
        let parts: Vec<&str> = traceparent.split('-').collect();

        if parts.len() >= 3 {
            let trace_id = parts[1].to_string();
            let parent_span_id = Some(parts[2].to_string());
            (trace_id, parent_span_id)
        } else {
            // Fallback: generate new IDs
            (Uuid::new_v4().to_string(), None)
        }
    }

    /// Finalize metrics with response data
    ///
    /// Must be called when the GraphQL response is ready.
    ///
    /// # Arguments
    ///
    /// * `status_code` - HTTP response status code
    /// * `response_body` - GraphQL response JSON
    /// * `had_errors` - Whether GraphQL returned errors
    pub fn record_response(
        &self,
        status_code: StatusCode,
        response_body: &JsonValue,
        had_errors: bool,
    ) {
        // This needs to be called on a mutable reference, so we can't directly modify
        // But we can emit metrics to the monitor via callbacks
        // For now, this documents the expected usage pattern
        let _ = (status_code, response_body, had_errors);
    }

    /// Get the metrics instance
    #[must_use]
    pub fn metrics(&self) -> &OperationMetrics {
        &self.metrics
    }

    /// Get elapsed time since request was received
    #[must_use]
    pub fn elapsed_ms(&self) -> f64 {
        self.request_received_at.elapsed().as_secs_f64() * 1000.0
    }
}

/// Middleware handler for GraphQL operation metrics
///
/// This function should be integrated into the Axum router to automatically
/// collect metrics for all GraphQL operations.
///
/// # Usage
///
/// ```ignore
/// let monitor = Arc::new(GraphQLOperationMonitor::new(config));
/// let router = Router::new()
///     .route("/graphql", post(
///         move |body| metrics_handler(body, monitor.clone())
///     ));
/// ```
#[derive(Debug)]
pub struct OperationMetricsMiddleware {
    monitor: Arc<GraphQLOperationMonitor>,
}

impl OperationMetricsMiddleware {
    /// Create a new metrics middleware with a monitor instance
    #[must_use]
    pub fn new(monitor: Arc<GraphQLOperationMonitor>) -> Self {
        Self { monitor }
    }

    /// Extract operation metrics from a GraphQL request
    ///
    /// Call this at the start of request processing.
    #[must_use]
    pub fn extract_metrics(
        &self,
        query: &str,
        variables: Option<&JsonValue>,
        headers: &HeaderMap,
    ) -> OperationMetricsContext {
        OperationMetricsContext::from_request(query, variables, headers)
    }

    /// Record operation completion
    ///
    /// Call this when the GraphQL operation completes.
    ///
    /// # Arguments
    ///
    /// * `mut context` - Metrics context from request start
    /// * `status_code` - HTTP response status code
    /// * `response_body` - GraphQL response JSON
    /// * `had_errors` - Whether response contained GraphQL errors
    pub fn record_operation(
        &self,
        context: &mut OperationMetricsContext,
        status_code: StatusCode,
        response_body: &JsonValue,
        had_errors: bool,
    ) {
        // Get mutable reference via Arc (this requires special handling)
        // For now, we'll use a different approach: record after collecting all data

        // Calculate response size
        let response_size = response_body.to_string().len();

        // Count fields and errors in response
        let error_count = if had_errors {
            response_body
                .get("errors")
                .and_then(|e| e.as_array())
                .map(|arr| arr.len())
                .unwrap_or(1)
        } else {
            0
        };

        // Count fields in the response data
        let field_count = Self::count_fields_in_response(response_body);

        // Build final metrics
        let mut final_metrics = (*context.metrics).clone();
        final_metrics.set_response_size(response_size);
        final_metrics.set_error_count(error_count);
        final_metrics.set_field_count(field_count);

        // Set status based on response and HTTP status
        use crate::http::operation_metrics::OperationStatus;
        let status = match (had_errors, status_code) {
            (false, StatusCode::OK) => OperationStatus::Success,
            (true, StatusCode::OK) => OperationStatus::PartialError,
            (true, _) => OperationStatus::Error,
            (false, StatusCode::GATEWAY_TIMEOUT) => OperationStatus::Timeout,
            _ => OperationStatus::Success,
        };
        final_metrics.set_status(status);

        // Calculate duration
        let duration_ms = context.elapsed_ms();
        final_metrics.duration_ms = duration_ms;

        // Record metrics
        let _ = self.monitor.record(final_metrics);
    }

    /// Count top-level fields in a GraphQL response
    fn count_fields_in_response(response: &JsonValue) -> usize {
        response
            .get("data")
            .and_then(|data| data.as_object())
            .map_or(0, |obj| obj.len())
    }
}

/// Helper function to inject operation metrics into response headers
///
/// Adds trace context headers so clients can correlate requests
pub fn inject_trace_headers(
    headers: &mut HeaderMap,
    metrics_context: &OperationMetricsContext,
) -> Result<(), &'static str> {
    let metrics = metrics_context.metrics();

    // Inject W3C traceparent header
    let traceparent = format!("00-{}-{}-01", metrics.trace_id, metrics.span_id);
    if let Ok(header_value) = traceparent.parse() {
        headers.insert("traceparent", header_value);
    }

    // Inject tracestate if present
    if !metrics.tracestate.is_empty() {
        if let Ok(header_value) = metrics.tracestate.parse() {
            headers.insert("tracestate", header_value);
        }
    }

    // Inject operation ID for correlation
    if let Ok(header_value) = metrics.operation_id.parse() {
        headers.insert("x-operation-id", header_value);
    }

    // Inject request ID if present
    if let Some(req_id) = &metrics.request_id {
        if let Ok(header_value) = req_id.parse() {
            headers.insert("x-request-id", header_value);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::operation_metrics::GraphQLOperationType;
    use crate::http::operation_monitor::OperationMonitorConfig;

    #[test]
    fn test_context_creation() {
        let headers = HeaderMap::new();
        let context =
            OperationMetricsContext::from_request("query { user { id } }", None, &headers);

        assert!(!context.operation_id.is_empty());
        assert_eq!(context.metrics.operation_type, GraphQLOperationType::Query);
        assert!(context.metrics.query_length > 0);
    }

    #[test]
    fn test_trace_context_extraction() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "traceparent",
            "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01"
                .parse()
                .unwrap(),
        );
        headers.insert("tracestate", "vendorname=opaquevalue".parse().unwrap());
        headers.insert("x-request-id", "req-12345".parse().unwrap());

        let context = OperationMetricsContext::from_request(
            "mutation UpdateUser { updateUser { id } }",
            None,
            &headers,
        );

        assert_eq!(
            context.metrics.trace_id,
            "4bf92f3577b34da6a3ce929d0e0e4736"
        );
        assert_eq!(context.metrics.parent_span_id, Some("00f067aa0ba902b7".to_string()));
        assert_eq!(context.metrics.request_id, Some("req-12345".to_string()));
    }

    #[test]
    fn test_operation_type_detection() {
        let headers = HeaderMap::new();

        let query_context =
            OperationMetricsContext::from_request("query GetUser { user { id } }", None, &headers);
        assert_eq!(query_context.metrics.operation_type, GraphQLOperationType::Query);

        let mutation_context = OperationMetricsContext::from_request(
            "mutation UpdateUser { updateUser { id } }",
            None,
            &headers,
        );
        assert_eq!(
            mutation_context.metrics.operation_type,
            GraphQLOperationType::Mutation
        );

        let subscription_context = OperationMetricsContext::from_request(
            "subscription OnUpdate { userUpdated { id } }",
            None,
            &headers,
        );
        assert_eq!(
            subscription_context.metrics.operation_type,
            GraphQLOperationType::Subscription
        );
    }

    #[test]
    fn test_variables_counting() {
        let headers = HeaderMap::new();
        let variables = serde_json::json!({
            "id": "user-123",
            "name": "John",
            "email": "john@example.com"
        });

        let context = OperationMetricsContext::from_request(
            "query GetUser($id: ID!, $name: String) { user { id } }",
            Some(&variables),
            &headers,
        );

        assert_eq!(context.metrics.variables_count, 3);
    }

    #[test]
    fn test_middleware_creation() {
        let config = OperationMonitorConfig::new();
        let monitor = Arc::new(GraphQLOperationMonitor::new(config));
        let middleware = OperationMetricsMiddleware::new(monitor);

        let headers = HeaderMap::new();
        let context = middleware.extract_metrics("query { user { id } }", None, &headers);

        assert!(!context.operation_id.is_empty());
    }

    #[test]
    fn test_response_field_counting() {
        let response = serde_json::json!({
            "data": {
                "user": { "id": "1", "name": "John" },
                "posts": [{ "id": "p1" }, { "id": "p2" }]
            }
        });

        let count = OperationMetricsMiddleware::count_fields_in_response(&response);
        assert_eq!(count, 2); // user, posts
    }

    #[test]
    fn test_traceparent_parsing() {
        let (trace_id, parent_span_id) = OperationMetricsContext::parse_traceparent(
            "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01",
        );

        assert_eq!(trace_id, "4bf92f3577b34da6a3ce929d0e0e4736");
        assert_eq!(parent_span_id, Some("00f067aa0ba902b7".to_string()));
    }

    #[test]
    fn test_elapsed_time() {
        let headers = HeaderMap::new();
        let context = OperationMetricsContext::from_request("query { user { id } }", None, &headers);

        std::thread::sleep(std::time::Duration::from_millis(10));

        let elapsed = context.elapsed_ms();
        assert!(elapsed > 10.0);
    }

    #[test]
    fn test_inject_trace_headers() {
        let mut headers = HeaderMap::new();
        let request_headers = HeaderMap::new();

        let context = OperationMetricsContext::from_request("query { user { id } }", None, &request_headers);

        let result = inject_trace_headers(&mut headers, &context);
        assert!(result.is_ok());

        assert!(headers.contains_key("traceparent"));
        assert!(headers.contains_key("x-operation-id"));
    }

    #[test]
    fn test_response_recording_success() {
        let config = OperationMonitorConfig::new();
        let monitor = Arc::new(GraphQLOperationMonitor::new(config));
        let middleware = OperationMetricsMiddleware::new(monitor.clone());

        let headers = HeaderMap::new();
        let mut context = middleware.extract_metrics("query { user { id } }", None, &headers);

        let response = serde_json::json!({
            "data": {
                "user": { "id": "1", "name": "John" }
            }
        });

        middleware.record_operation(&mut context, StatusCode::OK, &response, false);

        // Check that operation was recorded
        assert!(monitor.total_operations_recorded() > 0);
    }

    #[test]
    fn test_response_recording_with_errors() {
        let config = OperationMonitorConfig::new();
        let monitor = Arc::new(GraphQLOperationMonitor::new(config));
        let middleware = OperationMetricsMiddleware::new(monitor.clone());

        let headers = HeaderMap::new();
        let mut context = middleware.extract_metrics("query { user { id } }", None, &headers);

        let response = serde_json::json!({
            "errors": [
                { "message": "User not found" }
            ]
        });

        middleware.record_operation(&mut context, StatusCode::OK, &response, true);

        assert!(monitor.total_operations_recorded() > 0);
    }

    #[test]
    fn test_context_isolation() {
        let headers1 = HeaderMap::new();
        let headers2 = HeaderMap::new();

        let context1 = OperationMetricsContext::from_request("query { user { id } }", None, &headers1);
        let context2 = OperationMetricsContext::from_request("mutation { updateUser { id } }", None, &headers2);

        // Contexts should be independent
        assert_ne!(context1.operation_id, context2.operation_id);
        assert_ne!(
            context1.metrics.operation_type,
            context2.metrics.operation_type
        );
    }
}
