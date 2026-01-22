//! Trace ID correlation and context propagation
//!
//! Provides utilities for injecting and extracting trace IDs from logging context.

use std::sync::OnceLock;
use std::cell::RefCell;

thread_local! {
    static TRACE_ID_CONTEXT: RefCell<Option<String>> = RefCell::new(None);
}

/// Extracts trace ID from various sources
pub struct TraceIdExtractor;

impl TraceIdExtractor {
    /// Extract trace ID from W3C traceparent header
    ///
    /// # Format
    /// ```text
    /// traceparent: 00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01
    ///             version-trace_id-span_id-flags
    /// ```
    pub fn from_w3c_traceparent(header: &str) -> Option<String> {
        let parts: Vec<&str> = header.split('-').collect();
        if parts.len() >= 3 {
            Some(parts[1].to_string())
        } else {
            None
        }
    }

    /// Extract trace ID from X-Trace-Id header
    pub fn from_x_trace_id(header: &str) -> Option<String> {
        if header.is_empty() {
            None
        } else {
            Some(header.to_string())
        }
    }

    /// Extract trace ID from HTTP headers map
    pub fn from_headers(headers: &[(String, String)]) -> Option<String> {
        for (key, value) in headers {
            let lower_key = key.to_lowercase();
            if lower_key == "traceparent" {
                return Self::from_w3c_traceparent(value);
            } else if lower_key == "x-trace-id" {
                return Self::from_x_trace_id(value);
            }
        }
        None
    }

    /// Extract trace ID from Jaeger trace header format
    pub fn from_jaeger_header(header: &str) -> Option<String> {
        // Jaeger format: trace_id:span_id:flags
        let parts: Vec<&str> = header.split(':').collect();
        if !parts.is_empty() && !parts[0].is_empty() {
            Some(parts[0].to_string())
        } else {
            None
        }
    }
}

/// Set the current trace ID in thread-local context
///
/// # Example
/// ```ignore
/// set_trace_id_context("abc123def456");
/// // All subsequent logs will include trace_id=abc123def456
/// ```
pub fn set_trace_id_context(trace_id: &str) {
    TRACE_ID_CONTEXT.with(|ctx| {
        if trace_id.is_empty() {
            *ctx.borrow_mut() = None;
        } else {
            *ctx.borrow_mut() = Some(trace_id.to_string());
        }
    });
}

/// Get the current trace ID from thread-local context
///
/// # Returns
/// `Some(trace_id)` if trace ID is set, `None` otherwise
pub fn get_current_trace_id() -> Option<String> {
    TRACE_ID_CONTEXT.with(|ctx| ctx.borrow().clone())
}

/// Clear the current trace ID context
pub fn clear_trace_id_context() {
    TRACE_ID_CONTEXT.with(|ctx| {
        *ctx.borrow_mut() = None;
    });
}

/// Structure for propagating trace context through async boundaries
#[derive(Clone, Debug)]
pub struct TraceContext {
    /// Trace ID from parent span
    pub trace_id: String,
    /// Span ID of current span
    pub span_id: String,
    /// Parent span ID (if any)
    pub parent_span_id: Option<String>,
    /// Sampling decision (0=not sampled, 1=sampled)
    pub sampled: bool,
}

impl TraceContext {
    /// Create new trace context
    pub fn new(trace_id: String, span_id: String, sampled: bool) -> Self {
        Self {
            trace_id,
            span_id,
            parent_span_id: None,
            sampled,
        }
    }

    /// Create trace context with parent span
    pub fn with_parent(
        trace_id: String,
        span_id: String,
        parent_span_id: String,
        sampled: bool,
    ) -> Self {
        Self {
            trace_id,
            span_id,
            parent_span_id: Some(parent_span_id),
            sampled,
        }
    }

    /// Convert to W3C traceparent header format
    pub fn to_traceparent_header(&self) -> String {
        let sampled_flag = if self.sampled { "01" } else { "00" };
        format!(
            "00-{}-{}-{}",
            self.trace_id,
            self.span_id,
            sampled_flag
        )
    }

    /// Parse from W3C traceparent header
    pub fn from_traceparent_header(header: &str) -> Option<Self> {
        let parts: Vec<&str> = header.split('-').collect();
        if parts.len() < 4 {
            return None;
        }

        let trace_id = parts[1].to_string();
        let span_id = parts[2].to_string();
        let sampled = parts[3] == "01";

        Some(Self::new(trace_id, span_id, sampled))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_w3c_traceparent() {
        let header = "00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01";
        let trace_id = TraceIdExtractor::from_w3c_traceparent(header);
        assert_eq!(trace_id, Some("0af7651916cd43dd8448eb211c80319c".to_string()));
    }

    #[test]
    fn test_extract_x_trace_id() {
        let header = "my-custom-trace-id-123";
        let trace_id = TraceIdExtractor::from_x_trace_id(header);
        assert_eq!(trace_id, Some("my-custom-trace-id-123".to_string()));
    }

    #[test]
    fn test_extract_jaeger_header() {
        let header = "abc123def456:span789:1";
        let trace_id = TraceIdExtractor::from_jaeger_header(header);
        assert_eq!(trace_id, Some("abc123def456".to_string()));
    }

    #[test]
    fn test_trace_context_creation() {
        let ctx = TraceContext::new("trace-123".to_string(), "span-456".to_string(), true);
        assert_eq!(ctx.trace_id, "trace-123");
        assert_eq!(ctx.span_id, "span-456");
        assert!(ctx.sampled);
    }

    #[test]
    fn test_trace_context_to_traceparent() {
        let ctx = TraceContext::new("trace-123".to_string(), "span-456".to_string(), true);
        let header = ctx.to_traceparent_header();
        assert!(header.contains("trace-123"));
        assert!(header.contains("span-456"));
        assert!(header.contains("01"));
    }

    #[test]
    fn test_trace_context_from_traceparent() {
        let header = "00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01";
        let ctx = TraceContext::from_traceparent_header(header);
        assert!(ctx.is_some());
        let ctx = ctx.unwrap();
        assert_eq!(ctx.trace_id, "0af7651916cd43dd8448eb211c80319c");
        assert_eq!(ctx.span_id, "b7ad6b7169203331");
        assert!(ctx.sampled);
    }

    #[test]
    fn test_trace_id_context_lifecycle() {
        clear_trace_id_context();
        assert_eq!(get_current_trace_id(), None);

        set_trace_id_context("my-trace-id");
        assert_eq!(get_current_trace_id(), Some("my-trace-id".to_string()));

        set_trace_id_context("another-trace-id");
        assert_eq!(
            get_current_trace_id(),
            Some("another-trace-id".to_string())
        );

        clear_trace_id_context();
        assert_eq!(get_current_trace_id(), None);
    }

    #[test]
    fn test_headers_extraction() {
        let headers = vec![
            ("Content-Type".to_string(), "application/json".to_string()),
            ("X-Trace-Id".to_string(), "trace-123".to_string()),
            ("Accept".to_string(), "*/*".to_string()),
        ];

        let trace_id = TraceIdExtractor::from_headers(&headers);
        assert_eq!(trace_id, Some("trace-123".to_string()));
    }

    #[test]
    fn test_traceparent_priority() {
        let headers = vec![
            ("traceparent".to_string(), "00-abc123-def456-01".to_string()),
            ("X-Trace-Id".to_string(), "fallback-id".to_string()),
        ];

        let trace_id = TraceIdExtractor::from_headers(&headers);
        // traceparent should be preferred
        assert_eq!(trace_id, Some("abc123".to_string()));
    }
}
