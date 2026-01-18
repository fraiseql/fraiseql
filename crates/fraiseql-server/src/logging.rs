//! Structured JSON logging for observability.
//!
//! Provides structured logging output in JSON format for easier parsing
//! and integration with log aggregation systems (ELK, Splunk, `DataDog`, etc).
//!
//! # Features
//!
//! - JSON-formatted log output for every log entry
//! - Request context tracking (`request_id`, operation, `user_id`)
//! - Performance metrics in logs (duration, query complexity)
//! - Severity levels (trace, debug, info, warn, error)
//! - Automatic timestamp and source location

use std::{fmt, sync::Arc};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Unique identifier for request context.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct RequestId(Uuid);

impl RequestId {
    /// Generate new random request ID.
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Create from existing UUID.
    #[must_use]
    pub fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }
}

impl Default for RequestId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for RequestId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Request context for structured logging.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestContext {
    /// Unique request identifier
    pub request_id: RequestId,

    /// GraphQL operation name (query, mutation, subscription)
    pub operation: Option<String>,

    /// User identifier (if authenticated)
    pub user_id: Option<String>,

    /// Client IP address
    pub client_ip: Option<String>,

    /// API version
    pub api_version: Option<String>,
}

impl RequestContext {
    /// Create new request context.
    #[must_use]
    pub fn new() -> Self {
        Self {
            request_id:  RequestId::new(),
            operation:   None,
            user_id:     None,
            client_ip:   None,
            api_version: None,
        }
    }

    /// Set operation name.
    #[must_use]
    pub fn with_operation(mut self, operation: String) -> Self {
        self.operation = Some(operation);
        self
    }

    /// Set user ID.
    #[must_use]
    pub fn with_user_id(mut self, user_id: String) -> Self {
        self.user_id = Some(user_id);
        self
    }

    /// Set client IP.
    #[must_use]
    pub fn with_client_ip(mut self, ip: String) -> Self {
        self.client_ip = Some(ip);
        self
    }

    /// Set API version.
    #[must_use]
    pub fn with_api_version(mut self, version: String) -> Self {
        self.api_version = Some(version);
        self
    }
}

impl Default for RequestContext {
    fn default() -> Self {
        Self::new()
    }
}

/// Log level for severity classification.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "UPPERCASE")]
pub enum LogLevel {
    /// Trace level (most verbose)
    Trace,
    /// Debug level
    Debug,
    /// Info level
    Info,
    /// Warning level
    Warn,
    /// Error level
    Error,
}

impl fmt::Display for LogLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Trace => write!(f, "TRACE"),
            Self::Debug => write!(f, "DEBUG"),
            Self::Info => write!(f, "INFO"),
            Self::Warn => write!(f, "WARN"),
            Self::Error => write!(f, "ERROR"),
        }
    }
}

impl From<tracing::Level> for LogLevel {
    fn from(level: tracing::Level) -> Self {
        match level {
            tracing::Level::TRACE => Self::Trace,
            tracing::Level::DEBUG => Self::Debug,
            tracing::Level::INFO => Self::Info,
            tracing::Level::WARN => Self::Warn,
            tracing::Level::ERROR => Self::Error,
        }
    }
}

/// Structured JSON log entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StructuredLogEntry {
    /// Log timestamp (ISO 8601 format)
    pub timestamp: String,

    /// Log level
    pub level: LogLevel,

    /// Log message
    pub message: String,

    /// Request context (if applicable)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_context: Option<RequestContext>,

    /// Performance metrics
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metrics: Option<LogMetrics>,

    /// Error details (if applicable)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ErrorDetails>,

    /// Source code location
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<SourceLocation>,

    /// Additional context fields
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<serde_json::Value>,
}

impl StructuredLogEntry {
    /// Create new log entry.
    #[must_use]
    pub fn new(level: LogLevel, message: String) -> Self {
        Self {
            timestamp: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
            level,
            message,
            request_context: None,
            metrics: None,
            error: None,
            source: None,
            context: None,
        }
    }

    /// Add request context.
    #[must_use]
    pub fn with_request_context(mut self, context: RequestContext) -> Self {
        self.request_context = Some(context);
        self
    }

    /// Add performance metrics.
    #[must_use]
    pub fn with_metrics(mut self, metrics: LogMetrics) -> Self {
        self.metrics = Some(metrics);
        self
    }

    /// Add error details.
    #[must_use]
    pub fn with_error(mut self, error: ErrorDetails) -> Self {
        self.error = Some(error);
        self
    }

    /// Add source location.
    #[must_use]
    pub fn with_source(mut self, source: SourceLocation) -> Self {
        self.source = Some(source);
        self
    }

