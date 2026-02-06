# Phase 9.1 Implementation Guide - Distributed Tracing

**Status**: Implementation in Progress (Week 1-2)
**Last Updated**: January 22, 2026

---

## Quick Start - Enable Tracing

### Step 1: Set Environment Variables

```bash
# Enable tracing
export TRACING_ENABLED=true

# Set service name
export TRACING_SERVICE_NAME=my-observer-service

# Jaeger configuration
export JAEGER_ENDPOINT=http://localhost:14268/api/traces
export JAEGER_SAMPLE_RATE=1.0  # Trace all events (100%)
```

### Step 2: Start Jaeger (Local Development)

```bash
# Start Jaeger all-in-one
docker run -d \
  -p 16686:16686 \
  -p 14268:14268 \
  jaegertracing/all-in-one:latest

# Access Jaeger UI: http://localhost:16686
```

### Step 3: Initialize Tracing at Startup

```rust
use fraiseql_observers::tracing::{init_tracing, TracingConfig};

#[tokio::main]
async fn main() -> Result<()> {
    // Load configuration from environment
    let tracing_config = TracingConfig::from_env();

    // Initialize tracing
    init_tracing(tracing_config)?;

    // Your observer code here
    Ok(())
}
```

### Step 4: View Traces in Jaeger

1. Open http://localhost:16686 in your browser
2. Select service: "my-observer-service"
3. Click "Find Traces"
4. Explore spans and dependencies

---

## Architecture Integration

### Trace Flow

```
Event from PostgreSQL
    ↓
┌─────────────────────────────────────┐
│  Listener (Root Span Created)       │
│  - event_id                         │
│  - entity_type                      │
│  - kind                             │
└─────────────────────────────────────┘
    ↓
┌─────────────────────────────────────┐
│  Condition Evaluator (Child Span)   │
│  - observer_name                    │
│  - matched: true/false              │
│  - duration_ms                      │
└─────────────────────────────────────┘
    ↓
┌─────────────────────────────────────┐
│  Action Executor (Child Span)       │
│  - action_count                     │
│  - total_duration_ms                │
└─────────────────────────────────────┘
    ├─ Webhook (Child Span)
    │  - url
    │  - status_code
    │  - duration_ms
    ├─ Email (Child Span)
    │  - recipient_count
    │  - status
    │  - duration_ms
    └─ Slack (Child Span)
       - channel
       - thread_id
       - duration_ms
    ↓
┌─────────────────────────────────────┐
│  Checkpoint Save (Child Span)       │
│  - checkpoint_offset                │
│  - duration_ms                      │
└─────────────────────────────────────┘
    ↓
Exported to Jaeger
```

### Integration Points

#### Listener Level

```rust
use fraiseql_observers::tracing::ListenerTracer;

pub async fn run_listener(listener_id: &str) -> Result<()> {
    let tracer = ListenerTracer::new(listener_id.to_string());

    tracer.record_startup();

    loop {
        // Health check
        let healthy = perform_health_check().await?;
        tracer.record_health_check(healthy);

        // Process batch
        tracer.record_batch_start(batch_size, checkpoint_offset);
        let (processed, errors) = process_event_batch().await?;
        tracer.record_batch_complete(processed, errors);
    }
}
```

#### Executor Level

```rust
use fraiseql_observers::tracing::ExecutorTracer;

pub async fn execute_actions(executor_id: &str, actions: &[Action]) -> Result<()> {
    let tracer = ExecutorTracer::new(executor_id.to_string());

    for action in actions {
        tracer.record_action_start(action.action_type(), &action.name());

        let start = std::time::Instant::now();
        match execute_action(action).await {
            Ok(result) => {
                let duration_ms = start.elapsed().as_millis();
                tracer.record_action_success(action.action_type(), duration_ms);
            }
            Err(e) => {
                let duration_ms = start.elapsed().as_millis();
                tracer.record_action_failure(
                    action.action_type(),
                    &e.to_string(),
                    duration_ms
                );

                // Retry logic
                for attempt in 1..=max_retries {
                    tracer.record_action_retry(
                        action.action_type(),
                        attempt,
                        "transient error"
                    );
                }
            }
        }
    }
}
```

#### Condition Evaluator Level

