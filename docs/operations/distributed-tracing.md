<!-- Skip to main content -->
---

title: Distributed Tracing in FraiseQL v2
description: FraiseQL v2 provides comprehensive distributed tracing support for tracking requests across service boundaries. Built on W3C Trace Context standards, it enables
keywords: ["deployment", "scaling", "performance", "monitoring", "troubleshooting"]
tags: ["documentation", "reference"]
---

# Distributed Tracing in FraiseQL v2

## Overview

FraiseQL v2 provides comprehensive distributed tracing support for tracking requests across service boundaries. Built on W3C Trace Context standards, it enables end-to-end request correlation, performance analysis, and debugging in microservice architectures.

## Key Features

- **W3C Trace Context Support**: Standard-compliant trace propagation across services
- **Request Correlation**: Unique trace IDs for following requests through entire system
- **Span Management**: Track individual operations within a trace
- **Cross-cutting Context**: Baggage for sharing metadata across services
- **Event Tracking**: Record significant events during request execution
- **Status Tracking**: Track operation success, errors, and completion
- **Zero External Dependencies**: No vendor lock-in, works with any tracing backend

## Architecture

### Core Components

#### TraceContext

Main context for request propagation across service boundaries.

```rust
<!-- Code example in RUST -->
use fraiseql_server::TraceContext;

// Create new trace
let ctx = TraceContext::new();
println!("Trace: {}", ctx.trace_id);

// Create child context (for downstream service calls)
let child = ctx.child_span();
assert_eq!(child.trace_id, ctx.trace_id); // Same trace
assert_ne!(child.span_id, ctx.span_id);   // Different span

// Add cross-cutting context
let ctx_with_baggage = ctx
    .with_baggage("user_id".to_string(), "user123".to_string())
    .with_baggage("tenant".to_string(), "acme".to_string());

// Check baggage
assert_eq!(ctx_with_baggage.baggage_item("user_id"), Some("user_id"));
```text
<!-- Code example in TEXT -->

#### TraceSpan

Individual operation within a trace.

```rust
<!-- Code example in RUST -->
use fraiseql_server::TraceSpan;

let span = TraceSpan::new(
    "trace-id".to_string(),
    "GetUser".to_string()
)
.add_attribute("db.system".to_string(), "postgresql".to_string())
.add_attribute("http.status_code".to_string(), "200".to_string());

// Add events
let event = TraceEvent::new("query_executed".to_string())
    .with_attribute("rows".to_string(), "1".to_string());

let span = span.add_event(event);

// Mark as complete
let mut span = span;
span.finish();
assert!(span.end_time_ms.is_some());
```text
<!-- Code example in TEXT -->

#### TraceEvent

Significant event during span execution.

```rust
<!-- Code example in RUST -->
use fraiseql_server::TraceEvent;

let event = TraceEvent::new("cache_miss".to_string())
    .with_attribute("cache_key".to_string(), "query:user:123".to_string())
    .with_attribute("cache_type".to_string(), "redis".to_string());

println!("Event: {} at {}", event.name, event.timestamp_ms);
```text
<!-- Code example in TEXT -->

#### SpanStatus

Status enumeration for span execution.

```rust
<!-- Code example in RUST -->
use fraiseql_server::SpanStatus;

// Successful execution
let status = SpanStatus::Ok;

// Error case
let error_status = SpanStatus::Error {
    message: "Database connection timeout".to_string()
};

println!("Status: {}", error_status);
```text
<!-- Code example in TEXT -->

## Usage Examples

### Basic Request Tracing

```rust
<!-- Code example in RUST -->
use fraiseql_server::TraceContext;

// Incoming request
let trace_ctx = TraceContext::new();
println!("Trace ID: {}", trace_ctx.trace_id);
println!("Span ID: {}", trace_ctx.span_id);

// Log with tracing context
tracing::info!(
    trace_id = %trace_ctx.trace_id,
    span_id = %trace_ctx.span_id,
    "Processing GraphQL query"
);
```text
<!-- Code example in TEXT -->

### W3C Trace Context Headers