    /// Add custom context.
    #[must_use]
    pub fn with_context(mut self, context: serde_json::Value) -> Self {
        self.context = Some(context);
        self
    }

    /// Serialize to JSON string.
    #[must_use]
    pub fn to_json_string(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| {
            format!(
                r#"{{"level":"{}","message":"{}","error":"serialization failed"}}"#,
                self.level, self.message
            )
        })
    }
}

/// Performance metrics for a log entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogMetrics {
    /// Duration in milliseconds
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<f64>,

    /// Query complexity (depth, field count, etc.)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub complexity: Option<u32>,

    /// Number of items processed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub items_processed: Option<u64>,

    /// Cache hit indicator
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_hit: Option<bool>,

    /// Database queries executed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub db_queries: Option<u32>,
}

impl LogMetrics {
    /// Create new metrics container.
    #[must_use]
    pub fn new() -> Self {
        Self {
            duration_ms:     None,
            complexity:      None,
            items_processed: None,
            cache_hit:       None,
            db_queries:      None,
        }
    }

    /// Set duration in milliseconds.
    #[must_use]
    pub fn with_duration_ms(mut self, duration: f64) -> Self {
        self.duration_ms = Some(duration);
        self
    }

    /// Set query complexity.
    #[must_use]
    pub fn with_complexity(mut self, complexity: u32) -> Self {
        self.complexity = Some(complexity);
        self
    }

    /// Set items processed count.
    #[must_use]
    pub fn with_items_processed(mut self, count: u64) -> Self {
        self.items_processed = Some(count);
        self
    }

    /// Set cache hit status.
    #[must_use]
    pub fn with_cache_hit(mut self, hit: bool) -> Self {
        self.cache_hit = Some(hit);
        self
    }

    /// Set database query count.
    #[must_use]
    pub fn with_db_queries(mut self, count: u32) -> Self {
        self.db_queries = Some(count);
        self
    }
}

impl Default for LogMetrics {
    fn default() -> Self {
        Self::new()
    }
}

/// Error details for error logs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorDetails {
    /// Error type/category
    pub error_type: String,

    /// Error message
    pub message: String,

    /// Error code (if applicable)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,

    /// Stack trace (in debug builds)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stack_trace: Option<String>,
}

impl ErrorDetails {
    /// Create new error details.
    #[must_use]
    pub fn new(error_type: String, message: String) -> Self {
        Self {
            error_type,
            message,
            code: None,
            stack_trace: None,
        }
    }

    /// Set error code.
    #[must_use]
    pub fn with_code(mut self, code: String) -> Self {
        self.code = Some(code);
        self
    }

    /// Set stack trace.
    #[must_use]
    pub fn with_stack_trace(mut self, trace: String) -> Self {
        self.stack_trace = Some(trace);
        self
    }
}

/// Source code location information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceLocation {
    /// Source file name
    pub file: String,

    /// Line number
    pub line: u32,

    /// Module path
    pub module: String,
}

impl SourceLocation {
    /// Create new source location.
    #[must_use]
    pub fn new(file: String, line: u32, module: String) -> Self {
        Self { file, line, module }
    }
}

/// Request logger for contextual logging.
#[derive(Debug, Clone)]
pub struct RequestLogger {
    context: Arc<RequestContext>,
}

impl RequestLogger {
    /// Create new request logger.
    #[must_use]
    pub fn new(context: RequestContext) -> Self {
        Self {
            context: Arc::new(context),
        }
    }

    /// Create from request ID.
    #[must_use]
    pub fn with_request_id(request_id: RequestId) -> Self {
        Self::new(RequestContext {
            request_id,
            ..Default::default()
        })
    }

    /// Get request context.
    #[must_use]
    pub fn context(&self) -> &RequestContext {
        &self.context
    }

    /// Create info log entry with context.
    pub fn info(&self, message: impl Into<String>) -> StructuredLogEntry {
        StructuredLogEntry::new(LogLevel::Info, message.into())
            .with_request_context((*self.context).clone())
    }

    /// Create warn log entry with context.
    pub fn warn(&self, message: impl Into<String>) -> StructuredLogEntry {
        StructuredLogEntry::new(LogLevel::Warn, message.into())
            .with_request_context((*self.context).clone())
    }

    /// Create error log entry with context.
    pub fn error(&self, message: impl Into<String>) -> StructuredLogEntry {
        StructuredLogEntry::new(LogLevel::Error, message.into())
            .with_request_context((*self.context).clone())
    }