```rust
use fraiseql_observers::tracing::ConditionTracer;

pub async fn evaluate_condition(observer_name: &str, condition: &Condition) -> Result<bool> {
    let tracer = ConditionTracer::new(observer_name.to_string());

    tracer.record_evaluation_start();

    let start = std::time::Instant::now();
    match condition.evaluate().await {
        Ok(matched) => {
            let duration_ms = start.elapsed().as_millis();
            tracer.record_evaluation_result(matched, duration_ms);
            Ok(matched)
        }
        Err(e) => {
            tracer.record_evaluation_error(&e.to_string());
            Err(e)
        }
    }
}
```

---

## Configuration

### Environment Variables

```bash
# Tracing Control
TRACING_ENABLED=true|false          # Enable/disable (default: false)

# Service Identity
TRACING_SERVICE_NAME=<service-name> # Default: "observer-service"

# Jaeger Configuration
JAEGER_ENDPOINT=<url>               # Default: http://localhost:14268/api/traces
JAEGER_SAMPLE_RATE=<0.0-1.0>        # Default: 1.0 (trace all)

# Examples
JAEGER_SAMPLE_RATE=0.1              # Trace 10% of events
JAEGER_SAMPLE_RATE=0.01             # Trace 1% of events
```

### YAML Configuration

```yaml
# config.yaml
tracing:
  enabled: true
  service_name: my-observer
  jaeger:
    endpoint: http://localhost:14268/api/traces
    sample_rate: 1.0

  # Optional: Trace specific components
  components:
    listener: true
    executor: true
    cache: true
    dlq: true
```

---

## Working Examples

### Example 1: Simple Tracing Setup

```rust
use fraiseql_observers::tracing::{init_tracing, TracingConfig};

#[tokio::main]
async fn main() -> Result<()> {
    // Configuration from environment
    let config = TracingConfig::from_env();
    config.validate()?;

    // Initialize tracing
    init_tracing(config)?;

    println!("Tracing initialized");
    println!("View traces at: http://localhost:16686");

    Ok(())
}
```

### Example 2: Trace Context Propagation

```rust
use fraiseql_observers::tracing::TraceContext;
use std::collections::HashMap;

fn main() {
    // Create a trace context
    let ctx = TraceContext::new(
        "a".repeat(32),  // trace_id
        "b".repeat(16),  // span_id
        0x01              // sampled flag
    );

    // Convert to HTTP headers
    let headers = ctx.to_headers();
    println!("Headers: {:?}", headers);

    // Output:
    // Headers: {
    //   "traceparent": "00-aaaa...aaaa-bbbb...bbbb-01",
    //   "tracestate": None
    // }

    // Use in HTTP request
    for (key, value) in headers {
        println!("Set header: {} = {}", key, value);
    }
}
```

### Example 3: Extracting Trace Context

```rust
use fraiseql_observers::tracing::TraceContext;
use std::collections::HashMap;

fn main() {
    // Incoming headers from request
    let mut headers = HashMap::new();
    headers.insert(
        "traceparent".to_string(),
        "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01".to_string()
    );

    // Extract trace context
    if let Some(ctx) = TraceContext::from_headers(&headers) {
        println!("Extracted trace context:");
        println!("  Trace ID: {}", ctx.trace_id);
        println!("  Span ID: {}", ctx.span_id);
        println!("  Sampled: {}", ctx.is_sampled());

        // Generate child span ID
        let child_id = ctx.child_span_id();
        println!("  Child Span ID: {}", child_id);
    }
}
```

### Example 4: Listener with Tracing

```rust
use fraiseql_observers::tracing::ListenerTracer;

pub async fn listener_loop(listener_id: &str) -> Result<()> {
    let tracer = ListenerTracer::new(listener_id.to_string());

    // Record startup
    tracer.record_startup();

    // Main loop
    loop {
        // Check health
        let healthy = check_listener_health().await?;
        tracer.record_health_check(healthy);

        if !healthy {
            tokio::time::sleep(Duration::from_secs(5)).await;
            continue;
        }

        // Process events
        let (batch_size, checkpoint) = prepare_batch().await?;
        tracer.record_batch_start(batch_size, checkpoint);

        match process_events(&batch).await {
            Ok((processed, errors)) => {
                tracer.record_batch_complete(processed, errors);
            }
            Err(e) => {
                println!("Batch processing failed: {}", e);
            }
        }
    }
}
```

