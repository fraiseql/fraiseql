# Observability

Complete observability stack for FraiseQL applications with **PostgreSQL-native error tracking, distributed tracing, and metrics**—all in one database.

## Overview

FraiseQL implements the **"In PostgreSQL Everything"** philosophy for observability. Instead of using external services like Sentry, Datadog, or New Relic, all observability data (errors, traces, metrics, business events) is stored in PostgreSQL.

**Benefits:**
- **Cost Savings**: Save $300-3,000/month vs SaaS observability platforms
- **Unified Storage**: All data in one place for easy correlation
- **SQL-Powered**: Query everything with standard SQL
- **Self-Hosted**: Full control, no vendor lock-in
- **ACID Guarantees**: Transactional consistency for observability data

**Observability Stack:**
```
┌─────────────────────────────────────────────────────────┐
│                    PostgreSQL Database                   │
│                                                          │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐ │
│  │   Errors     │  │   Traces     │  │   Metrics    │ │
│  │  (Sentry-    │  │ (OpenTelem-  │  │ (Prometheus  │ │
│  │   like)      │  │   etry)      │  │   or PG)     │ │
│  └──────┬───────┘  └──────┬───────┘  └──────┬───────┘ │
│         │                  │                  │         │
│         └──────────────────┴──────────────────┘         │
│                    Joined via trace_id                   │
│                                                          │
│  ┌──────────────────────────────────────────────────┐  │
│  │         Business Events (tb_entity_change_log)    │  │
│  └──────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────┘
                           │
                           ↓
                    ┌──────────────┐
                    │   Grafana    │
                    │  Dashboards  │
                    └──────────────┘
```

## Table of Contents