    /// Create debug log entry with context.
    pub fn debug(&self, message: impl Into<String>) -> StructuredLogEntry {
        StructuredLogEntry::new(LogLevel::Debug, message.into())
            .with_request_context((*self.context).clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_request_id_generation() {
        let id1 = RequestId::new();
        let id2 = RequestId::new();
        assert_ne!(id1.0, id2.0);
    }

    #[test]
    fn test_request_context_builder() {
        let context = RequestContext::new()
            .with_operation("GetUser".to_string())
            .with_user_id("user123".to_string())
            .with_client_ip("192.168.1.1".to_string())
            .with_api_version("v1".to_string());

        assert_eq!(context.operation, Some("GetUser".to_string()));
        assert_eq!(context.user_id, Some("user123".to_string()));
        assert_eq!(context.client_ip, Some("192.168.1.1".to_string()));
        assert_eq!(context.api_version, Some("v1".to_string()));
    }

    #[test]
    fn test_log_entry_creation() {
        let entry = StructuredLogEntry::new(LogLevel::Info, "test message".to_string());
        assert_eq!(entry.level, LogLevel::Info);
        assert_eq!(entry.message, "test message");
        assert!(entry.request_context.is_none());
    }

    #[test]
    fn test_log_entry_with_context() {
        let context = RequestContext::new().with_operation("Query".to_string());

        let entry = StructuredLogEntry::new(LogLevel::Info, "operation executed".to_string())
            .with_request_context(context.clone());

        assert!(entry.request_context.is_some());
        assert_eq!(entry.request_context.unwrap().operation, Some("Query".to_string()));
    }

    #[test]
    fn test_log_metrics_builder() {
        let metrics = LogMetrics::new()
            .with_duration_ms(123.45)
            .with_complexity(5)
            .with_items_processed(100)
            .with_cache_hit(true)
            .with_db_queries(3);

        assert_eq!(metrics.duration_ms, Some(123.45));
        assert_eq!(metrics.complexity, Some(5));
        assert_eq!(metrics.items_processed, Some(100));
        assert_eq!(metrics.cache_hit, Some(true));
        assert_eq!(metrics.db_queries, Some(3));
    }

    #[test]
    fn test_error_details_builder() {
        let error =
            ErrorDetails::new("DatabaseError".to_string(), "Connection timeout".to_string())
                .with_code("DB_TIMEOUT".to_string());

        assert_eq!(error.error_type, "DatabaseError");
        assert_eq!(error.message, "Connection timeout");
        assert_eq!(error.code, Some("DB_TIMEOUT".to_string()));
    }

    #[test]
    fn test_log_entry_json_serialization() {
        let entry = StructuredLogEntry::new(LogLevel::Error, "test error".to_string());
        let json = entry.to_json_string();

        assert!(json.contains("\"level\":\"ERROR\""));
        assert!(json.contains("\"message\":\"test error\""));
        assert!(json.contains("\"timestamp\":"));
    }

    #[test]
    fn test_request_logger_creation() {
        let context = RequestContext::new().with_operation("Query".to_string());
        let logger = RequestLogger::new(context);

        assert_eq!(logger.context().operation, Some("Query".to_string()));
    }

    #[test]
    fn test_request_logger_log_entry() {
        let logger = RequestLogger::with_request_id(RequestId::new());
        let entry = logger.info("test message");

        assert_eq!(entry.level, LogLevel::Info);
        assert!(entry.request_context.is_some());
    }

    #[test]
    fn test_log_level_conversion() {
        assert_eq!(LogLevel::from(tracing::Level::INFO), LogLevel::Info);
        assert_eq!(LogLevel::from(tracing::Level::WARN), LogLevel::Warn);
        assert_eq!(LogLevel::from(tracing::Level::ERROR), LogLevel::Error);
        assert_eq!(LogLevel::from(tracing::Level::DEBUG), LogLevel::Debug);
        assert_eq!(LogLevel::from(tracing::Level::TRACE), LogLevel::Trace);
    }

    #[test]
    fn test_complex_log_entry() {
        let context = RequestContext::new()
            .with_operation("GetUsers".to_string())
            .with_user_id("user123".to_string());

        let metrics = LogMetrics::new()
            .with_duration_ms(45.67)
            .with_db_queries(2)
            .with_cache_hit(true);

        let error =
            ErrorDetails::new("ValidationError".to_string(), "Invalid query parameter".to_string());

        let source = SourceLocation::new(
            "routes/graphql.rs".to_string(),
            42,
            "fraiseql_server::routes".to_string(),
        );

        let entry = StructuredLogEntry::new(LogLevel::Warn, "Query validation warning".to_string())
            .with_request_context(context)
            .with_metrics(metrics)
            .with_error(error)
            .with_source(source);

        let json = entry.to_json_string();
        assert!(json.contains("\"level\":\"WARN\""));
        assert!(json.contains("\"duration_ms\":"));
        assert!(json.contains("\"error_type\":"));
        assert!(json.contains("\"file\":"));
    }
}
