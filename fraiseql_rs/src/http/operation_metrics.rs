//! GraphQL operation metrics for monitoring and observability (Phase 19, Commit 4.5)
//!
//! This module provides comprehensive metrics tracking for GraphQL operations (queries, mutations,
//! and subscriptions) at the HTTP handler level. Metrics include timing, error tracking, field
//! counting, and trace context linkage.
//!
//! # Overview
//!
//! Each GraphQL operation generates an `OperationMetrics` instance that captures:
//! - **Identity**: Unique operation ID and optional operation name
//! - **Type**: Query, mutation, or subscription
//! - **Timing**: Start/end times and duration in milliseconds
//! - **Trace Context**: W3C trace IDs for distributed tracing (Phase 19, Commit 2)
//! - **Execution Details**: Field count, alias count, variable count
//! - **Performance**: Response size, error count, slow operation flag
//!
//! # Example
//!
//! ```ignore
//! use fraiseql_rs::http::operation_metrics::{OperationMetrics, GraphQLOperationType};
//! use std::time::Instant;
//!
//! let mut metrics = OperationMetrics::new(
//!     "op_123".to_string(),
//!     Some("GetUser".to_string()),
//!     GraphQLOperationType::Query,
//! );
//!
//! // Simulate operation execution
//! std::thread::sleep(std::time::Duration::from_millis(50));
//!
//! // Record completion
//! metrics.finish();
//! metrics.set_response_size(2048);
//! metrics.add_field_count(5);
//!
//! // Check if slow
//! if metrics.is_slow_for_type(100.0, 500.0, 1000.0) {
//!     println!("Slow operation detected: {:?}", metrics);
//! }
//! ```

use serde::{Deserialize, Serialize};
use std::fmt;
use std::time::Instant;

/// Unique identifier for an operation instance
pub type OperationId = String;

/// GraphQL operation type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GraphQLOperationType {
    /// Query operation (read-only)
    Query,

    /// Mutation operation (write/modification)
    Mutation,

    /// Subscription operation (real-time streaming)
    Subscription,

    /// Unknown or unparseable operation type
    Unknown,
}

impl fmt::Display for GraphQLOperationType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Query => write!(f, "query"),
            Self::Mutation => write!(f, "mutation"),
            Self::Subscription => write!(f, "subscription"),
            Self::Unknown => write!(f, "unknown"),
        }
    }
}

/// Execution status of a GraphQL operation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OperationStatus {
    /// Operation completed successfully with no errors
    Success,

    /// Operation completed with some GraphQL errors but returned data
    PartialError,

    /// Operation failed with errors, no data returned
    Error,

    /// Operation exceeded time limit
    Timeout,
}

impl fmt::Display for OperationStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Success => write!(f, "success"),
            Self::PartialError => write!(f, "partial_error"),
            Self::Error => write!(f, "error"),
            Self::Timeout => write!(f, "timeout"),
        }
    }
}

/// Comprehensive metrics for a single GraphQL operation
///
/// Records all observability data for a GraphQL operation from HTTP request through
/// execution completion. Integrates with W3C Trace Context (Phase 19, Commit 2) for
/// distributed tracing.
#[derive(Debug, Clone, Serialize)]
pub struct OperationMetrics {
    /// Unique identifier for this operation instance
    pub operation_id: OperationId,

    /// Named operation name (if operation was named in query)
    pub operation_name: Option<String>,

    /// Type of GraphQL operation (query, mutation, subscription)
    pub operation_type: GraphQLOperationType,

    /// Timestamp when operation started
    #[serde(skip)]
    start_time: Instant,

    /// Timestamp when operation ended (None if still executing)
    #[serde(skip)]
    end_time: Option<Instant>,

    /// Operation duration in milliseconds (calculated from start/end times)
    pub duration_ms: f64,

    /// W3C Trace ID from traceparent header (32 hex characters)
    pub trace_id: String,

    /// Span ID for this operation within the trace (16 hex characters)
    pub span_id: String,

    /// Parent span ID from incoming traceparent header (optional)
    pub parent_span_id: Option<String>,

    /// Tracestate header value (optional, vendor-specific trace state)
    pub tracestate: String,

    /// Request ID from X-Request-ID header (for backward compatibility)
    pub request_id: Option<String>,

    /// Length of GraphQL query string in characters
    pub query_length: usize,

    /// Number of variables in the operation
    pub variables_count: usize,

    /// Size of HTTP response body in bytes
    pub response_size_bytes: usize,