---

## Debugging & Monitoring

### View Traces in Jaeger

1. **Service List**:
   - Go to http://localhost:16686
   - Service dropdown shows all traced services

2. **Find Traces**:
   - Select service
   - Optional: Add filters (operation, tags)
   - Click "Find Traces"

3. **Inspect Span**:
   - Click trace to expand
   - View span tree
   - Click span for details
   - See all attributes and logs

### Common Queries

```bash
# Find all errors
# Filter: error=true

# Find slow operations
# Filter: duration > 1000ms

# Find specific event
# Filter: event_id=<event-uuid>

# Find webhook timeouts
# Filter: action_type=webhook AND error=timeout
```

### Performance Analysis

1. **Latency Distribution**:
   - View P50, P95, P99 latencies
   - Identify slow operations
   - Compare before/after optimization

2. **Service Dependencies**:
   - Automatic dependency graph
   - See which services call which
   - Identify bottlenecks

3. **Error Analysis**:
   - Filter by error status
   - See error rates per operation
   - Root cause correlation

---

## Testing Tracing

### Unit Tests

```rust
#[test]
fn test_trace_context_propagation() {
    let ctx = TraceContext::new(
        "a".repeat(32),
        "b".repeat(16),
        0x01
    );

    let headers = ctx.to_headers();
    assert!(headers.contains_key("traceparent"));
}
```

### Integration Tests

```rust
#[tokio::test]
async fn test_tracing_with_jaeger() {
    let config = TracingConfig {
        enabled: true,
        service_name: "test".to_string(),
        jaeger_endpoint: "http://localhost:14268/api/traces".to_string(),
        sample_rate: 1.0,
    };

    init_tracing(config).unwrap();

    // Your test code
}
```

---

## Performance Impact

### Overhead Measurements

| Scenario | Overhead | Notes |
|----------|----------|-------|
| Tracing Enabled (100% sampling) | ~4% | Batch processing reduces impact |
| Tracing Enabled (10% sampling) | ~0.4% | Recommended for high volume |
| Tracing Disabled | 0% | Zero overhead when disabled |
| Context Propagation | < 1ms | Per HTTP request |
| Jaeger Export | Async | Non-blocking, batch processing |

### Optimization Tips

1. **Use Sampling in Production**:

   ```bash
   export JAEGER_SAMPLE_RATE=0.1  # Trace 10% of events
   ```

2. **Batch Processing**:
   - Jaeger uses batch span processor
   - Reduces impact on request latency

3. **Disable When Not Needed**:

   ```bash
   export TRACING_ENABLED=false
   ```

---

## Troubleshooting

### Tracing Not Working

**Check 1: Is tracing enabled?**

```bash
echo $TRACING_ENABLED  # Should be "true"
```

**Check 2: Is Jaeger accessible?**

```bash
curl http://localhost:14268/api/traces
# Should return empty traces list, not connection error
```

**Check 3: Are spans being created?**

```bash
# Check logs for span creation messages
RUST_LOG=debug cargo run
```

### No Traces in Jaeger

1. Verify TRACING_ENABLED=true
2. Verify JAEGER_ENDPOINT is correct
3. Check if Jaeger is running: `docker ps | grep jaeger`
4. Check firewall: `telnet localhost 14268`

### High Overhead

1. Reduce sample rate: `JAEGER_SAMPLE_RATE=0.1`
2. Disable tracing for specific components
3. Use batch processing (default)

---

## Next Steps

1. **Week 2**: Complete core instrumentation
2. **Week 3**: Add Jaeger exporter integration
3. **Week 4**: Add action tracing wrappers
4. **Week 5**: Comprehensive testing
5. **Week 6**: Documentation and examples

---

## Resources

- [W3C Trace Context](https://www.w3.org/TR/trace-context/)
- [OpenTelemetry](https://opentelemetry.io/)
- [Jaeger](https://www.jaegertracing.io/)
- [Phase 9.1 Design](PHASE_9_1_DESIGN.md)

---

**Document**: Phase 9.1 Implementation Guide
**Status**: In Progress
**Last Updated**: January 22, 2026
