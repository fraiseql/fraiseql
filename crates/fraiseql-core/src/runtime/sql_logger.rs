//! Structured SQL query logging and tracing.
//!
//! Provides comprehensive SQL query logging with:
//! - Query parameters (sanitized for security)
//! - Execution timing
//! - Result metrics (rows affected)
//! - Error tracking
//! - Performance statistics

use std::time::Instant;

use serde::{Deserialize, Serialize};
use tracing::{Level, debug, span, warn};

/// SQL query log entry with execution details.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SqlQueryLog {
    /// Query ID for correlation
    pub query_id: String,

    /// SQL statement (truncated for very large queries)
    pub sql: String,

    /// Bound parameters count
    pub param_count: usize,

    /// Execution time in microseconds
    pub duration_us: u64,

    /// Number of rows affected/returned
    pub rows_affected: Option<usize>,

    /// Whether query executed successfully
    pub success: bool,

    /// Error message if query failed
    pub error: Option<String>,

    /// Database operation type (SELECT, INSERT, UPDATE, DELETE, etc.)
    pub operation: SqlOperation,

    /// Optional slow query warning threshold (microseconds)
    pub slow_threshold_us: Option<u64>,

    /// Whether this query exceeded slow threshold
    pub was_slow: bool,
}

/// SQL operation type for classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum SqlOperation {
    /// SELECT query
    Select,
    /// INSERT query
    Insert,
    /// UPDATE query
    Update,
    /// DELETE query
    Delete,
    /// Other operation (DDL, administrative)
    Other,
}

impl SqlOperation {
    /// Detect operation type from SQL string.
    #[must_use]
    pub fn from_sql(sql: &str) -> Self {
        let trimmed = sql.trim_start().to_uppercase();

        if trimmed.starts_with("SELECT") {
            SqlOperation::Select
        } else if trimmed.starts_with("INSERT") {
            SqlOperation::Insert
        } else if trimmed.starts_with("UPDATE") {
            SqlOperation::Update
        } else if trimmed.starts_with("DELETE") {
            SqlOperation::Delete
        } else {
            SqlOperation::Other
        }
    }
}

impl std::fmt::Display for SqlOperation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SqlOperation::Select => write!(f, "SELECT"),
            SqlOperation::Insert => write!(f, "INSERT"),
            SqlOperation::Update => write!(f, "UPDATE"),
            SqlOperation::Delete => write!(f, "DELETE"),
            SqlOperation::Other => write!(f, "OTHER"),
        }
    }
}

/// Builder for creating SQL query logs.
#[must_use = "call .finish_success() or .finish_error() to construct the final value"]
pub struct SqlQueryLogBuilder {
    query_id:          String,
    sql:               String,
    param_count:       usize,
    start:             Instant,
    slow_threshold_us: Option<u64>,
}

impl SqlQueryLogBuilder {
    /// Create new SQL query log builder.
    ///
    /// # Arguments
    /// * `query_id` - Unique identifier for the GraphQL query
    /// * `sql` - The SQL statement (will be truncated if > 2000 chars)
    /// * `param_count` - Number of bound parameters
    pub fn new(query_id: &str, sql: &str, param_count: usize) -> Self {
        // Char-boundary-safe: generated SQL can embed multi-byte literals from
        // user input, so a fixed byte cut could panic (audit H20 class).
        let truncated_sql = crate::utils::text::truncate_for_display(sql, 2000);

        Self {
            query_id: query_id.to_string(),
            sql: truncated_sql,
            param_count,
            start: Instant::now(),
            slow_threshold_us: None,
        }
    }

    /// Set slow query threshold in microseconds.
    pub const fn with_slow_threshold(mut self, threshold_us: u64) -> Self {
        self.slow_threshold_us = Some(threshold_us);
        self
    }

    /// Finish logging and create log entry (successful execution).
    pub fn finish_success(self, rows_affected: Option<usize>) -> SqlQueryLog {
        let duration_us = u64::try_from(self.start.elapsed().as_micros()).unwrap_or(u64::MAX);
        let was_slow = self.slow_threshold_us.is_some_and(|t| duration_us > t);

        let log = SqlQueryLog {
            query_id: self.query_id.clone(),
            sql: self.sql.clone(),
            param_count: self.param_count,
            duration_us,
            rows_affected,
            success: true,
            error: None,
            operation: SqlOperation::from_sql(&self.sql),
            slow_threshold_us: self.slow_threshold_us,
            was_slow,
        };

        if was_slow {
            warn!(
                query_id = %log.query_id,
                operation = %log.operation,
                duration_us = log.duration_us,
                threshold_us = self.slow_threshold_us.unwrap_or(0),
                "Slow SQL query detected"
            );
        } else {
            debug!(
                query_id = %log.query_id,
                operation = %log.operation,
                duration_us = log.duration_us,
                params = log.param_count,
                "SQL query executed"
            );
        }

        log
    }

    /// Finish logging and create log entry (failed execution).
    pub fn finish_error(self, error: &str) -> SqlQueryLog {
        let duration_us = u64::try_from(self.start.elapsed().as_micros()).unwrap_or(u64::MAX);

        let log = SqlQueryLog {
            query_id: self.query_id.clone(),
            sql: self.sql.clone(),
            param_count: self.param_count,
            duration_us,
            rows_affected: None,
            success: false,
            error: Some(error.to_string()),
            operation: SqlOperation::from_sql(&self.sql),
            slow_threshold_us: self.slow_threshold_us,
            was_slow: false,
        };

        warn!(
            query_id = %log.query_id,
            operation = %log.operation,
            duration_us = log.duration_us,
            error = error,
            "SQL query failed"
        );

        log
    }
}

impl SqlQueryLog {
    /// Get log entry as a formatted string suitable for logging.
    #[must_use]
    pub fn to_log_string(&self) -> String {
        if self.success {
            format!(
                "SQL {} query (query_id={}, duration_us={}, params={}, rows={:?})",
                self.operation,
                self.query_id,
                self.duration_us,
                self.param_count,
                self.rows_affected
            )
        } else {
            format!(
                "SQL {} query FAILED (query_id={}, duration_us={}, error={})",
                self.operation,
                self.query_id,
                self.duration_us,
                self.error.as_ref().unwrap_or(&"Unknown".to_string())
            )
        }
    }

    /// Check if query was slow based on threshold.
    #[must_use]
    pub const fn is_slow(&self) -> bool {
        self.was_slow
    }

    /// Get execution time in milliseconds (for human-friendly display).
    #[must_use]
    pub fn duration_ms(&self) -> f64 {
        #[allow(clippy::cast_precision_loss)]
        // Reason: duration_us is a microsecond counter used for display; f64 precision loss is
        // acceptable
        {
            self.duration_us as f64 / 1000.0
        }
    }
}

/// Create a tracing span for SQL query execution.
pub fn create_sql_span(query_id: &str, operation: SqlOperation) -> tracing::Span {
    span!(
        Level::DEBUG,
        "sql_query",
        query_id = query_id,
        operation = %operation,
    )
}
