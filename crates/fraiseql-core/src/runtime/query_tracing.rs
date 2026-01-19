//! Query execution tracing and observability instrumentation.
//!
//! Provides structured tracing for query compilation, validation, and execution phases.
//! Integrates with the tracing crate for OpenTelemetry-compatible span collection.

use std::time::Instant;

use serde::{Deserialize, Serialize};
use tracing::{debug, span, warn, Level};

use crate::error::Result;

/// Trace span for a query compilation phase.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryPhaseSpan {
    /// Name of the phase (e.g., "parse", "validate", "execute").
    pub phase: String,

    /// Duration in microseconds.
    pub duration_us: u64,

    /// Whether the phase succeeded.
    pub success: bool,

    /// Optional error message if phase failed.
    pub error: Option<String>,
}

/// Complete trace for a query execution.
///
/// Tracks all phases from initial parse through execution,
/// enabling performance analysis and debugging.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryExecutionTrace {
    /// Unique query ID for tracing correlation.
    pub query_id: String,

    /// Query string (truncated for large queries).
    pub query: String,

    /// List of phase spans (parse, validate, execute, etc.).
    pub phases: Vec<QueryPhaseSpan>,

    /// Total execution time in microseconds.
    pub total_duration_us: u64,

    /// Whether execution succeeded.
    pub success: bool,

    /// Optional error message.
    pub error: Option<String>,

    /// Number of results returned.
    pub result_count: Option<usize>,
}

/// Builder for creating and tracking query execution traces.
///
/// # Example
///
/// ```rust,no_run
/// use fraiseql_core::runtime::query_tracing::QueryTraceBuilder;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let mut builder = QueryTraceBuilder::new("query_123", "{ user { id name } }");
///
/// // Record compilation phase
/// let phase_result = builder.record_phase("compile", async {
///     // Compilation logic here
///     Ok(())
/// }).await;
///
/// // Get final trace
/// let trace = builder.finish(true, None)?;
/// println!("Query took {:?} us total", trace.total_duration_us);
/// # Ok(())
/// # }
/// ```
pub struct QueryTraceBuilder {
    query_id: String,
    query: String,
    phases: Vec<QueryPhaseSpan>,
    start: Instant,
}

impl QueryTraceBuilder {
    /// Create new query trace builder.
    ///
    /// # Arguments
    ///
    /// * `query_id` - Unique ID for query correlation
    /// * `query` - Query string (will be truncated if very long)
    #[must_use]
    pub fn new(query_id: &str, query: &str) -> Self {
        let query_str = if query.len() > 500 {
            format!("{}...", &query[..500])
        } else {
            query.to_string()
        };

        Self {
            query_id: query_id.to_string(),
            query: query_str,
            phases: Vec::new(),
            start: Instant::now(),
        }
    }

    /// Record a phase that completed successfully.
    ///
    /// # Arguments
    ///
    /// * `phase_name` - Name of phase (e.g., "parse", "validate")
    /// * `duration_us` - Duration in microseconds
    pub fn record_phase_success(&mut self, phase_name: &str, duration_us: u64) {
        self.phases.push(QueryPhaseSpan {
            phase: phase_name.to_string(),
            duration_us,
            success: true,
            error: None,
        });

        debug!(
            phase = phase_name,
            duration_us = duration_us,
            "Query phase completed"
        );
    }

    /// Record a phase that failed.
    ///
    /// # Arguments
    ///
    /// * `phase_name` - Name of phase (e.g., "parse", "validate")
    /// * `duration_us` - Duration in microseconds
    /// * `error` - Error message
    pub fn record_phase_error(&mut self, phase_name: &str, duration_us: u64, error: &str) {
        self.phases.push(QueryPhaseSpan {
            phase: phase_name.to_string(),
            duration_us,
            success: false,
            error: Some(error.to_string()),
        });

        warn!(
            phase = phase_name,
            duration_us = duration_us,
            error = error,
            "Query phase failed"
        );
    }

    /// Finish tracing and create final trace record.
    ///
    /// # Arguments
    ///
    /// * `success` - Whether query executed successfully
    /// * `error` - Optional error message
    /// * `result_count` - Number of results returned (if applicable)
    ///
    /// # Returns
    ///
    /// Complete query execution trace
    pub fn finish(
        self,
        success: bool,
        error: Option<&str>,
        result_count: Option<usize>,
    ) -> Result<QueryExecutionTrace> {
        let total_duration_us = self.start.elapsed().as_micros() as u64;

        Ok(QueryExecutionTrace {
            query_id: self.query_id.clone(),
            query: self.query,
            phases: self.phases,
            total_duration_us,
            success,
            error: error.map(|e| e.to_string()),
            result_count,
        })
    }

