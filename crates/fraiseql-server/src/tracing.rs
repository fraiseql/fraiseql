//! Distributed tracing support for request correlation and debugging.
//!
//! Provides request trace context tracking, span management, and integration
//! with distributed tracing systems like Jaeger, Zipkin, and Datadog.

use crate::logging::RequestId;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;

/// Trace context for distributed tracing across service boundaries.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceContext {
    /// Unique trace identifier (same across all services in request chain)
    pub trace_id: String,

    /// Span identifier within this service
    pub span_id: String,

    /// Parent span identifier (from upstream service)
    pub parent_span_id: Option<String>,

    /// Sampling decision (0=not sampled, 1=sampled)
    pub sampled: u8,

    /// Trace flags (bit field for trace configuration)
    pub trace_flags: u8,

    /// Request-scoped baggage (key-value context)
    pub baggage: HashMap<String, String>,
}

impl TraceContext {
    /// Create new trace context with generated IDs.
    #[must_use] 
    pub fn new() -> Self {
        Self {
            trace_id: generate_trace_id(),
            span_id: generate_span_id(),
            parent_span_id: None,
            sampled: 1,
            trace_flags: 0x01,
            baggage: HashMap::new(),
        }
    }

    /// Create from request ID.
    #[must_use] 
    pub fn from_request_id(request_id: RequestId) -> Self {
        Self {
            trace_id: request_id.to_string(),
            span_id: generate_span_id(),
            parent_span_id: None,
            sampled: 1,
            trace_flags: 0x01,
            baggage: HashMap::new(),
        }
    }

    /// Create child span from parent context.
    #[must_use] 
    pub fn child_span(&self) -> Self {
        
        // Don't modify baggage in child
        Self {
            trace_id: self.trace_id.clone(),
            span_id: generate_span_id(),
            parent_span_id: Some(self.span_id.clone()),
            sampled: self.sampled,
            trace_flags: self.trace_flags,
            baggage: self.baggage.clone(),
        }
    }

    /// Add baggage item (cross-cutting context).
    #[must_use] 
    pub fn with_baggage(mut self, key: String, value: String) -> Self {
        self.baggage.insert(key, value);
        self
    }

    /// Get baggage item.
    #[must_use] 
    pub fn baggage_item(&self, key: &str) -> Option<&str> {
        self.baggage.get(key).map(std::string::String::as_str)
    }

    /// Set sampling decision.
    pub fn set_sampled(&mut self, sampled: bool) {
        self.sampled = u8::from(sampled);
    }

    /// Generate W3C Trace Context header value.
    #[must_use] 
    pub fn to_w3c_traceparent(&self) -> String {
        // Format: version-traceid-spanid-traceflags
        // version: 00, traceid: 32 hex chars, spanid: 16 hex chars, traceflags: 2 hex chars
        format!(
            "00-{}-{}-{:02x}",
            self.trace_id, self.span_id, self.trace_flags
        )
    }

    /// Parse W3C Trace Context header.
    pub fn from_w3c_traceparent(header: &str) -> Result<Self, TraceParseError> {
        let parts: Vec<&str> = header.split('-').collect();
        if parts.len() != 4 {
            return Err(TraceParseError::InvalidFormat);
        }

        if parts[0] != "00" {
            return Err(TraceParseError::UnsupportedVersion);
        }

        if parts[1].len() != 32 || !parts[1].chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(TraceParseError::InvalidTraceId);
        }

        if parts[2].len() != 16 || !parts[2].chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(TraceParseError::InvalidSpanId);
        }

        let trace_flags = u8::from_str_radix(parts[3], 16)
            .map_err(|_| TraceParseError::InvalidTraceFlags)?;

        Ok(Self {
            trace_id: parts[1].to_string(),
            span_id: generate_span_id(), // Generate new span ID for this service
            parent_span_id: Some(parts[2].to_string()),
            sampled: (trace_flags & 0x01),
            trace_flags,
            baggage: HashMap::new(),
        })
    }

    /// Check if trace should be sampled.
    #[must_use] 
    pub fn is_sampled(&self) -> bool {
        self.sampled == 1
    }
}

impl Default for TraceContext {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for TraceContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "trace_id={}, span_id={}, sampled={}",
            self.trace_id, self.span_id, self.sampled
        )
    }
}

