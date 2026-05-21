//! Query execution tracing and observability instrumentation.
//!
//! Provides structured tracing for query compilation, validation, and execution phases.
//! Integrates with the tracing crate for OpenTelemetry-compatible span collection.

use std::time::Instant;

use serde::{Deserialize, Serialize};
use tracing::{Level, debug, span, warn};

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
/// ```rust
/// use fraiseql_core::runtime::query_tracing::QueryTraceBuilder;
///
/// let mut builder = QueryTraceBuilder::new("query_123", "{ user { id name } }");
///
/// // Record parse phase (2.5ms = 2500 microseconds)
/// builder.record_phase_success("parse", 2500);
///
/// // Record validate phase (3ms = 3000 microseconds)
/// builder.record_phase_success("validate", 3000);
///
/// // Record execute phase (7ms = 7000 microseconds)
/// builder.record_phase_success("execute", 7000);
///
/// // Finalize trace with result count
/// let trace = builder.finish(true, None, Some(42))?;
/// assert_eq!(trace.success, true);
/// assert_eq!(trace.result_count, Some(42));
/// println!("Query took {} microseconds", trace.total_duration_us);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[must_use = "call .finish() to construct the final value"]
pub struct QueryTraceBuilder {
    pub(crate) query_id: String,
    pub(crate) query:    String,
    pub(crate) phases:   Vec<QueryPhaseSpan>,
    start:               Instant,
}

impl QueryTraceBuilder {
    /// Create new query trace builder.
    ///
    /// # Arguments
    ///
    /// * `query_id` - Unique ID for query correlation
    /// * `query` - Query string (will be truncated if very long)
    pub fn new(query_id: &str, query: &str) -> Self {
        let query_str = if query.len() > 500 {
            format!("{}...", &query[..500])
        } else {
            query.to_string()
        };

        Self {
            query_id: query_id.to_string(),
            query:    query_str,
            phases:   Vec::new(),
            start:    Instant::now(),
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

        debug!(phase = phase_name, duration_us = duration_us, "Query phase completed");
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
    /// # Errors
    ///
    /// Currently infallible; reserved for future extension (e.g., sink flush failures).
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
        let total_duration_us = u64::try_from(self.start.elapsed().as_micros()).unwrap_or(u64::MAX);

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
    #[must_use]
    pub fn query_id(&self) -> &str {
        &self.query_id
    }

    /// Get current elapsed time in microseconds.
    #[must_use]
    pub fn elapsed_us(&self) -> u64 {
        u64::try_from(self.start.elapsed().as_micros()).unwrap_or(u64::MAX)
    }
}

impl QueryExecutionTrace {
    /// Get average phase duration in microseconds.
    #[must_use]
    pub fn average_phase_duration_us(&self) -> u64 {
        if self.phases.is_empty() {
            0
        } else {
            self.phases.iter().map(|p| p.duration_us).sum::<u64>() / self.phases.len() as u64
        }
    }

    /// Get slowest phase.
    #[must_use]
    pub fn slowest_phase(&self) -> Option<&QueryPhaseSpan> {
        self.phases.iter().max_by_key(|p| p.duration_us)
    }

    /// Get trace as log-friendly string.
    ///
    /// Suitable for structured logging or monitoring dashboards.
    #[must_use]
    pub fn to_log_string(&self) -> String {
        let status = if self.success { "success" } else { "error" };
        let phases_str = self
            .phases
            .iter()
            .map(|p| format!("{}={}us", p.phase, p.duration_us))
            .collect::<Vec<_>>()
            .join(" ");

        let error_str = self.error.as_ref().map(|e| format!(" error={}", e)).unwrap_or_default();

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
    span!(Level::DEBUG, "query_phase", phase = phase_name, query_id = query_id)
}

/// Truncate query string to specified length.
///
/// Useful for logging to avoid truncating long queries.
pub(crate) fn truncate_query(query: &str, max_len: usize) -> String {
    if query.len() > max_len {
        format!("{}...", &query[..max_len])
    } else {
        query.to_string()
    }
}