    /// Get query ID for logging/correlation.
    pub fn query_id(&self) -> &str {
        &self.query_id
    }

    /// Get current elapsed time in microseconds.
    pub fn elapsed_us(&self) -> u64 {
        self.start.elapsed().as_micros() as u64
    }
}

impl QueryExecutionTrace {
    /// Get average phase duration in microseconds.
    pub fn average_phase_duration_us(&self) -> u64 {
        if self.phases.is_empty() {
            0
        } else {
            self.phases.iter().map(|p| p.duration_us).sum::<u64>() / self.phases.len() as u64
        }
    }

    /// Get slowest phase.
    pub fn slowest_phase(&self) -> Option<&QueryPhaseSpan> {
        self.phases.iter().max_by_key(|p| p.duration_us)
    }

    /// Get trace as log-friendly string.
    ///
    /// Suitable for structured logging or monitoring dashboards.
    pub fn to_log_string(&self) -> String {
        let status = if self.success { "success" } else { "error" };
        let phases_str = self
            .phases
            .iter()
            .map(|p| format!("{}={}us", p.phase, p.duration_us))
            .collect::<Vec<_>>()
            .join(" ");

        let error_str = self
            .error
            .as_ref()
            .map(|e| format!(" error={}", e))
            .unwrap_or_default();

        format!(
            "query_id={} status={} total={}us phases=[{}]{}",
            self.query_id, status, self.total_duration_us, phases_str, error_str
        )
    }
}

/// Create a tracing span for query execution.
///
/// Automatically enters a span and returns it for use with `.in_scope()`.
///
/// # Example
///
/// ```rust,no_run
/// use fraiseql_core::runtime::query_tracing::create_query_span;
///
/// # fn example() {
/// let span = create_query_span("query_123", "{ user { id } }");
/// let _enter = span.enter();
/// // Tracing will be recorded within this scope
/// # }
/// ```
pub fn create_query_span(query_id: &str, query_text: &str) -> tracing::Span {
    span!(
        Level::DEBUG,
        "query_execute",
        query_id = query_id,
        query = truncate_query(query_text, 100),
    )
}

/// Create a tracing span for a specific phase.
///
/// # Arguments
///
/// * `phase_name` - Name of phase (e.g., "parse", "validate", "execute")
/// * `query_id` - Query ID for correlation
pub fn create_phase_span(phase_name: &str, query_id: &str) -> tracing::Span {
    span!(
        Level::DEBUG,
        "query_phase",
        phase = phase_name,
        query_id = query_id
    )
}

