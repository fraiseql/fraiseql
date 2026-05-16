//! Logging module tests

mod mod_tests {
    use super::super::*;

    #[test]
    fn test_trace_id_correlation() {
        set_trace_id_context("test-trace-id-123");
        let trace_id = get_current_trace_id();
        assert_eq!(trace_id, Some("test-trace-id-123".to_string()));
    }

    #[test]
    fn test_clear_trace_id() {
        set_trace_id_context("some-trace-id");
        set_trace_id_context("");
        let trace_id = get_current_trace_id();
        assert_eq!(trace_id, None);
    }
}

mod structured_tests {
    use std::collections::HashMap;

    use super::super::{correlation::TraceContext, structured::*};

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

#[allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
mod correlation_tests {
    use super::super::correlation::*;

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
        assert_eq!(get_current_trace_id(), Some("another-trace-id".to_string()));

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