```rust
<!-- Code example in RUST -->
use fraiseql_server::TraceContext;

// Create trace
let trace = TraceContext::new();

// Generate W3C traceparent header for HTTP requests
let header = trace.to_w3c_traceparent();
println!("traceparent: {}", header);
// Output: "traceparent: 00-abc123def456789012345678901234ab-fedcba9876543210-01"

// Send downstream
// GET /api/users HTTP/1.1
// traceparent: 00-abc123def456789012345678901234ab-fedcba9876543210-01

// Parse incoming header
let incoming_header = "00-abc123def456789012345678901234ab-fedcba9876543210-01";
let downstream_trace = TraceContext::from_w3c_traceparent(incoming_header)
    .expect("Valid traceparent header");

// Same trace, different span
assert_eq!(downstream_trace.trace_id, trace.trace_id);
assert_eq!(downstream_trace.parent_span_id, Some(trace.span_id));
```text
<!-- Code example in TEXT -->

### Baggage for Cross-cutting Context

```rust
<!-- Code example in RUST -->
use fraiseql_server::TraceContext;

// Add authentication and organizational context
let trace = TraceContext::new()
    .with_baggage("user_id".to_string(), "user_456".to_string())
    .with_baggage("tenant_id".to_string(), "tenant_789".to_string())
    .with_baggage("request_priority".to_string(), "high".to_string());

// Access baggage
println!("User: {}", trace.baggage_item("user_id").unwrap_or("unknown"));

// Baggage inherited by child spans
let child = trace.child_span();
assert_eq!(child.baggage_item("user_id"), Some("user_456"));
assert_eq!(child.baggage_item("tenant_id"), Some("tenant_789"));
```text
<!-- Code example in TEXT -->

### Span Lifecycle

```rust
<!-- Code example in RUST -->
use fraiseql_server::{TraceSpan, SpanStatus, TraceEvent};

let trace_id = "trace-12345".to_string();

// Create span
let mut span = TraceSpan::new(trace_id, "ExecuteQuery".to_string())
    .with_parent_span("parent-span-id".to_string())
    .add_attribute("query_type".to_string(), "SELECT".to_string());

// Record events during execution
span = span.add_event(
    TraceEvent::new("parse_complete".to_string())
);

span = span.add_event(
    TraceEvent::new("validation_complete".to_string())
);

span = span.add_event(
    TraceEvent::new("execution_start".to_string())
        .with_attribute("row_count".to_string(), "42".to_string())
);

// Mark completion
span.finish();
println!("Duration: {:?}ms", span.duration_ms());

// Success status
span = span.set_ok();
```text
<!-- Code example in TEXT -->

### Error Tracking in Traces

```rust
<!-- Code example in RUST -->
use fraiseql_server::{TraceSpan, SpanStatus};

let mut span = TraceSpan::new("trace-123".to_string(), "DatabaseQuery".to_string());

// Something fails
let error_result = "SELECT * FROM nonexistent_table".parse::<String>();

if let Err(e) = error_result {
    // Record error in span
    span = span.set_error(format!("SQL Parse Error: {}", e));
}

span.finish();

// Check status
match &span.status {
    SpanStatus::Error { message } => {
        println!("Query failed: {}", message);
    },
    _ => println!("Unexpected status")
}
```text
<!-- Code example in TEXT -->

### Request Context Integration

```rust
<!-- Code example in RUST -->
use fraiseql_server::{TraceContext, RequestContext, RequestId};

// Create request-scoped context
let trace = TraceContext::new();
let mut request_ctx = RequestContext::new()
    .with_operation("GetUser".to_string())
    .with_user_id("user123".to_string());

// Correlate trace and request
tracing::info!(
    trace_id = %trace.trace_id,
    span_id = %trace.span_id,
    operation = %request_ctx.operation.as_ref().unwrap_or(&"unknown".to_string()),
    user_id = %request_ctx.user_id.as_ref().unwrap_or(&"unknown".to_string()),
    "Query initiated"
);
```text
<!-- Code example in TEXT -->

## Integration with Tracing Systems

### Jaeger (OpenTelemetry)

Export trace context to Jaeger:

```rust
<!-- Code example in RUST -->
use fraiseql_server::TraceContext;

let trace = TraceContext::new();

// Send to Jaeger collector
// POST http://jaeger-collector:14268/api/traces
let payload = serde_json::json!({
    "traceID": trace.trace_id,
    "spans": [
        {
            "spanID": trace.span_id,
            "operationName": "GraphQLQuery",
            "startTime": chrono::Utc::now().timestamp_millis(),
            "tags": [
                {"key": "span.kind", "vStr": "internal"},
                {"key": "component", "vStr": "FraiseQL"}
            ]
        }
    ]
});
```text
<!-- Code example in TEXT -->

### Zipkin

Export to Zipkin:

```rust
<!-- Code example in RUST -->
use fraiseql_server::TraceContext;

let trace = TraceContext::new();

// Format for Zipkin v2 API
let span = serde_json::json!({
    "traceId": trace.trace_id,
    "id": trace.span_id,
    "name": "query",
    "timestamp": chrono::Utc::now().timestamp_micros(),
    "tags": {
        "http.status_code": "200",
        "span.kind": "SPAN_KIND_INTERNAL"
    }
});
```text
<!-- Code example in TEXT -->

### Datadog

Integrate with Datadog:

```rust
<!-- Code example in RUST -->
use fraiseql_server::TraceContext;

let trace = TraceContext::new();

// Use as correlation ID in Datadog
tracing::info!(
    dd_trace_id = %trace.trace_id,
    dd_span_id = %trace.span_id,
    "Event for Datadog correlation"
);
```text
<!-- Code example in TEXT -->

### Custom Backend

Implement custom trace exporter:

```rust
<!-- Code example in RUST -->
use fraiseql_server::{TraceContext, TraceSpan};

pub struct CustomTraceExporter;

impl CustomTraceExporter {
    pub async fn export_span(span: TraceSpan) -> Result<(), String> {
        // Convert to your backend format
        let payload = serde_json::json!({
            "trace_id": span.trace_id,
            "span_id": span.span_id,
            "operation": span.operation,
            "duration_ms": span.duration_ms(),
            "status": span.status.to_string(),
        });

        // Send to backend
        // POST to custom collector
        Ok(())
    }
}
```text
<!-- Code example in TEXT -->

## W3C Trace Context Format

FraiseQL uses the W3C Trace Context standard for interoperability:

```text
<!-- Code example in TEXT -->
Header: traceparent
Format: version-traceid-spanid-traceflags

Example:
traceparent: 00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01

Components:

- 00: Version (always 00 for current version)
- 0af7651916cd43dd8448eb211c80319c: Trace ID (32 hex digits)
- b7ad6b7169203331: Span ID (16 hex digits)
- 01: Trace flags (2 hex digits)
  - 0x01: Sampled
  - 0x00: Not sampled
```text
<!-- Code example in TEXT -->

## Sampling Strategy

Control trace sampling for high-volume scenarios:

```rust
<!-- Code example in RUST -->
use fraiseql_server::TraceContext;

// Sample all traces
let trace = TraceContext::new();
assert!(trace.is_sampled());

// Disable sampling
let mut trace = TraceContext::new();
trace.set_sampled(false);
assert!(!trace.is_sampled());

// Implement sampling policy
fn should_sample(user_id: Option<&str>, priority: Option<&str>) -> bool {
    // Always sample authenticated users
    if user_id.is_some() {
        return true;
    }

    // Always sample high-priority requests
    if priority == Some("high") {
        return true;
    }

    // Sample 10% of other requests
    rand::random::<f64>() < 0.1
}
```text
<!-- Code example in TEXT -->

## Performance Considerations

### Memory Usage

- TraceContext: ~400 bytes (including baggage)
- TraceSpan: ~600 bytes (with events)
- Negligible overhead per request

### CPU Usage

- Trace ID generation: < 1 microsecond
- Header parsing: < 5 microseconds
- No blocking operations

## Best Practices

1. **Always Create Traces**: Every request should have a trace ID
2. **Propagate Headers**: Forward W3C headers to downstream services
3. **Add Context**: Use baggage for authentication and organizational data
4. **Record Events**: Document significant operations in spans
5. **Monitor Sampling**: Adjust sampling based on traffic patterns
6. **Correlate Logs**: Include trace ID in all log entries
7. **Set Timeouts**: Configure appropriate spans timeouts
8. **Handle Errors**: Record error details in span status

## Troubleshooting

### Trace Not Appearing

- Verify trace ID propagation across services
- Check W3C header format: `00-{32-hex}-{16-hex}-{2-hex}`
- Ensure sampled flag is set correctly
- Verify backend collector is receiving traces

### Missing Span Context

- Check if TraceContext was created before span
- Verify parent_span_id is set correctly
- Ensure baggage is preserved across service boundaries

## Testing

All tracing components are fully tested:

```bash
<!-- Code example in BASH -->
# Run tracing tests
cargo test -p FraiseQL-server --lib tracing

# Run all tests
cargo test -p FraiseQL-server --lib
```text
<!-- Code example in TEXT -->

## Future Enhancements

- OpenTelemetry integration
- Automatic instrumentation
- Metrics export from traces
- Performance profiling via traces
- Distributed sampling strategies
- Automatic error grouping
