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
        let truncated_sql = if sql.len() > 2000 {
            format!("{}...", &sql[..2000])
        } else {
            sql.to_string()
        };

        Self {
            query_id: query_id.to_string(),
            sql: truncated_sql,
            param_count,
            start: Instant::now(),
            slow_threshold_us: None,
        }
    }

    /// Set slow query threshold in microseconds.
    pub fn with_slow_threshold(mut self, threshold_us: u64) -> Self {
        self.slow_threshold_us = Some(threshold_us);
        self
    }

    /// Finish logging and create log entry (successful execution).
    pub fn finish_success(self, rows_affected: Option<usize>) -> SqlQueryLog {
        let duration_us = self.start.elapsed().as_micros() as u64;
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
        let duration_us = self.start.elapsed().as_micros() as u64;

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
    pub fn is_slow(&self) -> bool {
        self.was_slow
    }

    /// Get execution time in milliseconds (for human-friendly display).
    pub fn duration_ms(&self) -> f64 {
        self.duration_us as f64 / 1000.0
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

#[cfg(test)]
mod tests {
    use std::{thread, time::Duration};

    use super::*;

    #[test]
    fn test_sql_operation_detection() {
        assert_eq!(SqlOperation::from_sql("SELECT * FROM users"), SqlOperation::Select);
        assert_eq!(SqlOperation::from_sql("  select id from users"), SqlOperation::Select);
        assert_eq!(SqlOperation::from_sql("INSERT INTO users VALUES (1)"), SqlOperation::Insert);
        assert_eq!(SqlOperation::from_sql("UPDATE users SET id=1"), SqlOperation::Update);
        assert_eq!(SqlOperation::from_sql("DELETE FROM users"), SqlOperation::Delete);
        assert_eq!(SqlOperation::from_sql("CREATE TABLE users (id INT)"), SqlOperation::Other);
    }

    #[test]
    fn test_sql_operation_display() {
        assert_eq!(SqlOperation::Select.to_string(), "SELECT");
        assert_eq!(SqlOperation::Insert.to_string(), "INSERT");
        assert_eq!(SqlOperation::Update.to_string(), "UPDATE");
        assert_eq!(SqlOperation::Delete.to_string(), "DELETE");
        assert_eq!(SqlOperation::Other.to_string(), "OTHER");
    }

    #[test]
    fn test_builder_success() {
        let builder = SqlQueryLogBuilder::new("query_1", "SELECT * FROM users", 0);
        let log = builder.finish_success(Some(10));

        assert!(log.success);
        assert_eq!(log.query_id, "query_1");
        assert_eq!(log.operation, SqlOperation::Select);
        assert_eq!(log.rows_affected, Some(10));
        assert!(log.error.is_none());
        // duration_us is wall-clock time, may vary depending on system speed
    }

    #[test]
    fn test_builder_error() {
        let builder = SqlQueryLogBuilder::new("query_1", "SELECT * FROM nonexistent", 0);
        let log = builder.finish_error("Table not found");

        assert!(!log.success);
        assert_eq!(log.error, Some("Table not found".to_string()));
        assert!(log.rows_affected.is_none());
    }

    #[test]
    fn test_query_truncation() {
        let long_query = "a".repeat(3000);
        let builder = SqlQueryLogBuilder::new("query_1", &long_query, 0);
        let log = builder.finish_success(None);

        assert!(log.sql.len() < 3000);
        assert!(log.sql.ends_with("..."));
    }

    #[test]
    fn test_slow_query_detection() {
        let builder =
            SqlQueryLogBuilder::new("query_1", "SELECT * FROM users", 0).with_slow_threshold(100);

        let log = builder.finish_success(Some(5));

        // Query should be considered fast (runs much faster than 100 us typically)
        assert!(!log.was_slow);
    }

    #[test]
    fn test_slow_query_warning() {
        let builder =
            SqlQueryLogBuilder::new("query_1", "SELECT * FROM users", 0).with_slow_threshold(1);

        // Simulate slow query by sleeping
        thread::sleep(Duration::from_micros(100));
        let log = builder.finish_success(Some(5));

        assert!(log.was_slow);
    }

    #[test]
    fn test_log_string_success() {
        let builder = SqlQueryLogBuilder::new("query_1", "SELECT * FROM users", 5);
        let log = builder.finish_success(Some(10));

        let log_str = log.to_log_string();
        assert!(log_str.contains("SELECT"));
        assert!(log_str.contains("query_1"));
        assert!(log_str.contains("params=5"));
        assert!(log_str.contains("rows=Some(10)"));
    }

    #[test]
    fn test_log_string_error() {
        let builder = SqlQueryLogBuilder::new("query_1", "SELECT * FROM users", 0);
        let log = builder.finish_error("Connection timeout");

        let log_str = log.to_log_string();
        assert!(log_str.contains("FAILED"));
        assert!(log_str.contains("Connection timeout"));
    }

    #[test]
    fn test_duration_ms() {
        let builder = SqlQueryLogBuilder::new("query_1", "SELECT * FROM users", 0);
        thread::sleep(Duration::from_millis(10));
        let log = builder.finish_success(None);

        let ms = log.duration_ms();
        assert!(ms >= 10.0);
    }

    #[test]
    fn test_serialization() {
        let builder = SqlQueryLogBuilder::new("query_1", "SELECT * FROM users", 3);
        let log = builder.finish_success(Some(25));

        let json = serde_json::to_string(&log).expect("serialize should work");
        let restored: SqlQueryLog = serde_json::from_str(&json).expect("deserialize should work");

        assert_eq!(restored.query_id, log.query_id);
        assert_eq!(restored.operation, log.operation);
        assert_eq!(restored.rows_affected, log.rows_affected);
    }

    #[test]
    fn test_all_operations() {
        let operations = vec![
            SqlOperation::Select,
            SqlOperation::Insert,
            SqlOperation::Update,
            SqlOperation::Delete,
            SqlOperation::Other,
        ];

        for op in operations {
            let builder = SqlQueryLogBuilder::new("query_1", "SELECT 1", 0);
            let mut log = builder.finish_success(None);
            log.operation = op;

            assert_eq!(log.operation, op);
            let log_str = log.to_log_string();
            assert!(log_str.contains(&op.to_string()));
        }
    }

    #[test]
    fn test_param_count() {
        let builder =
            SqlQueryLogBuilder::new("query_1", "SELECT * FROM users WHERE id = ? AND name = ?", 2);
        let log = builder.finish_success(Some(1));

        assert_eq!(log.param_count, 2);
        assert!(log.to_log_string().contains("params=2"));
    }
}