    /// Final status of the operation
    pub status: OperationStatus,

    /// Number of GraphQL errors returned
    pub error_count: usize,

    /// Number of fields selected in the operation
    pub field_count: usize,

    /// Number of field aliases used
    pub alias_count: usize,

    /// Whether operation duration exceeds configured slow threshold
    pub is_slow: bool,

    /// Configured slow threshold for this operation type (in milliseconds)
    pub slow_threshold_ms: f64,
}

impl OperationMetrics {
    /// Create a new operation metrics instance
    ///
    /// # Arguments
    ///
    /// * `operation_id` - Unique identifier for this operation
    /// * `operation_name` - Optional operation name from GraphQL query
    /// * `operation_type` - Type of operation (query, mutation, subscription)
    ///
    /// # Example
    ///
    /// ```ignore
    /// let metrics = OperationMetrics::new(
    ///     "op_abc123".to_string(),
    ///     Some("GetUser".to_string()),
    ///     GraphQLOperationType::Query,
    /// );
    /// ```
    #[must_use]
    pub fn new(
        operation_id: OperationId,
        operation_name: Option<String>,
        operation_type: GraphQLOperationType,
    ) -> Self {
        Self {
            operation_id,
            operation_name,
            operation_type,
            start_time: Instant::now(),
            end_time: None,
            duration_ms: 0.0,
            trace_id: String::new(),
            span_id: String::new(),
            parent_span_id: None,
            tracestate: String::new(),
            request_id: None,
            query_length: 0,
            variables_count: 0,
            response_size_bytes: 0,
            status: OperationStatus::Success,
            error_count: 0,
            field_count: 0,
            alias_count: 0,
            is_slow: false,
            slow_threshold_ms: 100.0, // Default for queries
        }
    }

    /// Mark operation as finished and calculate duration
    ///
    /// Must be called after operation completes to finalize timing metrics.
    pub fn finish(&mut self) {
        self.end_time = Some(Instant::now());
        if let Some(end) = self.end_time {
            self.duration_ms = end.duration_since(self.start_time).as_secs_f64() * 1000.0;
        }
    }

    /// Set trace context from W3C Trace Context headers (Phase 19, Commit 2)
    ///
    /// # Arguments
    ///
    /// * `trace_id` - W3C trace ID (32 hex characters)
    /// * `span_id` - Span ID for this operation (16 hex characters)
    /// * `parent_span_id` - Parent span ID from traceparent header
    /// * `tracestate` - Optional tracestate header value
    /// * `request_id` - Optional X-Request-ID header value
    pub fn set_trace_context(
        &mut self,
        trace_id: String,
        span_id: String,
        parent_span_id: Option<String>,
        tracestate: String,
        request_id: Option<String>,
    ) {
        self.trace_id = trace_id;
        self.span_id = span_id;
        self.parent_span_id = parent_span_id;
        self.tracestate = tracestate;
        self.request_id = request_id;
    }

    /// Set the query string length
    #[allow(clippy::missing_const_for_fn)]
    pub fn set_query_length(&mut self, length: usize) {
        self.query_length = length;
    }

    /// Set the number of variables in the operation
    #[allow(clippy::missing_const_for_fn)]
    pub fn set_variables_count(&mut self, count: usize) {
        self.variables_count = count;
    }

    /// Set the response size in bytes
    #[allow(clippy::missing_const_for_fn)]
    pub fn set_response_size(&mut self, size: usize) {
        self.response_size_bytes = size;
    }

    /// Set the operation status
    #[allow(clippy::missing_const_for_fn)]
    pub fn set_status(&mut self, status: OperationStatus) {
        self.status = status;
    }

    /// Add to the error count
    #[allow(clippy::missing_const_for_fn)]
    pub fn add_error(&mut self) {
        self.error_count += 1;
    }

    /// Set error count directly
    #[allow(clippy::missing_const_for_fn)]
    pub fn set_error_count(&mut self, count: usize) {
        self.error_count = count;
    }

    /// Add to the field count
    #[allow(clippy::missing_const_for_fn)]
    pub fn add_field(&mut self) {
        self.field_count += 1;
    }

    /// Set field count directly
    #[allow(clippy::missing_const_for_fn)]
    pub fn set_field_count(&mut self, count: usize) {
        self.field_count = count;
    }

    /// Add to the alias count
    #[allow(clippy::missing_const_for_fn)]
    pub fn add_alias(&mut self) {
        self.alias_count += 1;
    }

