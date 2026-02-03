# FraiseQL v2 Observability & Monitoring Guide

## Overview

FraiseQL v2 provides a comprehensive, production-ready observability stack that enables real-time monitoring, performance analysis, debugging, and operational excellence. The observability system integrates Prometheus metrics, structured JSON logging, distributed tracing, and performance monitoring into a cohesive platform.

## Table of Contents

1. [Quick Start](#quick-start)
2. [Architecture](#architecture)
3. [Monitoring Stack Components](#monitoring-stack-components)
4. [Integration Patterns](#integration-patterns)
5. [Deployment Configuration](#deployment-configuration)
6. [Alerting and SLOs](#alerting-and-slos)
7. [Best Practices](#best-practices)
8. [Troubleshooting](#troubleshooting)

## Quick Start

### Minimal Setup

```bash
# 1. Start FraiseQL Server
RUST_LOG=info cargo run -p fraiseql-server

# 2. Access metrics endpoint
curl http://localhost:8000/metrics

# 3. Access health check
curl http://localhost:8000/health

# 4. View introspection schema
curl http://localhost:8000/introspection
```

### With Prometheus + Grafana

```bash
# 1. Start Docker services
docker-compose -f docker-compose.yml up -d

# 2. Access Grafana
open http://localhost:3000

# 3. Add Prometheus datasource
# - URL: http://prometheus:9090
# - Default

# 4. Import dashboard
# - URL: file://monitoring/grafana-dashboard.json
```

### Accessing Metrics

```bash
# Prometheus text format (scrape by Prometheus)
curl http://localhost:8000/metrics

# JSON format (for dashboards/APIs)
curl http://localhost:8000/metrics/json

# Example response:
# {
#   "queries_total": 1250,
#   "queries_success": 1200,
#   "queries_error": 50,
#   "avg_query_duration_ms": 23.5,
#   "cache_hit_ratio": 0.65
# }
```

## OpenTelemetry Integration (Phase 5 Cycle 3)

### Initialization

FraiseQL v2 provides full **OpenTelemetry** integration for distributed tracing and observability:

```rust
use fraiseql_server::observability;

// Initialize OpenTelemetry at startup
#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracer, metrics, and logging
    observability::init_observability()?;

    // Now all requests will be automatically traced
    start_server().await
}
```

### W3C Trace Context Format

FraiseQL uses the **W3C Trace Context** standard for cross-service tracing:

```
traceparent: 00-{trace-id}-{span-id}-{trace-flags}
            └──────────┬──────────┘
            Example: 00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01
```

**Components**:

- **Version** (2 hex digits): `00` (v1)
- **Trace ID** (32 hex digits): Unique request identifier across all services
  - Generated: `uuid()` as 32-char hex (e.g., `4bf92f3577b34da6a3ce929d0e0e4736`)
- **Span ID** (16 hex digits): Unique operation within trace
  - Generated: `uuid()` as 16-char hex (e.g., `00f067aa0ba902b7`)
- **Trace Flags** (2 hex digits): `01` = sampled, `00` = not sampled

### Trace Context Propagation

Automatic trace context propagation across service boundaries:

```rust
use fraiseql_server::observability::context::{TraceContext, get_context, set_context};

// In request handler
let incoming_traceparent = req.headers().get("traceparent")?;
let trace_ctx = TraceContext::from_traceparent(incoming_traceparent)?;

// Store in thread-local context
set_context(trace_ctx.clone());

// Execute query (trace ID automatically included in all logs/spans)
let result = execute_query(query).await?;

// Create child span for downstream call
let child_span = trace_ctx.child();
let downstream_traceparent = child_span.traceparent_header();

// Call downstream service with trace propagation
client.call(service, request)
    .header("traceparent", downstream_traceparent)
    .await?;

// Clear context after request
clear_context();
```

### Span Creation and Management

Use the **SpanBuilder** pattern for creating instrumentation:

```rust
use fraiseql_server::observability::tracing::{SpanBuilder, SpanStatus};

// Create a span with attributes
let span = SpanBuilder::new("execute_query")
    .with_attribute("operation", "GetUser")
    .with_attribute("database", "postgres")
    .with_attribute("cache", "hit")
    .build();

// Status after execution
if query_succeeded {
    span.set_status(SpanStatus::Ok);
} else {
    span.set_status(SpanStatus::Error);
}
```

### Structured Logging with Trace Correlation

All logs automatically include trace context:

```rust
use fraiseql_server::observability::logging::{LogEntry, LogLevel};

let entry = LogEntry::new(LogLevel::Info, "Query executed")
    .with_duration_ms(45.2)
    .with_field("operation", "GetUser")
    .with_field("rows", "142");

// Automatically includes:
// - timestamp
// - level (INFO)
// - message
// - trace_id (from context)
// - span_id (from context)
// - all custom fields
// Output: JSON to stdout
```

**Example JSON Output**:
```json
{
  "timestamp": "2026-01-31T12:34:56.789Z",
  "level": "INFO",
  "message": "Query executed",
  "trace_id": "4bf92f3577b34da6a3ce929d0e0e4736",
  "span_id": "00f067aa0ba902b7",
  "duration_ms": 45.2,
  "operation": "GetUser",
  "rows": 142
}
```

### Metrics Collection

Metrics automatically tracked for:

```rust
use fraiseql_server::observability::metrics::MetricsCollector;

let collector = MetricsCollector::new();

// Record each request
collector.record_request(
    duration_ms: 45,
    is_error: false
);

// Get summary with Prometheus format
let summary = collector.summary();
// Output includes:
// - graphql_requests_total {count}
// - graphql_errors_total {count}
// - graphql_duration_ms {average}
```

---

## Architecture

### Three-Layer Observability Stack

```
┌─────────────────────────────────────────────────────────────┐
│                    Visualization Layer                       │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐      │
│  │    Grafana   │  │   Kibana     │  │   DataDog    │      │
│  └──────────────┘  └──────────────┘  └──────────────┘      │
└─────────────────────────────────────────────────────────────┘
                            ↑
┌─────────────────────────────────────────────────────────────┐
│                   Backend/Aggregation Layer                  │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐      │
│  │  Prometheus  │  │ Elasticsearch│  │   Jaeger     │      │
│  └──────────────┘  └──────────────┘  └──────────────┘      │
└─────────────────────────────────────────────────────────────┘
                            ↑
┌─────────────────────────────────────────────────────────────┐
│              FraiseQL Server Observability                   │
│  ┌──────────────────────────────────────────────────────┐   │
│  │             Metrics                                  │   │
│  │  - Query execution time, throughput, errors          │   │
│  │  - Database performance, connection pools            │   │
│  │  - Cache hit rates and efficiency                    │   │
│  └──────────────────────────────────────────────────────┘   │
│  ┌──────────────────────────────────────────────────────┐   │
│  │          Structured JSON Logging                      │   │
│  │  - Request context and correlation                   │   │
│  │  - Performance metrics in every log entry            │   │
│  │  - Error details with stack traces                   │   │
│  └──────────────────────────────────────────────────────┘   │
│  ┌──────────────────────────────────────────────────────┐   │
│  │         Distributed Tracing (W3C)                     │   │
│  │  - Request correlation across services               │   │
│  │  - Span tracking and event recording                 │   │
│  │  - Cross-cutting context via baggage                 │   │
│  └──────────────────────────────────────────────────────┘   │
│  ┌──────────────────────────────────────────────────────┐   │
│  │         Performance Monitoring                        │   │
│  │  - Slow query detection and analysis                 │   │
│  │  - Operation profiling                               │   │
│  │  - Cache efficiency tracking                         │   │
│  └──────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────┘
```

## Monitoring Stack Components

### 1. Prometheus Metrics

**Purpose**: Real-time metric collection and time-series analysis

**Key Metrics**:

- `fraiseql_graphql_queries_total`: Total queries executed
- `fraiseql_graphql_queries_success`: Successful queries
- `fraiseql_graphql_queries_error`: Failed queries
- `fraiseql_graphql_query_duration_ms`: Average query duration
- `fraiseql_database_queries_total`: Database operations
- `fraiseql_cache_hit_ratio`: Cache efficiency (0-1)
- `fraiseql_validation_errors_total`: Schema validation errors
- `fraiseql_parse_errors_total`: Query parse errors
- `fraiseql_execution_errors_total`: Execution errors

**Endpoints**:

- `/metrics` - Prometheus text format (for scraping)
- `/metrics/json` - JSON format (for dashboards)

**See Also**: [Metrics Reference](./reference/metrics.md) for detailed metrics

### 2. Structured JSON Logging

**Purpose**: Contextual logging for debugging and analysis

**Log Entry Structure**:

```json
{
  "timestamp": "2024-01-16T15:30:45.123Z",
  "level": "INFO",
  "message": "GraphQL query executed",
  "request_context": {
    "request_id": "550e8400-e29b-41d4-a716-446655440000",
    "operation": "GetUser",
    "user_id": "user123",
    "client_ip": "203.0.113.42"
  },
  "metrics": {
    "duration_ms": 23.5,
    "db_queries": 2,
    "cache_hit": true
  },
  "error": null
}
```

**See Also**: [Structured Logging Guide](./structured-logging.md)

### 3. Distributed Tracing

**Purpose**: Request correlation across service boundaries

**W3C Trace Context Header Format**:

```
traceparent: 00-{32-hex-trace-id}-{16-hex-span-id}-{trace-flags}
```

**Key Features**:

- Automatic trace ID generation
- Parent-child span relationships
- Cross-cutting context via baggage
- W3C standard compliant

**See Also**: [Distributed Tracing Guide](./distributed-tracing.md)

### 4. Performance Monitoring

**Purpose**: Detailed performance analysis and optimization

**Tracking**:

- Query execution phases (parse, validation, DB, formatting)
- Slow query detection and analysis
- Cache efficiency analysis
- Operation-specific profiling
- Database query performance

**See Also**: [Observability Architecture](./observability-architecture.md)

## Integration Patterns

### Pattern 1: Request Lifecycle Tracing

Track a single request through the entire execution pipeline:

```rust
use fraiseql_server::{
    TraceContext, RequestContext, PerformanceMonitor,
    StructuredLogEntry, LogLevel, LogMetrics
};

// 1. Create trace context (at entry point)
let trace = TraceContext::new();
let request_ctx = RequestContext::new()
    .with_operation("GetUser".to_string());

// 2. Create performance monitor
let perf_monitor = PerformanceMonitor::new(100.0); // 100ms threshold

// 3. Execute query
let query_perf = execute_query(...);
perf_monitor.record_query(query_perf.clone());

// 4. Log with all context
let entry = StructuredLogEntry::new(
    LogLevel::Info,
    "Query executed successfully".to_string()
)
.with_request_context(request_ctx)
.with_metrics(LogMetrics::new()
    .with_duration_ms(query_perf.duration_us as f64 / 1000.0)
    .with_db_queries(query_perf.db_queries)
    .with_cache_hit(query_perf.cached));

tracing::info!("{}", entry.to_json_string());
```

### Pattern 2: Service-to-Service Tracing

Propagate trace context across service boundaries:

```rust
use fraiseql_server::TraceContext;

// Upstream service receives request with trace
let incoming_header = req.headers().get("traceparent").unwrap();
let trace = TraceContext::from_w3c_traceparent(incoming_header)?;

// Add baggage for downstream
let trace_with_context = trace
    .with_baggage("user_id".to_string(), user.id.clone())
    .with_baggage("tenant".to_string(), user.tenant.clone());

// Downstream call
let child_trace = trace_with_context.child_span();
downstream_service.call(
    client,
    request,
    headers.insert("traceparent", child_trace.to_w3c_traceparent())
)?;
```

### Pattern 3: Performance Analysis Dashboard

Real-time monitoring with Grafana:

```bash
# 1. Configure Grafana datasource
curl -X POST http://localhost:3000/api/datasources \
  -H "Content-Type: application/json" \
  -d '{
    "name": "Prometheus",
    "type": "prometheus",
    "url": "http://prometheus:9090",
    "access": "proxy",
    "isDefault": true
  }'

# 2. Import dashboard
curl -X POST http://localhost:3000/api/dashboards/db \
  -H "Content-Type: application/json" \
  -d @monitoring/grafana-dashboard.json
```

### Pattern 4: Alerting Rules

Set up Prometheus alerting:

```yaml
# prometheus/alerts.yml
groups:
  - name: fraiseql
    rules:
      # High error rate
      - alert: HighErrorRate
        expr: |
          (rate(fraiseql_graphql_queries_error[5m]) /
           rate(fraiseql_graphql_queries_total[5m])) > 0.05
        annotations:
          summary: "Error rate above 5%"

      # High latency
      - alert: HighLatency
        expr: fraiseql_graphql_query_duration_ms > 500
        annotations:
          summary: "Query latency above 500ms"

      # Low cache hit rate
      - alert: LowCacheHitRate
        expr: fraiseql_cache_hit_ratio < 0.50
        annotations:
          summary: "Cache hit rate below 50%"
```

## Deployment Configuration

### Docker Compose (Development)

```yaml
version: '3.8'
services:
  fraiseql:
    image: fraiseql-server:latest
    ports:
      - "8000:8000"
    environment:
      DATABASE_URL: postgresql://user:pass@postgres:5432/db
      RUST_LOG: info
    depends_on:
      - postgres

  postgres:
    image: postgres:15
    environment:
      POSTGRES_PASSWORD: password
    volumes:
      - postgres_data:/var/lib/postgresql/data

  prometheus:
    image: prom/prometheus:latest
    ports:
      - "9090:9090"
    volumes:
      - ./monitoring/prometheus.yml:/etc/prometheus/prometheus.yml
    command:
      - '--config.file=/etc/prometheus/prometheus.yml'

  grafana:
    image: grafana/grafana:latest
    ports:
      - "3000:3000"
    depends_on:
      - prometheus

volumes:
  postgres_data:
```

### Kubernetes (Production)

```yaml
apiVersion: v1
kind: ConfigMap
metadata:
  name: fraiseql-config
data:
  config.toml: |
    bind_addr = "0.0.0.0:8000"
    database_url = "postgresql://..."
    pool_min_size = 5
    pool_max_size = 20

---
apiVersion: apps/v1
kind: Deployment
metadata:
  name: fraiseql-server
spec:
  replicas: 3
  selector:
    matchLabels:
      app: fraiseql
  template:
    metadata:
      labels:
        app: fraiseql
    spec:
      containers:
      - name: fraiseql
        image: fraiseql-server:latest
        ports:
        - containerPort: 8000
        env:
        - name: RUST_LOG
          value: info
        livenessProbe:
          httpGet:
            path: /health
            port: 8000
          initialDelaySeconds: 10
          periodSeconds: 10
        readinessProbe:
          httpGet:
            path: /health
            port: 8000
          initialDelaySeconds: 5
          periodSeconds: 5
```

## Alerting and SLOs

### Recommended Alert Thresholds

| Metric | Threshold | Severity | Action |
|--------|-----------|----------|--------|
| Query Error Rate | > 5% | Warning | Investigate |
| Query Error Rate | > 10% | Critical | Page on-call |
| Query Latency p95 | > 200ms | Warning | Analyze |
| Query Latency p95 | > 1s | Critical | Page on-call |
| Cache Hit Rate | < 50% | Warning | Review caching |
| Database Time % | > 80% | Warning | Optimize queries |
| Server Error Rate | > 1% | Critical | Page on-call |

### Sample SLOs

```
Service Level Objectives for FraiseQL v2:

1. Availability SLO: 99.95% (4 hours/month downtime)
   - Alert if: error_rate > 0.5% for 5 minutes

2. Latency SLO: 95th percentile < 200ms
   - Alert if: p95_latency > 250ms for 10 minutes

3. Cache Efficiency: > 60% hit rate
   - Alert if: cache_hit_ratio < 50% for 30 minutes

4. Query Success: > 99.9%
   - Alert if: success_rate < 99% for 5 minutes
```

## Best Practices

### 1. Logging Best Practices

✅ **DO:**

- Include request IDs in all log entries
- Log at appropriate levels (DEBUG/TRACE only in development)
- Include business context (user_id, operation, tenant)
- Use structured JSON format

❌ **DON'T:**

- Log sensitive data (passwords, tokens, PII)
- Use vague error messages
- Mix structured and unstructured logs
- Omit context information

### 2. Metrics Best Practices

✅ **DO:**

- Use consistent metric names
- Export metrics every 30-60 seconds
- Track both success and error cases
- Monitor resource usage (CPU, memory, connections)

❌ **DON'T:**

- Create unbounded cardinality metrics
- Export sensitive information
- Change metric names without versioning
- Track PII in metrics

### 3. Tracing Best Practices

✅ **DO:**

- Create traces at system boundaries
- Propagate trace IDs across services
- Sample appropriately for traffic volume
- Set meaningful span names

❌ **DON'T:**

- Create traces for every operation
- Store sensitive data in baggage
- Forget to finish spans
- Use verbose log messages in spans

### 4. Performance Monitoring

✅ **DO:**

- Monitor slow query rate
- Track cache efficiency
- Analyze database performance
- Use performance data for optimization

❌ **DON'T:**

- Ignore performance trends
- Rely on averages alone (use percentiles)
- Skip error analysis
- Assume caching is always beneficial

## Troubleshooting

### Metrics Not Appearing in Prometheus

**Symptoms**: `/metrics` endpoint works but Prometheus scrape fails

**Solutions**:

1. Check Prometheus configuration: `curl http://prometheus:9090/-/healthy`
2. Verify target is reachable: `telnet fraiseql 8000`
3. Check scrape interval: Default is 15 seconds
4. Look for errors in Prometheus logs

### Missing Log Entries

**Symptoms**: Some requests not logged

**Solutions**:

1. Check log level: Set `RUST_LOG=debug` for verbose logging
2. Verify sink is configured correctly
3. Check for log buffering: May have 1-2 second delay
4. Ensure application isn't crashing silently

### Trace Context Lost Between Services

**Symptoms**: Trace IDs not propagating

**Solutions**:

1. Verify `traceparent` header is being set
2. Check header format: `00-{32-hex}-{16-hex}-{2-hex}`
3. Ensure services parse and propagate headers
4. Check for header case sensitivity

### High Memory Usage

**Symptoms**: Server memory grows over time

**Solutions**:

1. Check for trace context leaks
2. Monitor baggage size (should be < 1KB per request)
3. Verify metrics aren't accumulating unbounded
4. Enable profile memory: `RUST_BACKTRACE=1`

## Additional Resources

- [Metrics Reference](./reference/metrics.md)
- [Structured Logging Guide](./structured-logging.md)
- [Distributed Tracing Guide](./distributed-tracing.md)
- [Observability Architecture](./observability-architecture.md)

## Support

For issues or questions about observability in FraiseQL v2:

- Check [troubleshooting section](#troubleshooting)
- Review logs with `RUST_LOG=debug`
- Enable tracing for detailed execution flow
- File an issue with metrics/logs/traces attached
