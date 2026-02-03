# Phase 9.1.E - Jaeger Integration Guide

**Status**: Implementation Complete
**Last Updated**: January 22, 2026

---

## Overview

Phase 9.1.E completes the integration with Jaeger backend for trace storage, visualization, and analysis. This guide covers Jaeger setup, configuration, and usage patterns.

---

## Quick Start - Jaeger Setup

### Step 1: Start Jaeger All-in-One

```bash
# Pull and run Jaeger all-in-one container
docker pull jaegertracing/all-in-one:latest

docker run -d \
  --name jaeger \
  -p 5775:5775/udp \
  -p 6831:6831/udp \
  -p 6832:6832/udp \
  -p 5778:5778 \
  -p 16686:16686 \
  -p 14268:14268 \
  -p 14250:14250 \
  -p 9411:9411 \
  jaegertracing/all-in-one:latest

# Verify Jaeger is running
curl http://localhost:16686/api/services
```

### Step 2: Configure Observer System

```bash
# Set environment variables
export TRACING_ENABLED=true
export TRACING_SERVICE_NAME=fraiseql-observer
export JAEGER_ENDPOINT=http://localhost:14268/api/traces
export JAEGER_SAMPLE_RATE=1.0
```

### Step 3: Initialize Tracing

```rust
use fraiseql_observers::tracing::{init_tracing, TracingConfig};

#[tokio::main]
async fn main() -> Result<()> {
    // Load configuration from environment
    let config = TracingConfig::from_env()?;

    // Initialize Jaeger exporter
    init_tracing(config)?;

    // Your observer code here
    run_observer().await?;

    Ok(())
}
```

### Step 4: View Traces in Jaeger UI

1. **Open** http://localhost:16686 in your browser
2. **Select service** from dropdown (should show `fraiseql-observer`)
3. **Click "Find Traces"**
4. **Explore** trace hierarchy, timing, and logs

---

## Jaeger Configuration

### JaegerConfig Structure

```rust
pub struct JaegerConfig {
    /// Jaeger HTTP endpoint
    pub endpoint: String,

    /// Sampling rate (0.0 to 1.0)
    pub sample_rate: f64,

    /// Service name
    pub service_name: String,

    /// Max batch size for export
    pub max_batch_size: usize,

    /// Export timeout in milliseconds
    pub export_timeout_ms: u64,
}
```

### Configuration via Environment

```bash
# Basic configuration
export JAEGER_ENDPOINT=http://localhost:14268/api/traces
export TRACING_SERVICE_NAME=observer-service
export JAEGER_SAMPLE_RATE=0.1  # Trace 10% of events

# Advanced configuration (optional)
export JAEGER_MAX_BATCH_SIZE=512
export JAEGER_EXPORT_TIMEOUT_MS=30000
```

### Configuration via YAML

```yaml
# config.yaml
tracing:
  enabled: true
  service_name: fraiseql-observer

jaeger:
  endpoint: http://localhost:14268/api/traces
  sample_rate: 0.1
  max_batch_size: 512
  export_timeout_ms: 30000
```

### Programmatic Configuration

```rust
use fraiseql_observers::tracing::{init_jaeger_exporter, JaegerConfig};

let config = JaegerConfig {
    endpoint: "http://jaeger.example.com:14268/api/traces".to_string(),
    sample_rate: 0.1,
    service_name: "production-observer".to_string(),
    max_batch_size: 512,
    export_timeout_ms: 30000,
};

config.validate()?;
init_jaeger_exporter(&config)?;
```

---

## Working with Traces

### Viewing Trace Details

1. **Service List Page**:
   - Shows all services sending traces
   - Real-time trace count
   - Error rate per service

2. **Trace Search**:
   - Filter by operation name
   - Filter by duration
   - Filter by tags
   - Search by trace ID

3. **Trace Timeline**:
   - Hierarchical span view
   - Duration bar for each span
   - Color-coded by operation

4. **Span Details**:
   - All tags and logs
   - Duration breakdown
   - Parent/child relationships
   - Error information

### Example Trace Flow