    /// Set alias count directly
    #[allow(clippy::missing_const_for_fn)]
    pub fn set_alias_count(&mut self, count: usize) {
        self.alias_count = count;
    }

    /// Set the slow threshold and determine if operation is slow
    ///
    /// # Arguments
    ///
    /// * `threshold_ms` - Slow threshold in milliseconds for this operation
    pub fn set_slow_threshold(&mut self, threshold_ms: f64) {
        self.slow_threshold_ms = threshold_ms;
        self.is_slow = self.duration_ms > threshold_ms;
    }

    /// Determine if operation is slow based on type-specific thresholds
    ///
    /// Returns true if operation duration exceeds the threshold for its type.
    ///
    /// # Arguments
    ///
    /// * `query_threshold_ms` - Slow threshold for queries
    /// * `mutation_threshold_ms` - Slow threshold for mutations
    /// * `subscription_threshold_ms` - Slow threshold for subscriptions
    #[must_use]
    pub fn is_slow_for_type(
        &self,
        query_threshold_ms: f64,
        mutation_threshold_ms: f64,
        subscription_threshold_ms: f64,
    ) -> bool {
        let threshold = match self.operation_type {
            GraphQLOperationType::Query => query_threshold_ms,
            GraphQLOperationType::Mutation => mutation_threshold_ms,
            GraphQLOperationType::Subscription => subscription_threshold_ms,
            GraphQLOperationType::Unknown => query_threshold_ms,
        };
        self.duration_ms > threshold
    }

    /// Get the total number of operations (hits) for aggregation
    #[must_use]
    pub fn total_operations(&self) -> u64 {
        1
    }

    /// Get success indicator for aggregation (0 or 1)
    #[must_use]
    pub fn success_indicator(&self) -> u64 {
        match self.status {
            OperationStatus::Success => 1,
            OperationStatus::PartialError | OperationStatus::Error | OperationStatus::Timeout => 0,
        }
    }

    /// Get error indicator for aggregation (0 or 1)
    #[must_use]
    pub fn error_indicator(&self) -> u64 {
        match self.status {
            OperationStatus::Error | OperationStatus::Timeout => 1,
            OperationStatus::Success | OperationStatus::PartialError => 0,
        }
    }

    /// Convert metrics to JSON for serialization
    ///
    /// Automatically calculated fields like duration are included in output.
    #[must_use]
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "operation_id": self.operation_id,
            "operation_name": self.operation_name,
            "operation_type": self.operation_type.to_string(),
            "duration_ms": format!("{:.2}", self.duration_ms),
            "is_slow": self.is_slow,
            "slow_threshold_ms": format!("{:.2}", self.slow_threshold_ms),
            "trace_id": self.trace_id,
            "span_id": self.span_id,
            "parent_span_id": self.parent_span_id,
            "tracestate": self.tracestate,
            "request_id": self.request_id,
            "query_length": self.query_length,
            "variables_count": self.variables_count,
            "response_size_bytes": self.response_size_bytes,
            "status": self.status.to_string(),
            "error_count": self.error_count,
            "field_count": self.field_count,
            "alias_count": self.alias_count,
        })
    }

    /// Check if operation is still executing (no end time set)
    #[must_use]
    pub fn is_executing(&self) -> bool {
        self.end_time.is_none()
    }

    /// Get the number of seconds since operation started
    #[must_use]
    pub fn elapsed_seconds(&self) -> f64 {
        self.start_time.elapsed().as_secs_f64()
    }
}

/// Computed statistics from a collection of operation metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationStatistics {
    /// Total number of operations
    pub total_operations: u64,

    /// Number of operations that exceeded slow threshold
    pub slow_operations: u64,

    /// Percentage of operations that were slow (0-100)
    pub slow_percentage: f64,

    /// Average operation duration in milliseconds
    pub avg_duration_ms: f64,

    /// Median operation duration (P50) in milliseconds
    pub p50_duration_ms: f64,

    /// 95th percentile operation duration (P95) in milliseconds
    pub p95_duration_ms: f64,

    /// 99th percentile operation duration (P99) in milliseconds
    pub p99_duration_ms: f64,

    /// Count of successful operations
    pub successful_operations: u64,

    /// Count of failed operations
    pub failed_operations: u64,

    /// Error rate (0-100)
    pub error_rate: f64,

    /// Total response size in bytes across all operations
    pub total_response_bytes: u64,

    /// Average response size in bytes
    pub avg_response_bytes: u64,

    /// Total fields selected across all operations
    pub total_fields: u64,

    /// Average fields per operation
    pub avg_fields: u64,
}