- [Error Tracking](#error-tracking)
- [Distributed Tracing](#distributed-tracing)
- [Metrics Collection](#metrics-collection)
- [Correlation](#correlation)
- [Grafana Dashboards](#grafana-dashboards)
- [Query Examples](#query-examples)
- [Performance Tuning](#performance-tuning)
- [Best Practices](#best-practices)

## Error Tracking

PostgreSQL-native error tracking with automatic fingerprinting, grouping, and notifications.

### Schema

```sql
-- Monitoring schema
CREATE SCHEMA IF NOT EXISTS monitoring;

-- Errors table
CREATE TABLE monitoring.errors (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    fingerprint TEXT NOT NULL,
    exception_type TEXT NOT NULL,
    message TEXT NOT NULL,
    stack_trace TEXT,
    context JSONB,
    environment TEXT NOT NULL,
    trace_id TEXT,
    span_id TEXT,
    occurred_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    resolved_at TIMESTAMP WITH TIME ZONE,
    ignored BOOLEAN DEFAULT FALSE,
    assignee TEXT
);

-- Indexes for fast queries
CREATE INDEX idx_errors_fingerprint ON monitoring.errors(fingerprint);
CREATE INDEX idx_errors_occurred_at ON monitoring.errors(occurred_at DESC);
CREATE INDEX idx_errors_environment ON monitoring.errors(environment);
CREATE INDEX idx_errors_trace_id ON monitoring.errors(trace_id) WHERE trace_id IS NOT NULL;
CREATE INDEX idx_errors_context ON monitoring.errors USING GIN(context);
CREATE INDEX idx_errors_unresolved ON monitoring.errors(fingerprint, occurred_at DESC)
    WHERE resolved_at IS NULL AND ignored = FALSE;
```

### Setup

```python
from fraiseql.monitoring import init_error_tracker

# Initialize in application startup
async def startup():
    db_pool = await create_pool(DATABASE_URL)

    tracker = init_error_tracker(
        db_pool,
        environment="production",
        auto_notify=True  # Automatic notifications
    )

    # Store in app state for use in middleware
    app.state.error_tracker = tracker
```

### Capture Errors

```python
# Automatic capture in middleware
@app.middleware("http")
async def error_tracking_middleware(request: Request, call_next):
    try:
        response = await call_next(request)
        return response
    except Exception as error:
        # Capture with context
        await app.state.error_tracker.capture_exception(
            error,
            context={
                "request_id": request.state.request_id,
                "user_id": getattr(request.state, "user_id", None),
                "path": request.url.path,
                "method": request.method,
                "headers": dict(request.headers)
            }
        )
        raise

# Manual capture in resolvers
@query
async def process_payment(info, order_id: str) -> PaymentResult:
    try:
        result = await charge_payment(order_id)
        return result
    except PaymentError as error:
        await info.context["error_tracker"].capture_exception(
            error,
            context={
                "order_id": order_id,
                "user_id": info.context["user_id"],
                "operation": "process_payment"
            }
        )
        raise
```

## Distributed Tracing

OpenTelemetry traces stored directly in PostgreSQL for correlation with errors and business events.

### Schema

```sql
-- Traces table
CREATE TABLE monitoring.traces (
    trace_id TEXT PRIMARY KEY,
    span_id TEXT NOT NULL,
    parent_span_id TEXT,
    operation_name TEXT NOT NULL,
    start_time TIMESTAMP WITH TIME ZONE NOT NULL,
    end_time TIMESTAMP WITH TIME ZONE NOT NULL,
    duration_ms INTEGER NOT NULL,
    status_code INTEGER,
    status_message TEXT,
    attributes JSONB,
    events JSONB,
    links JSONB,
    resource JSONB,
    environment TEXT NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

-- Indexes
CREATE INDEX idx_traces_start_time ON monitoring.traces(start_time DESC);
CREATE INDEX idx_traces_operation ON monitoring.traces(operation_name);
CREATE INDEX idx_traces_duration ON monitoring.traces(duration_ms DESC);
CREATE INDEX idx_traces_status ON monitoring.traces(status_code);
CREATE INDEX idx_traces_attributes ON monitoring.traces USING GIN(attributes);
CREATE INDEX idx_traces_parent ON monitoring.traces(parent_span_id) WHERE parent_span_id IS NOT NULL;
```

### Setup

```python
from opentelemetry import trace
from opentelemetry.sdk.trace import TracerProvider
from opentelemetry.sdk.trace.export import BatchSpanProcessor
from fraiseql.monitoring.exporters import PostgreSQLSpanExporter

# Configure OpenTelemetry to export to PostgreSQL
def setup_tracing(db_pool):
    # Create PostgreSQL exporter
    exporter = PostgreSQLSpanExporter(db_pool)

    # Configure tracer provider
    provider = TracerProvider()
    processor = BatchSpanProcessor(exporter)
    provider.add_span_processor(processor)

    # Set as global tracer provider
    trace.set_tracer_provider(provider)

    return trace.get_tracer(__name__)

tracer = setup_tracing(db_pool)
```

### Instrument Code

```python
from opentelemetry import trace

tracer = trace.get_tracer(__name__)

@query
async def get_user_orders(info, user_id: str) -> list[Order]:
    # Create span
    with tracer.start_as_current_span(
        "get_user_orders",
        attributes={
            "user.id": user_id,
            "operation.type": "query"
        }
    ) as span:
        # Database query
        with tracer.start_as_current_span("db.query") as db_span:
            db_span.set_attribute("db.statement", "SELECT * FROM v_order WHERE user_id = $1")
            db_span.set_attribute("db.system", "postgresql")

            orders = await info.context["repo"].find("v_order", where={"user_id": user_id})

            db_span.set_attribute("db.rows_returned", len(orders))

        # Add business context
        span.set_attribute("orders.count", len(orders))
        span.set_attribute("orders.total_value", sum(o.total for o in orders))

        return orders
```

### Automatic Instrumentation

```python
from opentelemetry.instrumentation.fastapi import FastAPIInstrumentor
from opentelemetry.instrumentation.asyncpg import AsyncPGInstrumentor

# Instrument FastAPI automatically
FastAPIInstrumentor.instrument_app(app)

# Instrument asyncpg (PostgreSQL driver)
AsyncPGInstrumentor().instrument()
```

## Metrics Collection

### PostgreSQL-Native Metrics

Store metrics directly in PostgreSQL for correlation with traces and errors:

```sql
CREATE TABLE monitoring.metrics (
    id SERIAL PRIMARY KEY,
    metric_name TEXT NOT NULL,
    metric_type TEXT NOT NULL, -- counter, gauge, histogram
    metric_value NUMERIC NOT NULL,
    labels JSONB,
    timestamp TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    environment TEXT NOT NULL
);

CREATE INDEX idx_metrics_name_time ON monitoring.metrics(metric_name, timestamp DESC);
CREATE INDEX idx_metrics_timestamp ON monitoring.metrics(timestamp DESC);
CREATE INDEX idx_metrics_labels ON monitoring.metrics USING GIN(labels);
```

### Record Metrics

```python
from fraiseql.monitoring import MetricsRecorder

metrics = MetricsRecorder(db_pool)

# Counter
await metrics.increment(
    "graphql.requests.total",
    labels={"operation": "getUser", "status": "success"}
)

# Gauge
await metrics.set_gauge(
    "db.pool.connections.active",
    value=pool.get_size() - pool.get_idle_size(),
    labels={"pool": "primary"}
)

# Histogram
await metrics.record_histogram(
    "graphql.request.duration_ms",
    value=duration_ms,
    labels={"operation": "getOrders"}
)
```

### Prometheus Integration (Optional)

Export PostgreSQL metrics to Prometheus:

```python
from prometheus_client import Counter, Histogram, Gauge, generate_latest

# Define metrics
graphql_requests = Counter(
    'graphql_requests_total',
    'Total GraphQL requests',
    ['operation', 'status']
)

graphql_duration = Histogram(
    'graphql_request_duration_seconds',
    'GraphQL request duration',
    ['operation']
)

# Expose metrics endpoint
@app.get("/metrics")
async def metrics_endpoint():
    return Response(
        content=generate_latest(),
        media_type="text/plain"
    )
```

## Correlation

The power of PostgreSQL-native observability is the ability to correlate everything with SQL.

### Error + Trace Correlation

```sql
-- Find traces for errors
SELECT
    e.fingerprint,
    e.message,
    e.occurred_at,
    t.operation_name,
    t.duration_ms,
    t.status_code,
    t.attributes
FROM monitoring.errors e
JOIN monitoring.traces t ON e.trace_id = t.trace_id
WHERE e.fingerprint = 'payment_processing_error'
ORDER BY e.occurred_at DESC
LIMIT 20;
```

### Error + Business Event Correlation

```sql
-- Find business context for errors
SELECT
    e.fingerprint,
    e.message,
    e.context->>'order_id' as order_id,
    c.entity_name,
    c.entity_id,
    c.change_type,
    c.before_data,
    c.after_data,
    c.changed_at
FROM monitoring.errors e
JOIN tb_entity_change_log c ON e.context->>'order_id' = c.entity_id::text
WHERE e.fingerprint = 'order_processing_error'
  AND c.entity_name = 'order'
ORDER BY e.occurred_at DESC;
```

### Trace + Metrics Correlation

```sql
-- Find slow requests with metrics
SELECT
    t.trace_id,
    t.operation_name,
    t.duration_ms,
    m.metric_value as db_query_count,
    t.attributes->>'user_id' as user_id
FROM monitoring.traces t
LEFT JOIN LATERAL (
    SELECT SUM(metric_value) as metric_value
    FROM monitoring.metrics
    WHERE metric_name = 'db.queries.count'
      AND timestamp BETWEEN t.start_time AND t.end_time
) m ON true
WHERE t.duration_ms > 1000  -- Slower than 1 second
ORDER BY t.duration_ms DESC
LIMIT 50;
```

### Full Correlation Query

```sql
-- Complete observability picture
SELECT
    e.fingerprint,
    e.message,
    e.occurred_at,
    t.operation_name,
    t.duration_ms,
    t.status_code,
    c.entity_name,
    c.change_type,
    e.context->>'user_id' as user_id,
    COUNT(*) OVER (PARTITION BY e.fingerprint) as error_count
FROM monitoring.errors e
LEFT JOIN monitoring.traces t ON e.trace_id = t.trace_id
LEFT JOIN tb_entity_change_log c
    ON t.trace_id = c.trace_id::text
    AND c.changed_at BETWEEN e.occurred_at - INTERVAL '1 second'
                         AND e.occurred_at + INTERVAL '1 second'
WHERE e.occurred_at > NOW() - INTERVAL '24 hours'
  AND e.resolved_at IS NULL
ORDER BY e.occurred_at DESC;
```

## Grafana Dashboards

Pre-built dashboards for PostgreSQL-native observability.

### Error Monitoring Dashboard

**Location**: `grafana/error_monitoring.json`

**Panels:**
- Error rate over time
- Top 10 error fingerprints
- Error distribution by environment
- Recent errors (table)
- Error resolution status

**Data Source**: PostgreSQL

**Example Query (Error Rate):**
```sql
SELECT
  date_trunc('minute', occurred_at) as time,
  COUNT(*) as error_count
FROM monitoring.errors
WHERE
  occurred_at >= $__timeFrom
  AND occurred_at <= $__timeTo
  AND environment = '$environment'
GROUP BY time
ORDER BY time;
```

### Trace Performance Dashboard

**Location**: `grafana/trace_performance.json`

**Panels:**
- Request rate (requests/sec)
- P50, P95, P99 latency
- Slowest operations
- Trace status distribution
- Database query duration

**Example Query (P95 Latency):**
```sql
SELECT
  date_trunc('minute', start_time) as time,
  percentile_cont(0.95) WITHIN GROUP (ORDER BY duration_ms) as p95_latency
FROM monitoring.traces
WHERE
  start_time >= $__timeFrom
  AND start_time <= $__timeTo
  AND environment = '$environment'
GROUP BY time
ORDER BY time;
```

### System Metrics Dashboard

**Location**: `grafana/system_metrics.json`

**Panels:**
- Database pool connections (active/idle)
- Cache hit rate
- GraphQL operation rate
- Memory usage
- Query execution time

### Installation

```bash
# Import dashboards to Grafana
cd grafana/
for dashboard in *.json; do
  curl -X POST http://admin:admin@localhost:3000/api/dashboards/db \
    -H "Content-Type: application/json" \
    -d @"$dashboard"
done
```

## Query Examples

### Error Analysis

```sql
-- Top errors in last 24 hours
SELECT
    fingerprint,
    exception_type,
    message,
    COUNT(*) as occurrences,
    MAX(occurred_at) as last_seen,
    MIN(occurred_at) as first_seen,
    COUNT(DISTINCT context->>'user_id') as affected_users
FROM monitoring.errors
WHERE occurred_at > NOW() - INTERVAL '24 hours'
  AND resolved_at IS NULL
GROUP BY fingerprint, exception_type, message
ORDER BY occurrences DESC
LIMIT 20;

-- Error trends (hourly)
SELECT
    date_trunc('hour', occurred_at) as hour,
    fingerprint,
    COUNT(*) as count
FROM monitoring.errors
WHERE occurred_at > NOW() - INTERVAL '7 days'
GROUP BY hour, fingerprint
ORDER BY hour DESC, count DESC;

-- Users affected by errors
SELECT
    context->>'user_id' as user_id,
    COUNT(DISTINCT fingerprint) as unique_errors,
    COUNT(*) as total_errors,
    array_agg(DISTINCT exception_type) as error_types
FROM monitoring.errors
WHERE occurred_at > NOW() - INTERVAL '24 hours'
  AND context->>'user_id' IS NOT NULL
GROUP BY context->>'user_id'
ORDER BY total_errors DESC
LIMIT 50;
```

### Performance Analysis

```sql
-- Slowest operations (P99)
SELECT
    operation_name,
    COUNT(*) as request_count,
    percentile_cont(0.50) WITHIN GROUP (ORDER BY duration_ms) as p50_ms,
    percentile_cont(0.95) WITHIN GROUP (ORDER BY duration_ms) as p95_ms,
    percentile_cont(0.99) WITHIN GROUP (ORDER BY duration_ms) as p99_ms,
    MAX(duration_ms) as max_ms
FROM monitoring.traces
WHERE start_time > NOW() - INTERVAL '1 hour'
GROUP BY operation_name
HAVING COUNT(*) > 10
ORDER BY p99_ms DESC
LIMIT 20;

-- Database query performance
SELECT
    attributes->>'db.statement' as query,
    COUNT(*) as execution_count,
    AVG(duration_ms) as avg_duration_ms,
    MAX(duration_ms) as max_duration_ms
FROM monitoring.traces
WHERE start_time > NOW() - INTERVAL '1 hour'
  AND attributes->>'db.system' = 'postgresql'
GROUP BY attributes->>'db.statement'
ORDER BY avg_duration_ms DESC
LIMIT 20;
```

### Correlation Analysis

```sql
-- Operations with highest error rate
SELECT
    t.operation_name,
    COUNT(DISTINCT t.trace_id) as total_requests,
    COUNT(DISTINCT e.id) as errors,
    ROUND(100.0 * COUNT(DISTINCT e.id) / COUNT(DISTINCT t.trace_id), 2) as error_rate_pct
FROM monitoring.traces t
LEFT JOIN monitoring.errors e ON t.trace_id = e.trace_id
WHERE t.start_time > NOW() - INTERVAL '1 hour'
GROUP BY t.operation_name
HAVING COUNT(DISTINCT t.trace_id) > 10
ORDER BY error_rate_pct DESC;

-- Trace timeline with events
SELECT
    t.trace_id,
    t.operation_name,
    t.start_time,
    t.duration_ms,
    e.exception_type,
    e.message,
    c.entity_name,
    c.change_type
FROM monitoring.traces t
LEFT JOIN monitoring.errors e ON t.trace_id = e.trace_id
LEFT JOIN tb_entity_change_log c ON t.trace_id = c.trace_id::text
WHERE t.trace_id = 'your-trace-id-here'
ORDER BY t.start_time;
```

## Performance Tuning

### Table Partitioning

Partition large tables for better query performance:

```sql
-- Partition errors by month
CREATE TABLE monitoring.errors_partitioned (
    LIKE monitoring.errors INCLUDING ALL
) PARTITION BY RANGE (occurred_at);

-- Create monthly partitions
CREATE TABLE monitoring.errors_2025_01
    PARTITION OF monitoring.errors_partitioned
    FOR VALUES FROM ('2025-01-01') TO ('2025-02-01');

CREATE TABLE monitoring.errors_2025_02
    PARTITION OF monitoring.errors_partitioned
    FOR VALUES FROM ('2025-02-01') TO ('2025-03-01');

-- Auto-create partitions with pg_partman
```

### Data Retention

Automatically clean up old data:

```sql
-- Delete old errors (90 days)
DELETE FROM monitoring.errors
WHERE occurred_at < NOW() - INTERVAL '90 days';

-- Delete old traces (30 days)
DELETE FROM monitoring.traces
WHERE start_time < NOW() - INTERVAL '30 days';

-- Delete old metrics (7 days)
DELETE FROM monitoring.metrics
WHERE timestamp < NOW() - INTERVAL '7 days';
```

### Scheduled Cleanup

```python
from apscheduler.schedulers.asyncio import AsyncIOScheduler

scheduler = AsyncIOScheduler()

@scheduler.scheduled_job('cron', hour=2, minute=0)
async def cleanup_old_observability_data():
    """Run daily at 2 AM."""
    async with db_pool.acquire() as conn:
        # Clean errors
        await conn.execute("""
            DELETE FROM monitoring.errors
            WHERE occurred_at < NOW() - INTERVAL '90 days'
        """)

        # Clean traces
        await conn.execute("""
            DELETE FROM monitoring.traces
            WHERE start_time < NOW() - INTERVAL '30 days'
        """)

        # Clean metrics
        await conn.execute("""
            DELETE FROM monitoring.metrics
            WHERE timestamp < NOW() - INTERVAL '7 days'
        """)

scheduler.start()
```

### Indexes Optimization

```sql
-- Add indexes for common queries
CREATE INDEX idx_errors_user_time ON monitoring.errors((context->>'user_id'), occurred_at DESC);
CREATE INDEX idx_traces_slow ON monitoring.traces(duration_ms DESC) WHERE duration_ms > 1000;
CREATE INDEX idx_errors_recent_unresolved ON monitoring.errors(occurred_at DESC)
    WHERE resolved_at IS NULL AND occurred_at > NOW() - INTERVAL '7 days';
```

## Best Practices

### 1. Context Enrichment

Always include rich context in errors and traces:

```python
await tracker.capture_exception(
    error,
    context={
        "user_id": user.id,
        "tenant_id": tenant.id,
        "request_id": request_id,
        "operation": operation_name,
        "input_size": len(input_data),
        "database_pool_size": pool.get_size(),
        "memory_usage_mb": get_memory_usage(),
        # Business context
        "order_id": order_id,
        "payment_amount": amount,
        "payment_method": method
    }
)
```

### 2. Trace Sampling

Sample traces in high-traffic environments:

```python
from opentelemetry.sdk.trace.sampling import TraceIdRatioBased

# Sample 10% of traces
sampler = TraceIdRatioBased(0.1)

provider = TracerProvider(sampler=sampler)
```

### 3. Error Notification Rules

Configure smart notifications:

```python
# Only notify on new fingerprints
tracker.set_notification_rule(
    "new_errors_only",
    notify_on_new_fingerprint=True
)

# Rate limit notifications
tracker.set_notification_rule(
    "rate_limited",
    notify_on_occurrence=[1, 10, 100, 1000]  # 1st, 10th, 100th, 1000th
)

# Critical errors only
tracker.set_notification_rule(
    "critical_only",
    notify_when=lambda error: "critical" in error.context.get("severity", "")
)
```

### 4. Dashboard Organization

Organize dashboards by audience:

- **DevOps Dashboard**: Infrastructure metrics, database health, error rates
- **Developer Dashboard**: Slow queries, error details, trace details
- **Business Dashboard**: User impact, feature usage, business metrics
- **Executive Dashboard**: High-level KPIs, uptime, cost metrics

### 5. Alert Fatigue Prevention

Avoid alert fatigue with smart grouping:

```sql
-- Group similar errors for single alert
SELECT
    fingerprint,
    COUNT(*) as occurrences,
    array_agg(DISTINCT context->>'user_id') as affected_users
FROM monitoring.errors
WHERE occurred_at > NOW() - INTERVAL '5 minutes'
  AND resolved_at IS NULL
GROUP BY fingerprint
HAVING COUNT(*) > 10  -- Only alert if >10 occurrences
ORDER BY occurrences DESC;
```

## Comparison to External APM

| Feature | PostgreSQL Observability | SaaS APM (Datadog, New Relic) |
|---------|-------------------------|-------------------------------|
| Cost | $0 (included) | $500-5,000/month |
| Error Tracking | ✅ Built-in | ✅ Built-in |
| Distributed Tracing | ✅ OpenTelemetry | ✅ Proprietary + OTel |
| Metrics | ✅ PostgreSQL or Prometheus | ✅ Built-in |
| Dashboards | ✅ Grafana | ✅ Built-in |
| Correlation | ✅ SQL joins | ⚠️ Limited |
| Business Context | ✅ Join with app tables | ❌ Separate |
| Data Location | ✅ Self-hosted | ❌ SaaS only |
| Query Flexibility | ✅ Full SQL | ⚠️ Limited query language |
| Retention | ✅ Configurable (unlimited) | ⚠️ Limited by plan |
| Setup Complexity | ⚠️ Manual setup | ✅ Quick start |
| Learning Curve | ⚠️ SQL knowledge required | ✅ GUI-driven |

## Next Steps

- [Monitoring Guide](monitoring.md) - Detailed monitoring setup
- [Deployment](deployment.md) - Production deployment patterns
- [Security](security.md) - Security best practices
- [Health Checks](health-checks.md) - Application health monitoring