```
Event Processing Trace [150ms total]
├── Root Span: process_event [150ms]
│   └── Tags: event_id=evt-123, entity_type=Order
│       └── Logs: Processing started
│
├── Child Span: condition_evaluation [10ms]
│   └── Tags: observer=order_validator, matched=true
│       └── Logs: Condition matched
│
└── Child Span: execute_action [135ms]
    ├── Child Span: webhook [45ms]
    │   └── Tags: url=https://api.example.com, status=200
    │       └── Logs: Webhook succeeded
    │
    ├── Child Span: email [80ms]
    │   └── Tags: recipient=user@example.com, message_id=msg-123
    │       └── Logs: Email sent
    │
    └── Child Span: slack [10ms]
        └── Tags: channel=#notifications, status=200
            └── Logs: Slack message posted
```

---

## Span Recording

### Recording Spans Programmatically

```rust
use fraiseql_observers::tracing::{record_span, JaegerSpan};

let span = JaegerSpan {
    trace_id: "a".repeat(32),
    span_id: "b".repeat(16),
    parent_span_id: None,
    operation_name: "process_event".to_string(),
    start_time_ms: 1000,
    duration_ms: 150,
    tags: vec![
        ("event_id".to_string(), "evt-123".to_string()),
        ("entity_type".to_string(), "Order".to_string()),
        ("status".to_string(), "success".to_string()),
    ],
    status: "ok".to_string(),
};

// Record span for export
record_span(span)?;
```

### Child Span Recording

```rust
let root_span = JaegerSpan {
    trace_id: "a".repeat(32),
    span_id: "b".repeat(16),
    parent_span_id: None,
    operation_name: "process_event".to_string(),
    start_time_ms: 1000,
    duration_ms: 150,
    tags: vec![],
    status: "ok".to_string(),
};

let child_span = JaegerSpan {
    trace_id: "a".repeat(32),      // Same trace ID
    span_id: "c".repeat(16),       // Different span ID
    parent_span_id: Some("b".repeat(16)),  // Parent span ID
    operation_name: "execute_action".to_string(),
    start_time_ms: 1010,           // Started after parent
    duration_ms: 135,
    tags: vec![
        ("action_count".to_string(), "3".to_string()),
    ],
    status: "ok".to_string(),
};

record_span(root_span)?;
record_span(child_span)?;
```

---

## Batch Export

### Automatic Batch Export

Spans are automatically exported when batch reaches configured size:

```rust
// Export happens automatically at:
// 1. Batch size reached (default: 512 spans)
// 2. Timeout elapsed
// 3. Explicit flush() call
```

### Manual Flush

```rust
use fraiseql_observers::tracing::flush_spans;

// Force export of pending spans
flush_spans()?;

// Call at application shutdown
impl Drop for ObserverSystem {
    fn drop(&mut self) {
        let _ = flush_spans();
    }
}
```

---

## Sampling Strategies

### No Sampling (Development)

```bash
# Trace all events (100%)
export JAEGER_SAMPLE_RATE=1.0

# Use this for:
# - Local development
# - Debugging specific issues
# - Low-traffic environments
```

### Light Sampling (Staging)

```bash
# Trace 10% of events
export JAEGER_SAMPLE_RATE=0.1

# Use this for:
# - Staging/pre-production
# - Moderate traffic
# - Cost optimization
```

### Heavy Sampling (Production)

```bash
# Trace 1% of events
export JAEGER_SAMPLE_RATE=0.01

# Use this for:
# - Production systems
# - High-traffic scenarios
# - Long-term monitoring
```

### Selective Sampling

```rust
// Sample based on event type
let sample_rate = match event.entity_type {
    "HighValue" => 1.0,   // Always trace high-value events
    "Standard" => 0.1,    // Trace 10% of standard
    "Low" => 0.01,        // Trace 1% of low priority
};

// Sample based on errors
let sample_rate = if result.is_err() {
    1.0  // Always trace errors
} else {
    0.01  // Rarely trace successes
};
```

---

## Performance Analysis

### Latency Percentiles

View **Statistics** tab in Jaeger to see:

- **P50**: Median latency
- **P95**: 95th percentile latency
- **P99**: 99th percentile latency

Use to identify performance regressions.