impl OperationStatistics {
    /// Create a new empty statistics instance
    #[must_use]
    pub fn new() -> Self {
        Self {
            total_operations: 0,
            slow_operations: 0,
            slow_percentage: 0.0,
            avg_duration_ms: 0.0,
            p50_duration_ms: 0.0,
            p95_duration_ms: 0.0,
            p99_duration_ms: 0.0,
            successful_operations: 0,
            failed_operations: 0,
            error_rate: 0.0,
            total_response_bytes: 0,
            avg_response_bytes: 0,
            total_fields: 0,
            avg_fields: 0,
        }
    }

    /// Calculate statistics from a collection of metrics
    ///
    /// # Arguments
    ///
    /// * `metrics` - Slice of operation metrics to aggregate
    #[must_use]
    pub fn from_metrics(metrics: &[OperationMetrics]) -> Self {
        if metrics.is_empty() {
            return Self::new();
        }

        let total = metrics.len() as u64;
        let mut durations: Vec<f64> = metrics.iter().map(|m| m.duration_ms).collect();
        durations.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

        let slow_ops = metrics.iter().filter(|m| m.is_slow).count() as u64;
        let successful = metrics
            .iter()
            .filter(|m| m.status == OperationStatus::Success)
            .count() as u64;
        let failed = total - successful;

        let avg_duration = durations.iter().sum::<f64>() / total as f64;
        let total_response_bytes: u64 = metrics.iter().map(|m| m.response_size_bytes as u64).sum();
        let total_fields: u64 = metrics.iter().map(|m| m.field_count as u64).sum();

        Self {
            total_operations: total,
            slow_operations: slow_ops,
            slow_percentage: (slow_ops as f64 / total as f64) * 100.0,
            avg_duration_ms: avg_duration,
            p50_duration_ms: percentile(&durations, 50),
            p95_duration_ms: percentile(&durations, 95),
            p99_duration_ms: percentile(&durations, 99),
            successful_operations: successful,
            failed_operations: failed,
            error_rate: (failed as f64 / total as f64) * 100.0,
            total_response_bytes,
            avg_response_bytes: if total > 0 {
                total_response_bytes / total
            } else {
                0
            },
            total_fields,
            avg_fields: if total > 0 { total_fields / total } else { 0 },
        }
    }
}

impl Default for OperationStatistics {
    fn default() -> Self {
        Self::new()
    }
}

