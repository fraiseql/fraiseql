//! Structured logging with trace context
//!
//! Provides JSON-formatted logs with trace ID correlation

use std::collections::HashMap;

/// Initialize structured logging
pub fn init_logging() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing-subscriber with JSON formatting
    // In GREEN phase, this is a no-op
    Ok(())
}

/// Log level
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum LogLevel {
    /// Debug level
    Debug = 0,
    /// Info level
    Info = 1,
    /// Warning level
    Warn = 2,
    /// Error level
    Error = 3,
}

impl LogLevel {
    /// Get string representation
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Debug => "DEBUG",
            Self::Info => "INFO",
            Self::Warn => "WARN",
            Self::Error => "ERROR",
        }
    }
}

/// Structured log entry
#[derive(Clone, Debug)]
pub struct LogEntry {
    /// Timestamp
    pub timestamp: String,
    /// Log level
    pub level: LogLevel,
    /// Log message
    pub message: String,
    /// Trace ID
    pub trace_id: Option<String>,
    /// Span ID
    pub span_id: Option<String>,
    /// Additional fields
    pub fields: HashMap<String, String>,
}

impl LogEntry {
    /// Create a new log entry
    pub fn new(level: LogLevel, message: impl Into<String>) -> Self {
        Self {
            timestamp: chrono::Utc::now().to_rfc3339(),
            level,
            message: message.into(),
            trace_id: None,
            span_id: None,
            fields: HashMap::new(),
        }
    }

    /// Set trace ID
    pub fn with_trace_id(mut self, trace_id: impl Into<String>) -> Self {
        self.trace_id = Some(trace_id.into());
        self
    }

    /// Set span ID
    pub fn with_span_id(mut self, span_id: impl Into<String>) -> Self {
        self.span_id = Some(span_id.into());
        self
    }

    /// Add a field
    pub fn with_field(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.fields.insert(key.into(), value.into());
        self
    }

    /// Format as JSON
    pub fn as_json(&self) -> Result<serde_json::Value, serde_json::Error> {
        let mut json = serde_json::json!({
            "timestamp": self.timestamp,
            "level": self.level.as_str(),
            "message": self.message,
        });

        if let Some(ref trace_id) = self.trace_id {
            json["trace_id"] = serde_json::Value::String(trace_id.clone());
        }

        if let Some(ref span_id) = self.span_id {
            json["span_id"] = serde_json::Value::String(span_id.clone());
        }

        for (key, value) in &self.fields {
            json[key] = serde_json::Value::String(value.clone());
        }

        Ok(json)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_log_entry() {
        let entry = LogEntry::new(LogLevel::Info, "Test message")
            .with_trace_id("trace-123")
            .with_span_id("span-456")
            .with_field("user_id", "user-123");

        assert_eq!(entry.level, LogLevel::Info);
        assert_eq!(entry.message, "Test message");
        assert_eq!(entry.trace_id, Some("trace-123".to_string()));
        assert_eq!(entry.fields.len(), 1);
    }

    #[test]
    fn test_log_levels() {
        assert!(LogLevel::Debug < LogLevel::Info);
        assert!(LogLevel::Info < LogLevel::Warn);
        assert!(LogLevel::Warn < LogLevel::Error);
    }
}