### Service Dependencies

View **Service Graph** in Jaeger to:

- See which services call which
- Identify bottlenecks
- Monitor dependency health

### Error Analysis

Filter by **error=true** to:

- See all failed operations
- Identify error patterns
- Track error rates over time

---

## Production Deployment

### Docker Compose Setup

```yaml
version: '3.8'

services:
  jaeger:
    image: jaegertracing/all-in-one:latest
    environment:
      - COLLECTOR_OTLP_ENABLED=true
      - MEMORY_MAX_TRACES=10000
    ports:
      - "16686:16686"
      - "14268:14268"
    volumes:
      - jaeger-data:/badger
    healthcheck:
      test: ["CMD", "curl", "-f", "http://localhost:16686/api/services"]
      interval: 10s
      timeout: 5s
      retries: 3

  observer:
    build: .
    environment:
      TRACING_ENABLED: "true"
      TRACING_SERVICE_NAME: "fraiseql-observer"
      JAEGER_ENDPOINT: "http://jaeger:14268/api/traces"
      JAEGER_SAMPLE_RATE: "0.1"
    depends_on:
      jaeger:
        condition: service_healthy
    ports:
      - "8080:8080"

volumes:
  jaeger-data:
```

### Kubernetes Deployment

```yaml
apiVersion: v1
kind: ConfigMap
metadata:
  name: observer-config
data:
  TRACING_ENABLED: "true"
  TRACING_SERVICE_NAME: "fraiseql-observer"
  JAEGER_ENDPOINT: "http://jaeger-collector.jaeger:14268/api/traces"
  JAEGER_SAMPLE_RATE: "0.1"

---
apiVersion: apps/v1
kind: Deployment
metadata:
  name: observer
spec:
  replicas: 3
  template:
    metadata:
      labels:
        app: observer
    spec:
      containers:
      - name: observer
        image: fraiseql-observer:latest
        envFrom:
        - configMapRef:
            name: observer-config
        ports:
        - containerPort: 8080
```

---

## Troubleshooting

### No Traces Appearing

**Check 1**: Is Jaeger running?
```bash
curl http://localhost:16686/api/services
# Should return: {"services":["fraiseql-observer"]}
```

**Check 2**: Is tracing enabled?
```bash
echo $TRACING_ENABLED
# Should be: true
```

**Check 3**: Is endpoint correct?
```bash
curl http://localhost:14268/api/traces
# Should succeed (empty trace response is OK)
```

### Traces Not Exported

**Check**: Call flush_spans() at shutdown
```rust
// Before exiting
flush_spans()?;

// Or in Drop impl
impl Drop for Observer {
    fn drop(&mut self) {
        let _ = flush_spans();
    }
}
```

### Jaeger UI Not Loading

**Check**:

- Port 16686 is accessible
- Docker container is running: `docker ps | grep jaeger`
- No firewall blocking: `telnet localhost 16686`

### High Memory Usage

**Solutions**:

1. Reduce sample rate: `JAEGER_SAMPLE_RATE=0.01`
2. Reduce batch timeout: `JAEGER_EXPORT_TIMEOUT_MS=10000`
3. Limit traces in Jaeger: Set `MEMORY_MAX_TRACES=10000`

---

## Jaeger Version Compatibility

Tested with:

- Jaeger 1.35+
- HTTP collector (port 14268)
- gRPC collector (port 14250)

---

## Next Steps

- **Week 4**: Add action retry tracing
- **Week 5**: Create Grafana dashboards for long-term analysis
- **Week 6**: Set up trace sampling rules
- **Week 7**: Integrate with alerting system

---

## Related Documents

- [Phase 9.1 Design](PHASE_9_1_DESIGN.md)
- [Phase 9.1 Implementation Guide](PHASE_9_1_IMPLEMENTATION_GUIDE.md)
- [Action Tracing Guide](PHASE_9_1_ACTION_TRACING_GUIDE.md)
- [Jaeger Official Docs](https://www.jaegertracing.io/docs/)

---

**Document**: Phase 9.1.E - Jaeger Integration Guide
**Status**: Complete
**Last Updated**: January 22, 2026
