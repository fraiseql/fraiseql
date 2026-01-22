//! Structured logging with automatic trace ID injection

use super::correlation::{get_current_trace_id, TraceContext};
use std::collections::HashMap;

/// Structured logger with automatic trace ID field injection
pub struct StructuredLogger {
    /// Service/component name
    service: String,
    /// Span ID for this logger instance
    span_id: Option<String>,
}

impl StructuredLogger {
    /// Create new structured logger
    ///
    /// # Example
    /// ```ignore
    /// let logger = StructuredLogger::new("webhook-service");
    /// logger.info("webhook_sent", vec![("status", "200"), ("duration_ms", "42")]);
    /// ```
    #[must_use] 
    pub fn new(service: &str) -> Self {
        Self {
            service: service.to_string(),
            span_id: None,
        }
    }

    /// Create logger with span ID for distributed tracing
    #[must_use] 
    pub fn with_span(service: &str, span_id: &str) -> Self {
        Self {
            service: service.to_string(),
            span_id: Some(span_id.to_string()),
        }
    }

    /// Create logger with trace context
    #[must_use] 
    pub fn with_context(service: &str, context: &TraceContext) -> Self {
        Self {
            service: service.to_string(),
            span_id: Some(context.span_id.clone()),
        }
    }

    /// Log at DEBUG level with fields
    pub fn debug(&self, event: &str, fields: Vec<(&str, &str)>) {
        self.log_internal("DEBUG", event, fields);
    }

    /// Log at INFO level with fields
    pub fn info(&self, event: &str, fields: Vec<(&str, &str)>) {
        self.log_internal("INFO", event, fields);
    }

    /// Log at WARN level with fields
    pub fn warn(&self, event: &str, fields: Vec<(&str, &str)>) {
        self.log_internal("WARN", event, fields);
    }

    /// Log at ERROR level with fields
    pub fn error(&self, event: &str, fields: Vec<(&str, &str)>) {
        self.log_internal("ERROR", event, fields);
    }

    /// Log with automatic trace ID injection
    fn log_internal(&self, level: &str, event: &str, fields: Vec<(&str, &str)>) {
        let mut all_fields = HashMap::new();

        // Add user-provided fields
        for (key, value) in fields {
            all_fields.insert(key, value);
        }

        // Inject trace ID if available
        let trace_id = get_current_trace_id();
        let trace_id_str = trace_id.as_deref().unwrap_or("none");
        all_fields.insert("trace_id", trace_id_str);

        // Include span ID if available
        if let Some(ref span_id) = self.span_id {
            all_fields.insert("span_id", span_id.as_str());
        }

        // Log in structured format
        let fields_str = self.format_fields(&all_fields);
        let log_line = format!(
            "{} [{}] {} {} {}",
            chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ"),
            level,
            self.service,
            event,
            fields_str
        );

        match level {
            "DEBUG" => tracing::debug!("{}", log_line),
            "INFO" => tracing::info!("{}", log_line),
            "WARN" => tracing::warn!("{}", log_line),
            "ERROR" => tracing::error!("{}", log_line),
            _ => tracing::debug!("{}", log_line),
        }
    }

    /// Format fields as structured output
    fn format_fields(&self, fields: &HashMap<&str, &str>) -> String {
        fields
            .iter()
            .map(|(k, v)| {
                if v.contains(' ') || v.contains('=') {
                    format!("{k}=\"{v}\"")
                } else {
                    format!("{k}={v}")
                }
            })
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// Builder for structured log fields
pub struct LogBuilder {
    service: String,
    fields: Vec<(String, String)>,
    trace_id: Option<String>,
    span_id: Option<String>,
}

impl LogBuilder {
    /// Create new log builder
    #[must_use] 
    pub fn new(service: &str) -> Self {
        Self {
            service: service.to_string(),
            fields: Vec::new(),
            trace_id: None,
            span_id: None,
        }
    }

    /// Add field
    #[must_use] 
    pub fn field(mut self, key: &str, value: &str) -> Self {
        self.fields.push((key.to_string(), value.to_string()));
        self
    }

    /// Add numeric field
    #[must_use] 
    pub fn field_i64(mut self, key: &str, value: i64) -> Self {
        self.fields.push((key.to_string(), value.to_string()));
        self
    }

    /// Add float field
    #[must_use] 
    pub fn field_f64(mut self, key: &str, value: f64) -> Self {
        self.fields.push((key.to_string(), value.to_string()));
        self
    }

    /// Add trace context
    #[must_use] 
    pub fn with_context(mut self, context: &TraceContext) -> Self {
        self.trace_id = Some(context.trace_id.clone());
        self.span_id = Some(context.span_id.clone());
        self
    }

    /// Log as DEBUG
    pub fn debug(self, event: &str) {
        let logger = StructuredLogger::with_span(&self.service, "");
        let fields = self
            .fields
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect::<Vec<_>>();
        logger.debug(event, fields);
    }

    /// Log as INFO
    pub fn info(self, event: &str) {
        let logger = StructuredLogger::with_span(&self.service, "");
        let fields = self
            .fields
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect::<Vec<_>>();
        logger.info(event, fields);
    }

    /// Log as WARN
    pub fn warn(self, event: &str) {
        let logger = StructuredLogger::with_span(&self.service, "");
        let fields = self
            .fields
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect::<Vec<_>>();
        logger.warn(event, fields);
    }

    /// Log as ERROR
    pub fn error(self, event: &str) {
        let logger = StructuredLogger::with_span(&self.service, "");
        let fields = self
            .fields
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect::<Vec<_>>();
        logger.error(event, fields);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_structured_logger_creation() {
        let logger = StructuredLogger::new("test-service");
        assert_eq!(logger.service, "test-service");
        assert_eq!(logger.span_id, None);
    }

    #[test]
    fn test_logger_with_span() {
        let logger = StructuredLogger::with_span("test-service", "span-123");
        assert_eq!(logger.service, "test-service");
        assert_eq!(logger.span_id, Some("span-123".to_string()));
    }

    #[test]
    fn test_log_builder() {
        let builder = LogBuilder::new("service")
            .field("status", "200")
            .field_i64("duration_ms", 42)
            .field_f64("latency", 3.15);

        assert_eq!(builder.service, "service");
        assert_eq!(builder.fields.len(), 3);
    }

    #[test]
    fn test_format_fields() {
        let logger = StructuredLogger::new("test");
        let mut fields = HashMap::new();
        fields.insert("status", "200");
        fields.insert("message", "request successful");

        let formatted = logger.format_fields(&fields);
        assert!(formatted.contains("status=200"));
        assert!(formatted.contains("message="));
    }

    #[test]
    fn test_trace_context_with_logger() {
        let context = TraceContext::new("trace-123".to_string(), "span-456".to_string(), true);
        let logger = StructuredLogger::with_context("service", &context);
        assert_eq!(logger.span_id, Some("span-456".to_string()));
    }
}