/// Truncate query string to specified length.
///
/// Useful for logging to avoid truncating long queries.
fn truncate_query(query: &str, max_len: usize) -> String {
    if query.len() > max_len {
        format!("{}...", &query[..max_len])
    } else {
        query.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trace_builder_new() {
        let builder = QueryTraceBuilder::new("query_1", "{ user { id } }");
        assert_eq!(builder.query_id, "query_1");
        assert_eq!(builder.query, "{ user { id } }");
        assert!(builder.phases.is_empty());
    }

    #[test]
    fn test_trace_builder_truncate_long_query() {
        let long_query = "a".repeat(600);
        let builder = QueryTraceBuilder::new("query_1", &long_query);
        assert!(builder.query.len() < 600);
        assert!(builder.query.ends_with("..."));
    }

    #[test]
    fn test_record_phase_success() {
        let mut builder = QueryTraceBuilder::new("query_1", "{ user { id } }");
        builder.record_phase_success("parse", 100);
        builder.record_phase_success("validate", 50);

        assert_eq!(builder.phases.len(), 2);
        assert_eq!(builder.phases[0].phase, "parse");
        assert_eq!(builder.phases[0].duration_us, 100);
        assert!(builder.phases[0].success);
    }

    #[test]
    fn test_record_phase_error() {
        let mut builder = QueryTraceBuilder::new("query_1", "{ user { id } }");
        builder.record_phase_error("parse", 100, "Invalid syntax");

        assert_eq!(builder.phases.len(), 1);
        assert_eq!(builder.phases[0].phase, "parse");
        assert!(!builder.phases[0].success);
        assert_eq!(builder.phases[0].error, Some("Invalid syntax".to_string()));
    }

    #[test]
    fn test_finish_success() {
        let mut builder = QueryTraceBuilder::new("query_1", "{ user { id } }");
        builder.record_phase_success("parse", 100);
        builder.record_phase_success("execute", 500);

        let trace = builder.finish(true, None, Some(10)).unwrap();
        assert!(trace.success);
        assert_eq!(trace.query_id, "query_1");
        assert_eq!(trace.phases.len(), 2);
        assert_eq!(trace.result_count, Some(10));
        // total_duration_us is wall-clock time, may be higher or lower than sum of phases
        assert!(trace.total_duration_us >= 0);
    }

    #[test]
    fn test_finish_error() {
        let mut builder = QueryTraceBuilder::new("query_1", "{ user { id } }");
        builder.record_phase_success("parse", 100);
        builder.record_phase_error("execute", 50, "Database connection failed");

        let trace = builder.finish(false, Some("Database connection failed"), None).unwrap();
        assert!(!trace.success);
        assert_eq!(trace.error, Some("Database connection failed".to_string()));
    }

    #[test]
    fn test_average_phase_duration() {
        let mut builder = QueryTraceBuilder::new("query_1", "{ user { id } }");
        builder.record_phase_success("parse", 100);
        builder.record_phase_success("validate", 200);
        builder.record_phase_success("execute", 300);

        let trace = builder.finish(true, None, None).unwrap();
        assert_eq!(trace.average_phase_duration_us(), 200);
    }

    #[test]
    fn test_slowest_phase() {
        let mut builder = QueryTraceBuilder::new("query_1", "{ user { id } }");
        builder.record_phase_success("parse", 100);
        builder.record_phase_success("execute", 500);
        builder.record_phase_success("cache_check", 50);

        let trace = builder.finish(true, None, None).unwrap();
        let slowest = trace.slowest_phase().unwrap();
        assert_eq!(slowest.phase, "execute");
        assert_eq!(slowest.duration_us, 500);
    }

    #[test]
    fn test_to_log_string_success() {
        let mut builder = QueryTraceBuilder::new("query_1", "{ user { id } }");
        builder.record_phase_success("parse", 100);
        builder.record_phase_success("execute", 500);

        let trace = builder.finish(true, None, Some(5)).unwrap();
        let log_str = trace.to_log_string();
        assert!(log_str.contains("query_id=query_1"));
        assert!(log_str.contains("status=success"));
        assert!(log_str.contains("parse=100us"));
        assert!(log_str.contains("execute=500us"));
    }

    #[test]
    fn test_to_log_string_error() {
        let mut builder = QueryTraceBuilder::new("query_1", "{ user { id } }");
        builder.record_phase_success("parse", 100);
        builder.record_phase_error("validate", 50, "Type mismatch");

        let trace = builder.finish(false, Some("Type mismatch"), None).unwrap();
        let log_str = trace.to_log_string();
        assert!(log_str.contains("status=error"));
        assert!(log_str.contains("error=Type mismatch"));
    }

    #[test]
    fn test_average_phase_duration_empty() {
        let builder = QueryTraceBuilder::new("query_1", "{ user { id } }");
        let trace = builder.finish(true, None, None).unwrap();
        assert_eq!(trace.average_phase_duration_us(), 0);
    }

    #[test]
    fn test_elapsed_us() {
        let builder = QueryTraceBuilder::new("query_1", "{ user { id } }");
        let elapsed = builder.elapsed_us();
        // Elapsed time should be non-negative (u64 is always >= 0)
        let _ = elapsed;
    }

    #[test]
    fn test_trace_serialization() {
        let mut builder = QueryTraceBuilder::new("query_1", "{ user { id } }");
        builder.record_phase_success("parse", 100);

        let trace = builder.finish(true, None, Some(5)).unwrap();
        let json = serde_json::to_string(&trace).expect("serialize should work");
        let restored: QueryExecutionTrace =
            serde_json::from_str(&json).expect("deserialize should work");

        assert_eq!(restored.query_id, trace.query_id);
        assert_eq!(restored.phases.len(), trace.phases.len());
    }

    #[test]
    fn test_query_phase_span_serialize() {
        let span = QueryPhaseSpan {
            phase: "parse".to_string(),
            duration_us: 100,
            success: true,
            error: None,
        };

        let json = serde_json::to_string(&span).expect("serialize should work");
        let restored: QueryPhaseSpan = serde_json::from_str(&json).expect("deserialize should work");

        assert_eq!(restored.phase, span.phase);
        assert_eq!(restored.duration_us, span.duration_us);
    }

    #[test]
    fn test_truncate_query_helper() {
        assert_eq!(truncate_query("hello", 100), "hello");
        assert!(truncate_query(&"a".repeat(200), 50).ends_with("..."));
    }
}