/// Trace span for measuring operation duration within a trace.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceSpan {
    /// Span identifier
    pub span_id: String,

    /// Trace identifier (inherited from context)
    pub trace_id: String,

    /// Parent span identifier
    pub parent_span_id: Option<String>,

    /// Operation name
    pub operation: String,

    /// Start time (Unix milliseconds)
    pub start_time_ms: i64,

    /// End time (Unix milliseconds, None if not finished)
    pub end_time_ms: Option<i64>,

    /// Span attributes (key-value tags)
    pub attributes: HashMap<String, String>,

    /// Span events (annotations with timestamp)
    pub events: Vec<TraceEvent>,

    /// Span status
    pub status: SpanStatus,
}

impl TraceSpan {
    /// Create new span.
    #[must_use] 
    pub fn new(trace_id: String, operation: String) -> Self {
        Self {
            span_id: generate_span_id(),
            trace_id,
            parent_span_id: None,
            operation,
            start_time_ms: current_time_ms(),
            end_time_ms: None,
            attributes: HashMap::new(),
            events: Vec::new(),
            status: SpanStatus::Unset,
        }
    }

    /// Set parent span.
    #[must_use] 
    pub fn with_parent_span(mut self, parent_span_id: String) -> Self {
        self.parent_span_id = Some(parent_span_id);
        self
    }

    /// Add span attribute.
    #[must_use] 
    pub fn add_attribute(mut self, key: String, value: String) -> Self {
        self.attributes.insert(key, value);
        self
    }

    /// Add span event.
    #[must_use] 
    pub fn add_event(mut self, event: TraceEvent) -> Self {
        self.events.push(event);
        self
    }

    /// Mark span as finished.
    pub fn finish(&mut self) {
        self.end_time_ms = Some(current_time_ms());
    }

    /// Get span duration in milliseconds.
    #[must_use] 
    pub fn duration_ms(&self) -> Option<i64> {
        self.end_time_ms.map(|end| end - self.start_time_ms)
    }

    /// Set span status to error.
    #[must_use] 
    pub fn set_error(mut self, message: String) -> Self {
        self.status = SpanStatus::Error { message };
        self
    }

    /// Set span status to ok.
    #[must_use] 
    pub fn set_ok(mut self) -> Self {
        self.status = SpanStatus::Ok;
        self
    }
}

/// Span status enumeration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SpanStatus {
    /// Unset status
    Unset,

    /// Operation succeeded
    Ok,

    /// Operation failed
    Error {
        /// Error message
        message: String,
    },
}

impl fmt::Display for SpanStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unset => write!(f, "UNSET"),
            Self::Ok => write!(f, "OK"),
            Self::Error { message } => write!(f, "ERROR: {message}"),
        }
    }
}

/// Event within a trace span.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceEvent {
    /// Event name
    pub name: String,

    /// Event timestamp (Unix milliseconds)
    pub timestamp_ms: i64,

    /// Event attributes
    pub attributes: HashMap<String, String>,
}

impl TraceEvent {
    /// Create new trace event.
    #[must_use] 
    pub fn new(name: String) -> Self {
        Self {
            name,
            timestamp_ms: current_time_ms(),
            attributes: HashMap::new(),
        }
    }

    /// Add event attribute.
    #[must_use] 
    pub fn with_attribute(mut self, key: String, value: String) -> Self {
        self.attributes.insert(key, value);
        self
    }
}

/// Trace error for parsing/validation failures.
#[derive(Debug, Clone, Copy)]
pub enum TraceParseError {
    /// Invalid header format
    InvalidFormat,

    /// Unsupported trace context version
    UnsupportedVersion,

    /// Invalid trace ID
    InvalidTraceId,

    /// Invalid span ID
    InvalidSpanId,

    /// Invalid trace flags
    InvalidTraceFlags,
}

impl fmt::Display for TraceParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidFormat => write!(f, "Invalid trace context format"),
            Self::UnsupportedVersion => write!(f, "Unsupported trace context version"),
            Self::InvalidTraceId => write!(f, "Invalid trace ID"),
            Self::InvalidSpanId => write!(f, "Invalid span ID"),
            Self::InvalidTraceFlags => write!(f, "Invalid trace flags"),
        }
    }
}

impl std::error::Error for TraceParseError {}

/// Generate random trace ID (32 hex characters).
fn generate_trace_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};

    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);

    format!("{:032x}", nanos ^ u128::from(std::process::id()))
}

/// Generate random span ID (16 hex characters).
fn generate_span_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};

    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);

    // Use process ID as thread ID is unstable
    let process_id = u128::from(std::process::id());
    format!("{:016x}", (nanos ^ process_id) as u64)
}