/// Calculate percentile from sorted values
fn percentile(sorted_values: &[f64], p: usize) -> f64 {
    if sorted_values.is_empty() {
        return 0.0;
    }

    let p = p.min(100);
    let index = (sorted_values.len() as f64 * p as f64 / 100.0) as usize;
    let index = index.min(sorted_values.len() - 1);
    sorted_values[index]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_operation_metrics_creation() {
        let metrics = OperationMetrics::new(
            "op_123".to_string(),
            Some("GetUser".to_string()),
            GraphQLOperationType::Query,
        );

        assert_eq!(metrics.operation_id, "op_123");
        assert_eq!(metrics.operation_name, Some("GetUser".to_string()));
        assert_eq!(metrics.operation_type, GraphQLOperationType::Query);
        assert_eq!(metrics.duration_ms, 0.0);
        assert_eq!(metrics.status, OperationStatus::Success);
    }

    #[test]
    fn test_operation_metrics_finish() {
        let mut metrics =
            OperationMetrics::new("op_123".to_string(), None, GraphQLOperationType::Mutation);

        // Simulate some work
        std::thread::sleep(std::time::Duration::from_millis(10));

        metrics.finish();

        assert!(metrics.duration_ms > 10.0);
        assert!(metrics.end_time.is_some());
        assert!(!metrics.is_executing());
    }

    #[test]
    fn test_trace_context_integration() {
        let mut metrics =
            OperationMetrics::new("op_123".to_string(), None, GraphQLOperationType::Query);

        metrics.set_trace_context(
            "4bf92f3577b34da6a3ce929d0e0e4736".to_string(),
            "00f067aa0ba902b7".to_string(),
            Some("parent123".to_string()),
            "vendorname=abc".to_string(),
            Some("req_456".to_string()),
        );

        assert_eq!(metrics.trace_id, "4bf92f3577b34da6a3ce929d0e0e4736");
        assert_eq!(metrics.span_id, "00f067aa0ba902b7");
        assert_eq!(metrics.parent_span_id, Some("parent123".to_string()));
        assert_eq!(metrics.request_id, Some("req_456".to_string()));
    }

    #[test]
    fn test_slow_detection_by_type() {
        let metrics =
            OperationMetrics::new("op_123".to_string(), None, GraphQLOperationType::Mutation);

        // For mutations, 450ms is slow (threshold is 500ms)
        assert!(!metrics.is_slow_for_type(100.0, 500.0, 1000.0)); // 0ms < 500ms

        // Simulate slow mutation
        let mut slow_metrics = metrics;
        slow_metrics.duration_ms = 600.0;

        // For mutations, 600ms is slow (threshold is 500ms)
        assert!(slow_metrics.is_slow_for_type(100.0, 500.0, 1000.0));
    }

    #[test]
    fn test_operation_statistics_calculation() {
        let metrics = vec![
            {
                let mut m =
                    OperationMetrics::new("op_1".to_string(), None, GraphQLOperationType::Query);
                m.duration_ms = 50.0;
                m.response_size_bytes = 1024;
                m.field_count = 5;
                m.set_status(OperationStatus::Success);
                m
            },
            {
                let mut m =
                    OperationMetrics::new("op_2".to_string(), None, GraphQLOperationType::Query);
                m.duration_ms = 100.0;
                m.response_size_bytes = 2048;
                m.field_count = 10;
                m.set_status(OperationStatus::Success);
                m
            },
            {
                let mut m =
                    OperationMetrics::new("op_3".to_string(), None, GraphQLOperationType::Query);
                m.duration_ms = 150.0;
                m.response_size_bytes = 1536;
                m.field_count = 7;
                m.set_status(OperationStatus::Error);
                m
            },
        ];

        let stats = OperationStatistics::from_metrics(&metrics);

        assert_eq!(stats.total_operations, 3);
        assert_eq!(stats.successful_operations, 2);
        assert_eq!(stats.failed_operations, 1);
        assert!((stats.avg_duration_ms - 100.0).abs() < 0.1); // (50+100+150)/3 = 100
        assert_eq!(stats.total_response_bytes, 4608); // 1024+2048+1536
        assert_eq!(stats.total_fields, 22); // 5+10+7
    }

    #[test]
    fn test_percentile_calculation() {
        let mut metrics = vec![];
        for i in 1..=100 {
            let mut m =
                OperationMetrics::new(format!("op_{}", i), None, GraphQLOperationType::Query);
            m.duration_ms = i as f64;
            metrics.push(m);
        }

        let stats = OperationStatistics::from_metrics(&metrics);

        assert!((stats.p50_duration_ms - 50.0).abs() < 1.0);
        assert!((stats.p95_duration_ms - 95.0).abs() < 1.0);
        assert!((stats.p99_duration_ms - 99.0).abs() < 1.0);
    }

    #[test]
    fn test_to_json_serialization() {
        let mut metrics = OperationMetrics::new(
            "op_123".to_string(),
            Some("GetUser".to_string()),
            GraphQLOperationType::Query,
        );
        metrics.duration_ms = 45.2;
        metrics.set_response_size(2048);
        metrics.set_field_count(7);
        metrics.set_status(OperationStatus::Success);

        let json = metrics.to_json();

        assert_eq!(json["operation_id"], "op_123");
        assert_eq!(json["operation_name"], "GetUser");
        assert_eq!(json["operation_type"], "query");
        assert_eq!(json["response_size_bytes"], 2048);
        assert_eq!(json["field_count"], 7);
    }

    #[test]
    fn test_operation_type_display() {
        assert_eq!(GraphQLOperationType::Query.to_string(), "query");
        assert_eq!(GraphQLOperationType::Mutation.to_string(), "mutation");
        assert_eq!(
            GraphQLOperationType::Subscription.to_string(),
            "subscription"
        );
        assert_eq!(GraphQLOperationType::Unknown.to_string(), "unknown");
    }

    #[test]
    fn test_operation_status_display() {
        assert_eq!(OperationStatus::Success.to_string(), "success");
        assert_eq!(OperationStatus::PartialError.to_string(), "partial_error");
        assert_eq!(OperationStatus::Error.to_string(), "error");
        assert_eq!(OperationStatus::Timeout.to_string(), "timeout");
    }

    #[test]
    fn test_empty_statistics() {
        let stats = OperationStatistics::from_metrics(&[]);

        assert_eq!(stats.total_operations, 0);
        assert_eq!(stats.slow_operations, 0);
        assert_eq!(stats.avg_duration_ms, 0.0);
    }
}
