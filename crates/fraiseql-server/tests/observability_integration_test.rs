//! Observability Integration Tests
//!
//! Tests for observability features integrated into actual HTTP handlers

#![allow(unused_imports)]

use std::collections::HashMap;

#[cfg(test)]
mod tests {
    use super::*;

    /// Test observability initialization
    #[test]
    fn test_observability_initialization() {
        // Should initialize without errors
        let result = fraiseql_server::observability::init_observability();
        assert!(result.is_ok(), "Observability should initialize successfully");
        println!("✅ Observability initialization test passed");
    }

    /// Test span creation in request handler
    #[test]
    fn test_span_creation_in_handler() {
        use fraiseql_server::observability::create_span;

        let span = create_span("handle_graphql_request")
            .with_attribute("operation_type", "query")
            .with_attribute("user_id", "user-123")
            .build();

        assert_eq!(span.name, "handle_graphql_request");
        assert_eq!(span.attributes.len(), 2);
        println!("✅ Span creation in handler test passed");
    }

    /// Test metrics counter usage
    #[test]
    fn test_metrics_counter_usage() {
        use fraiseql_server::observability::MetricCounter;

        let mut counter = MetricCounter::new("graphql_queries_total")
            .with_label("operation", "query")
            .with_label("status", "success");

        counter.increment();
        counter.increment();
        counter.increment_by(2);

        assert_eq!(counter.value, 4);
        println!("✅ Metrics counter usage test passed");
    }

    /// Test histogram for query duration
    #[test]
    fn test_histogram_query_duration() {
        use fraiseql_server::observability::MetricHistogram;

        let mut histogram = MetricHistogram::new("query_duration_ms");

        // Simulate query durations
        histogram.observe(12);
        histogram.observe(45);
        histogram.observe(78);
        histogram.observe(23);
        histogram.observe(156);

        assert_eq!(histogram.min(), Some(12));
        assert_eq!(histogram.max(), Some(156));
        assert!(histogram.mean().is_some());

        println!(
            "✅ Histogram query duration test passed (min: {}, max: {}, mean: {:.2})",
            histogram.min().unwrap(),
            histogram.max().unwrap(),
            histogram.mean().unwrap()
        );
    }

    /// Test trace context propagation through request
    #[test]
    fn test_trace_context_in_request() {
        use fraiseql_server::observability::context::{TraceContext, get_context, set_context};

        let ctx = TraceContext::new()
            .with_baggage("user_id", "user-123")
            .with_baggage("request_id", "req-456");

        set_context(ctx.clone());

        let retrieved = get_context().expect("Context should be retrievable");
        assert_eq!(retrieved.trace_id, ctx.trace_id);
        assert_eq!(retrieved.baggage.len(), 2);

        println!("✅ Trace context in request test passed");
    }

    /// Test W3C traceparent header generation
    #[test]
    fn test_traceparent_header_generation() {
        use fraiseql_server::observability::context::TraceContext;

        let ctx = TraceContext::new();
        let header = ctx.traceparent_header();

        assert!(header.starts_with("00-"), "Should start with version");
        assert!(header.contains(&ctx.trace_id), "Should contain trace ID");
        assert!(header.contains(&ctx.span_id), "Should contain span ID");

        println!("✅ Traceparent header generation test passed");
    }

    /// Test structured logging with trace context
    #[test]
    fn test_structured_logging_integration() {
        use fraiseql_server::observability::logging::{LogEntry, LogLevel};

        let log = LogEntry::new(LogLevel::Info, "GraphQL query executed")
            .with_trace_id("trace-123")
            .with_span_id("span-456")
            .with_field("query_size", "256")
            .with_field("duration_ms", "45");

        // Should be JSON-serializable
        let json = log.as_json();
        assert!(json.is_ok(), "Log should serialize to JSON");

        if let Ok(json_value) = json {
            assert_eq!(json_value["level"], "INFO");
            assert_eq!(json_value["trace_id"], "trace-123");
            assert_eq!(json_value["query_size"], "256");
        }

        println!("✅ Structured logging integration test passed");
    }

    /// Test metrics registry
    #[test]
    fn test_metrics_registry() {
        use fraiseql_server::observability::MetricsRegistry;

        let registry = MetricsRegistry::new();

        // Register counters
        let counter = fraiseql_server::observability::MetricCounter::new("test_counter");
        let result = registry.register_counter(counter);
        assert!(result.is_ok(), "Should register counter successfully");

        println!("✅ Metrics registry test passed");
    }

    /// Test child span creation
    #[test]
    fn test_child_span_context() {
        use fraiseql_server::observability::context::TraceContext;

        let parent = TraceContext::new();
        let child = parent.child();

        assert_eq!(child.trace_id, parent.trace_id, "Child should inherit trace ID");
        assert_ne!(child.span_id, parent.span_id, "Child should have different span ID");
        assert_eq!(
            child.parent_span_id,
            Some(parent.span_id.clone()),
            "Child should reference parent"
        );

        println!("✅ Child span context test passed");
    }

    /// Test log level filtering
    #[test]
    fn test_log_level_filtering() {
        use fraiseql_server::observability::logging::LogLevel;

        assert!(LogLevel::Debug < LogLevel::Info);
        assert!(LogLevel::Info < LogLevel::Warn);
        assert!(LogLevel::Warn < LogLevel::Error);

        assert_eq!(LogLevel::Info.as_str(), "INFO");
        assert_eq!(LogLevel::Error.as_str(), "ERROR");

        println!("✅ Log level filtering test passed");
    }
}