/// Get current time as Unix milliseconds.
fn current_time_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};

    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trace_context_creation() {
        let ctx = TraceContext::new();
        assert!(!ctx.trace_id.is_empty());
        assert!(!ctx.span_id.is_empty());
        assert!(ctx.is_sampled());
    }

    #[test]
    fn test_trace_context_child_span() {
        let parent = TraceContext::new();
        let child = parent.child_span();

        assert_eq!(parent.trace_id, child.trace_id);
        assert_ne!(parent.span_id, child.span_id);
        assert_eq!(child.parent_span_id, Some(parent.span_id));
    }

    #[test]
    fn test_trace_context_baggage() {
        let ctx = TraceContext::new()
            .with_baggage("user_id".to_string(), "user123".to_string())
            .with_baggage("tenant".to_string(), "acme".to_string());

        assert_eq!(ctx.baggage_item("user_id"), Some("user123"));
        assert_eq!(ctx.baggage_item("tenant"), Some("acme"));
        assert_eq!(ctx.baggage_item("missing"), None);
    }

    #[test]
    fn test_w3c_traceparent_format() {
        let ctx = TraceContext::new();
        let header = ctx.to_w3c_traceparent();

        assert!(header.starts_with("00-"));
        let parts: Vec<&str> = header.split('-').collect();
        assert_eq!(parts.len(), 4);
        assert_eq!(parts[0], "00");
        assert_eq!(parts[1].len(), 32);
        assert_eq!(parts[2].len(), 16);
    }

    #[test]
    fn test_w3c_traceparent_parsing() {
        let original = TraceContext::new();
        let header = original.to_w3c_traceparent();

        let parsed = TraceContext::from_w3c_traceparent(&header)
            .expect("Failed to parse traceparent");

        assert_eq!(parsed.trace_id, original.trace_id);
        assert_eq!(parsed.parent_span_id, Some(original.span_id));
    }

    #[test]
    fn test_w3c_traceparent_invalid_format() {
        let invalid = "invalid-format";
        assert!(TraceContext::from_w3c_traceparent(invalid).is_err());
    }

    #[test]
    fn test_trace_span_creation() {
        let span = TraceSpan::new("trace123".to_string(), "GetUser".to_string());

        assert_eq!(span.trace_id, "trace123");
        assert_eq!(span.operation, "GetUser");
        assert!(span.end_time_ms.is_none());
        assert_eq!(span.status.to_string(), "UNSET");
    }

    #[test]
    fn test_trace_span_finish() {
        let mut span = TraceSpan::new("trace123".to_string(), "Query".to_string());
        assert!(span.end_time_ms.is_none());

        span.finish();
        assert!(span.end_time_ms.is_some());

        let duration = span.duration_ms();
        assert!(duration.is_some());
        assert!(duration.unwrap() >= 0);
    }

    #[test]
    fn test_trace_span_attributes() {
        let span = TraceSpan::new("trace123".to_string(), "Query".to_string())
            .add_attribute("db.system".to_string(), "postgresql".to_string())
            .add_attribute("http.status_code".to_string(), "200".to_string());

        assert_eq!(span.attributes.len(), 2);
        assert_eq!(span.attributes.get("db.system"), Some(&"postgresql".to_string()));
    }

    #[test]
    fn test_trace_span_events() {
        let event1 = TraceEvent::new("query_start".to_string());
        let event2 = TraceEvent::new("query_end".to_string())
            .with_attribute("rows_affected".to_string(), "42".to_string());

        let span = TraceSpan::new("trace123".to_string(), "Update".to_string())
            .add_event(event1)
            .add_event(event2);

        assert_eq!(span.events.len(), 2);
        assert_eq!(span.events[1].name, "query_end");
    }

    #[test]
    fn test_trace_span_error_status() {
        let span = TraceSpan::new("trace123".to_string(), "Query".to_string())
            .set_error("Database connection failed".to_string());

        match span.status {
            SpanStatus::Error { message } => assert_eq!(message, "Database connection failed"),
            _ => panic!("Expected error status"),
        }
    }

    #[test]
    fn test_trace_context_from_request_id() {
        use crate::logging::RequestId;

        let request_id = RequestId::new();
        let ctx = TraceContext::from_request_id(request_id);

        assert_eq!(ctx.trace_id, request_id.to_string());
        assert!(ctx.is_sampled());
    }

    #[test]
    fn test_trace_event_creation() {
        let event = TraceEvent::new("cache_hit".to_string())
            .with_attribute("cache_key".to_string(), "query:user:123".to_string());

        assert_eq!(event.name, "cache_hit");
        assert_eq!(event.attributes.get("cache_key"), Some(&"query:user:123".to_string()));
    }

    #[test]
    fn test_trace_span_sampling() {
        let mut ctx = TraceContext::new();
        assert!(ctx.is_sampled());

        ctx.set_sampled(false);
        assert!(!ctx.is_sampled());
    }
}
