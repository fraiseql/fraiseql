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
  - [Schema](#schema)
  - [Setup](#setup)
  - [Capture Errors](#capture-errors)
  - [Error Notifications](#error-notifications)
- [Distributed Tracing](#distributed-tracing)
- [Metrics Collection](#metrics-collection)
- [Correlation](#correlation)
- [Grafana Dashboards](#grafana-dashboards)
- [Query Examples](#query-examples)
- [Performance Tuning](#performance-tuning)
  - [Production-Scale Error Storage](#production-scale-error-storage)
  - [Data Retention](#data-retention)
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
import fraiseql

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
import fraiseql

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
@fraiseql.query
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

### Error Notifications

Configure automatic notifications when errors occur using Email, Slack, or custom webhooks.

#### Overview

FraiseQL includes a production-ready notification system that sends alerts when errors are captured. The system supports:

- **Multiple Channels**: Email (SMTP), Slack (webhooks), generic webhooks
- **Smart Rate Limiting**: Per-error-type, configurable thresholds
- **Delivery Tracking**: Full audit log of notification attempts
- **Template-Based Messages**: Customizable notification formats
- **Async Delivery**: Non-blocking notification sending

**Comparison to External Services:**

| Feature | FraiseQL Notifications | PagerDuty/Opsgenie |
|---------|----------------------|-------------------|
| Email Alerts | ✅ Built-in (SMTP) | ✅ Built-in |
| Slack Integration | ✅ Webhook-based | ✅ Built-in |
| Rate Limiting | ✅ Per-error, configurable | ⚠️ Plan-dependent |
| Custom Webhooks | ✅ Full HTTP customization | ⚠️ Limited |
| Delivery Tracking | ✅ PostgreSQL audit log | ✅ Built-in |
| Cost | $0 (included) | $19-99/user/month |
| Setup | ⚠️ Manual config | ✅ Quick start |

#### Email Notifications

Send error alerts via SMTP with HTML-formatted messages.

**Setup:**

```python
from fraiseql.monitoring.notifications import EmailChannel, NotificationManager

# Configure email channel
email_channel = EmailChannel(
    smtp_host="smtp.gmail.com",
    smtp_port=587,
    smtp_user="alerts@myapp.com",
    smtp_password="app_password",
    use_tls=True,
    from_address="noreply@myapp.com"
)

# Create notification manager
notification_manager = NotificationManager(db_pool)
notification_manager.register_channel("email", lambda **kwargs: email_channel)
```

**Configuration in Database:**

```sql
-- Create notification rule
INSERT INTO tb_error_notification_config (
    config_id,
    error_type,              -- Filter by error type (NULL = all)
    severity,                -- Filter by severity (array)
    environment,             -- Filter by environment (array)
    channel_type,            -- 'email', 'slack', 'webhook'
    channel_config,          -- Channel-specific JSON config
    rate_limit_minutes,      -- Minutes between notifications (0 = no limit)
    min_occurrence_count,    -- Only notify after N occurrences
    enabled
) VALUES (
    gen_random_uuid(),
    'ValueError',                                    -- Only ValueError errors
    ARRAY['error', 'critical'],                     -- Critical/error severity
    ARRAY['production'],                            -- Production only
    'email',
    jsonb_build_object(
        'to', ARRAY['team@myapp.com', 'oncall@myapp.com'],
        'subject', 'Production Error: {error_type}'
    ),
    60,                                             -- Max 1 notification per hour
    1,                                              -- Notify on first occurrence
    true
);
```

**Email Format:**

- **Plain Text**: Simple formatted message
- **HTML**: Rich formatting with severity colors, stack traces, error details
- **Template Variables**: `{error_type}`, `{environment}`, `{error_message}`, etc.

#### Slack Notifications

Send formatted error alerts to Slack channels using incoming webhooks.

**Setup:**

```python
from fraiseql.monitoring.notifications import SlackChannel

# Slack channel auto-registers with NotificationManager
# No explicit setup needed - configure via database
```

**Slack Webhook Configuration:**

1. **Create Incoming Webhook** in Slack:
   - Go to https://api.slack.com/apps
   - Create app → Incoming Webhooks
   - Add webhook to workspace
   - Copy webhook URL

2. **Configure in Database:**

```sql
INSERT INTO tb_error_notification_config (
    config_id,
    error_fingerprint,       -- Specific error (NULL = all matching type/severity)
    severity,
    environment,
    channel_type,
    channel_config,
    rate_limit_minutes,
    enabled
) VALUES (
    gen_random_uuid(),
    NULL,                    -- All errors matching filters
    ARRAY['critical'],       -- Critical only
    ARRAY['production', 'staging'],
    'slack',
    jsonb_build_object(
        'webhook_url', 'https://hooks.slack.com/services/YOUR/WEBHOOK/URL',
        'channel', '#alerts',
        'username', 'FraiseQL Error Bot'
    ),
    30,                      -- Max 1 notification per 30 minutes
    true
);
```

**Slack Message Format:**

FraiseQL sends rich Slack Block Kit messages with:
- **Header**: Error type with severity emoji (🔴 🟡 🔵)
- **Details**: Environment, occurrence count, timestamps
- **Stack Trace**: Code-formatted preview (500 chars)
- **Footer**: Error ID and fingerprint for debugging

#### Custom Webhooks

Send error data to any HTTP endpoint for custom integrations.

**Setup:**

```sql
INSERT INTO tb_error_notification_config (
    config_id,
    error_type,
    channel_type,
    channel_config,
    rate_limit_minutes,
    enabled
) VALUES (
    gen_random_uuid(),
    'PaymentError',
    'webhook',
    jsonb_build_object(
        'url', 'https://api.myapp.com/webhooks/errors',
        'method', 'POST',                           -- POST, PUT, PATCH
        'headers', jsonb_build_object(
            'Authorization', 'Bearer secret_token',
            'X-Custom-Header', 'value'
        )
    ),
    0,                       -- No rate limiting
    true
);
```

**Webhook Payload:**

```json
{
  "error_id": "123e4567-...",
  "error_fingerprint": "payment_timeout_abc123",
  "error_type": "PaymentError",
  "error_message": "Payment gateway timeout",
  "severity": "error",
  "occurrence_count": 5,
  "first_seen": "2025-10-11T10:00:00Z",
  "last_seen": "2025-10-11T12:30:00Z",
  "environment": "production",
  "release_version": "v1.2.3",
  "stack_trace": "Traceback (most recent call last):\n  ..."
}
```

#### Rate Limiting Strategies

**Strategy 1: First Occurrence Only**

```sql
-- Notify only when error first occurs
rate_limit_minutes = 0,
min_occurrence_count = 1
```

**Strategy 2: Threshold-Based**

```sql
-- Notify after 10 occurrences, then hourly
rate_limit_minutes = 60,
min_occurrence_count = 10
```

**Strategy 3: Multiple Thresholds** (via multiple configs)

```sql
-- Config 1: Notify immediately on first occurrence
INSERT INTO tb_error_notification_config (
    error_fingerprint, min_occurrence_count, rate_limit_minutes, channel_config
) VALUES (
    'critical_bug_fingerprint', 1, 0, '{"webhook_url": "..."}'
);

-- Config 2: Notify again at 10th occurrence
INSERT INTO tb_error_notification_config (
    error_fingerprint, min_occurrence_count, rate_limit_minutes, channel_config
) VALUES (
    'critical_bug_fingerprint', 10, 0, '{"webhook_url": "..."}'
);

-- Config 3: Notify hourly after 100 occurrences
INSERT INTO tb_error_notification_config (
    error_fingerprint, min_occurrence_count, rate_limit_minutes, channel_config
) VALUES (
    'critical_bug_fingerprint', 100, 60, '{"webhook_url": "..."}'
);
```

**Strategy 4: Environment-Specific**

```sql
-- Production: Immediate alerts
INSERT INTO tb_error_notification_config (
    environment, rate_limit_minutes, channel_type
) VALUES (
    ARRAY['production'], 0, 'slack'
);

-- Staging: Daily digest
INSERT INTO tb_error_notification_config (
    environment, rate_limit_minutes, channel_type
) VALUES (
    ARRAY['staging'], 1440, 'email'  -- 24 hours
);
```

#### Notification Delivery Tracking

All notification attempts are logged for auditing and troubleshooting.

**Query Delivery Status:**

```sql
-- Recent notification deliveries
SELECT
    n.sent_at,
    n.channel_type,
    n.recipient,
    n.status,              -- 'sent', 'failed'
    n.error_message,       -- NULL if successful
    e.error_type,
    e.error_message
FROM tb_error_notification_log n
JOIN tb_error_log e ON n.error_id = e.error_id
ORDER BY n.sent_at DESC
LIMIT 50;

-- Failed notifications (troubleshooting)
SELECT
    n.sent_at,
    n.channel_type,
    n.error_message as delivery_error,
    e.error_type,
    COUNT(*) OVER (PARTITION BY n.channel_type) as failures_by_channel
FROM tb_error_notification_log n
JOIN tb_error_log e ON n.error_id = e.error_id
WHERE n.status = 'failed'
  AND n.sent_at > NOW() - INTERVAL '24 hours'
ORDER BY n.sent_at DESC;

-- Notification volume by channel
SELECT
    channel_type,
    COUNT(*) as total_sent,
    COUNT(*) FILTER (WHERE status = 'sent') as successful,
    COUNT(*) FILTER (WHERE status = 'failed') as failed,
    ROUND(100.0 * COUNT(*) FILTER (WHERE status = 'sent') / COUNT(*), 2) as success_rate
FROM tb_error_notification_log
WHERE sent_at > NOW() - INTERVAL '7 days'
GROUP BY channel_type;
```

#### Custom Notification Channels

Extend the notification system with custom channels.

**Example: SMS Notifications via Twilio**

```python
from fraiseql.monitoring.notifications import NotificationManager
import httpx

class TwilioSMSChannel:
    """SMS notification channel using Twilio."""

    def __init__(self, account_sid: str, auth_token: str, from_number: str):
        self.account_sid = account_sid
        self.auth_token = auth_token
        self.from_number = from_number

    async def send(self, error: dict, config: dict) -> tuple[bool, str | None]:
        """Send SMS notification."""
        try:
            to_number = config.get("to")
            if not to_number:
                return False, "No recipient phone number"

            message = self.format_message(error)

            async with httpx.AsyncClient() as client:
                response = await client.post(
                    f"https://api.twilio.com/2010-04-01/Accounts/{self.account_sid}/Messages.json",
                    auth=(self.account_sid, self.auth_token),
                    data={
                        "From": self.from_number,
                        "To": to_number,
                        "Body": message
                    }
                )

                if response.status_code == 201:
                    return True, None
                return False, f"Twilio API returned {response.status_code}"

        except Exception as e:
            return False, str(e)

    def format_message(self, error: dict, template: str | None = None) -> str:
        """Format error for SMS (160 char limit)."""
        return (
            f"🚨 {error['error_type']}: {error['error_message'][:80]}\n"
            f"Env: {error['environment']} | Count: {error['occurrence_count']}"
        )

# Register custom channel
notification_manager = NotificationManager(db_pool)
notification_manager.register_channel(
    "twilio_sms",
    lambda **config: TwilioSMSChannel(
        account_sid=config["account_sid"],
        auth_token=config["auth_token"],
        from_number=config["from_number"]
    )
)
```

**Usage in Database:**

```sql
INSERT INTO tb_error_notification_config (
    config_id,
    severity,
    channel_type,
    channel_config,
    enabled
) VALUES (
    gen_random_uuid(),
    ARRAY['critical'],
    'twilio_sms',                -- Custom channel type
    jsonb_build_object(
        'to', '+1234567890',
        'account_sid', 'AC...',
        'auth_token', 'your_token',
        'from_number', '+0987654321'
    ),
    true
);
```

#### Troubleshooting

**Issue: Notifications not sending**

1. **Check configuration:**
   ```sql
   SELECT * FROM tb_error_notification_config WHERE enabled = true;
   ```

2. **Verify error matches filters:**
   ```sql
   SELECT
       e.error_type,
       e.severity,
       e.environment,
       c.error_type as config_error_type,
       c.severity as config_severity,
       c.environment as config_environment
   FROM tb_error_log e
   CROSS JOIN tb_error_notification_config c
   WHERE e.error_id = 'your-error-id'
     AND c.enabled = true;
   ```

3. **Check rate limiting:**
   ```sql
   SELECT * FROM tb_error_notification_log
   WHERE error_id = 'your-error-id'
   ORDER BY sent_at DESC;
   ```

4. **Review delivery errors:**
   ```sql
   SELECT error_message, COUNT(*) as count
   FROM tb_error_notification_log
   WHERE status = 'failed'
     AND sent_at > NOW() - INTERVAL '24 hours'
   GROUP BY error_message
   ORDER BY count DESC;
   ```

**Issue: Email delivery fails**

- Verify SMTP credentials and host
- Check firewall allows outbound port 587/465
- Test SMTP connection manually:
  ```python
  import smtplib
  server = smtplib.SMTP("smtp.gmail.com", 587)
  server.starttls()
  server.login("user", "password")
  ```

**Issue: Slack webhook fails**

- Verify webhook URL is correct
- Check webhook hasn't been revoked in Slack
- Test webhook manually:
  ```bash
  curl -X POST https://hooks.slack.com/services/YOUR/WEBHOOK/URL \
    -H 'Content-Type: application/json' \
    -d '{"text": "Test message"}'
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
import fraiseql

from opentelemetry import trace

tracer = trace.get_tracer(__name__)

@fraiseql.query
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

            orders = await info.context["db"].find("v_order", where={"user_id": user_id})

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

### Production-Scale Error Storage

FraiseQL implements automatic table partitioning for production-scale error storage, handling millions of error occurrences efficiently.

#### Overview

**Challenge**: Error occurrence tables grow rapidly in production (1M+ rows per month in high-traffic apps). Sequential scans become slow, retention policies are complex, and disk space grows unbounded.

**Solution**: Monthly partitioning with automatic partition management.

**Benefits:**
- **Query Performance**: 10-50x faster queries via partition pruning
- **Storage Efficiency**: Drop old partitions instantly vs slow DELETE operations
- **Maintenance**: Auto-create future partitions, auto-drop old partitions
- **Retention**: 6-month default retention (configurable)

#### Architecture

```sql
-- Partitioned error occurrence table (automatically created by schema.sql)
CREATE TABLE tb_error_occurrence (
    occurrence_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    error_id UUID NOT NULL REFERENCES tb_error_log(error_id),
    occurred_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    stack_trace TEXT,
    context JSONB,
    trace_id TEXT,
    resolved BOOLEAN DEFAULT FALSE,
    created_at TIMESTAMPTZ DEFAULT NOW()
) PARTITION BY RANGE (occurred_at);

-- Monthly partitions are automatically created:
-- - tb_error_occurrence_2025_10 (Oct 2025)
-- - tb_error_occurrence_2025_11 (Nov 2025)
-- - tb_error_occurrence_2025_12 (Dec 2025)
-- ... etc.
```

**Partition Naming**: `tb_error_occurrence_YYYY_MM`

**Partition Range**: Each partition contains one calendar month of data.

#### Automatic Partition Management

FraiseQL includes PostgreSQL functions for managing partitions automatically.

**1. Create Partition for Specific Month**

```sql
-- Create partition for a specific date's month
SELECT create_error_occurrence_partition('2025-12-15'::date);
-- Returns: 'tb_error_occurrence_2025_12'

-- Idempotent: safe to call multiple times
SELECT create_error_occurrence_partition('2025-12-01'::date);
-- Returns existing partition if already exists
```

**Function Definition** (included in `schema.sql`):

```sql
CREATE OR REPLACE FUNCTION create_error_occurrence_partition(target_date DATE)
RETURNS TEXT AS $$
DECLARE
    partition_name TEXT;
    start_date DATE;
    end_date DATE;
BEGIN
    -- Calculate partition boundaries
    start_date := date_trunc('month', target_date)::date;
    end_date := (start_date + INTERVAL '1 month')::date;
    partition_name := 'tb_error_occurrence_' || to_char(start_date, 'YYYY_MM');

    -- Create partition if not exists
    IF NOT EXISTS (
        SELECT 1 FROM pg_class WHERE relname = partition_name
    ) THEN
        EXECUTE format(
            'CREATE TABLE %I PARTITION OF tb_error_occurrence
             FOR VALUES FROM (%L) TO (%L)',
            partition_name, start_date, end_date
        );
    END IF;

    RETURN partition_name;
END;
$$ LANGUAGE plpgsql;
```

**2. Ensure Future Partitions Exist**

```sql
-- Ensure next 3 months have partitions
SELECT * FROM ensure_error_occurrence_partitions(3);

-- Returns:
-- partition_name               | created
-- -----------------------------+---------
-- tb_error_occurrence_2025_11  | true
-- tb_error_occurrence_2025_12  | true
-- tb_error_occurrence_2026_01  | true
```

**Function Definition**:

```sql
CREATE OR REPLACE FUNCTION ensure_error_occurrence_partitions(months_ahead INT)
RETURNS TABLE(partition_name TEXT, created BOOLEAN) AS $$
DECLARE
    target_date DATE;
    result_name TEXT;
    was_created BOOLEAN;
BEGIN
    FOR i IN 0..months_ahead LOOP
        target_date := (CURRENT_DATE + (i || ' months')::INTERVAL)::DATE;

        -- Check if partition exists
        SELECT relname INTO result_name
        FROM pg_class
        WHERE relname = 'tb_error_occurrence_' || to_char(target_date, 'YYYY_MM');

        was_created := (result_name IS NULL);

        -- Create if missing
        IF was_created THEN
            result_name := create_error_occurrence_partition(target_date);
        END IF;

        partition_name := result_name;
        created := was_created;
        RETURN NEXT;
    END LOOP;
END;
$$ LANGUAGE plpgsql;
```

**Recommended Cron Job**:

```bash
# Ensure partitions exist for next 3 months (run monthly)
0 0 1 * * psql -d myapp -c "SELECT ensure_error_occurrence_partitions(3);"
```

**3. Drop Old Partitions (Retention Policy)**

```sql
-- Drop partitions older than 6 months
SELECT * FROM drop_old_error_occurrence_partitions(6);

-- Returns:
-- partition_name               | dropped
-- -----------------------------+---------
-- tb_error_occurrence_2025_04  | true
-- tb_error_occurrence_2025_03  | true
```

**Function Definition**:

```sql
CREATE OR REPLACE FUNCTION drop_old_error_occurrence_partitions(retention_months INT)
RETURNS TABLE(partition_name TEXT, dropped BOOLEAN) AS $$
DECLARE
    cutoff_date DATE;
    part_record RECORD;
BEGIN
    cutoff_date := (CURRENT_DATE - (retention_months || ' months')::INTERVAL)::DATE;

    -- Find partitions older than cutoff
    FOR part_record IN
        SELECT
            c.relname,
            pg_get_expr(c.relpartbound, c.oid) as partition_bound
        FROM pg_class c
        JOIN pg_inherits i ON c.oid = i.inhrelid
        JOIN pg_class p ON i.inhparent = p.oid
        WHERE p.relname = 'tb_error_occurrence'
          AND c.relname LIKE 'tb_error_occurrence_%'
    LOOP
        -- Extract date from partition name (tb_error_occurrence_2025_04 -> 2025-04-01)
        DECLARE
            part_date DATE;
        BEGIN
            part_date := to_date(
                regexp_replace(part_record.relname, 'tb_error_occurrence_', ''),
                'YYYY_MM'
            );

            IF part_date < cutoff_date THEN
                EXECUTE format('DROP TABLE IF EXISTS %I', part_record.relname);
                partition_name := part_record.relname;
                dropped := true;
                RETURN NEXT;
            END IF;
        END;
    END LOOP;
END;
$$ LANGUAGE plpgsql;
```

**Recommended Cron Job**:

```bash
# Drop partitions older than 6 months (run monthly)
0 0 1 * * psql -d myapp -c "SELECT drop_old_error_occurrence_partitions(6);"
```

**4. Partition Statistics**

```sql
-- Get partition storage statistics
SELECT * FROM get_partition_stats();

-- Returns:
-- table_name            | partition_name               | row_count | total_size | index_size
-- ----------------------|------------------------------|-----------|------------|------------
-- tb_error_occurrence   | tb_error_occurrence_2025_10  | 1234567   | 450 MB     | 120 MB
-- tb_error_occurrence   | tb_error_occurrence_2025_11  | 987654    | 380 MB     | 95 MB
-- tb_error_occurrence   | tb_error_occurrence_2025_12  | 45678     | 18 MB      | 5 MB
```

**Function Definition**:

```sql
CREATE OR REPLACE FUNCTION get_partition_stats()
RETURNS TABLE(
    table_name TEXT,
    partition_name TEXT,
    row_count BIGINT,
    total_size TEXT,
    index_size TEXT
) AS $$
BEGIN
    RETURN QUERY
    SELECT
        'tb_error_occurrence'::TEXT,
        c.relname::TEXT,
        c.reltuples::BIGINT,
        pg_size_pretty(pg_total_relation_size(c.oid)),
        pg_size_pretty(pg_indexes_size(c.oid))
    FROM pg_class c
    JOIN pg_inherits i ON c.oid = i.inhrelid
    JOIN pg_class p ON i.inhparent = p.oid
    WHERE p.relname = 'tb_error_occurrence'
    ORDER BY c.relname;
END;
$$ LANGUAGE plpgsql;
```

#### Query Performance

**Partition Pruning** automatically eliminates irrelevant partitions from queries.

**Example: Query Last 7 Days**

```sql
-- Query automatically scans only current month's partition
EXPLAIN (ANALYZE, BUFFERS)
SELECT *
FROM tb_error_occurrence
WHERE occurred_at > NOW() - INTERVAL '7 days';

-- Query Plan:
-- Seq Scan on tb_error_occurrence_2025_10
--   Filter: (occurred_at > (now() - '7 days'::interval))
--   Buffers: shared hit=145
--   -> Only 1 partition scanned (not all 12+)
```

**Performance Comparison**:

| Operation | Non-Partitioned (10M rows) | Partitioned (10M rows) | Speedup |
|-----------|---------------------------|------------------------|---------|
| Query last 7 days | 2,500ms (full scan) | 50ms (1 partition) | 50x |
| Query specific month | 2,500ms (full scan) | 40ms (1 partition) | 62x |
| Count all rows | 1,800ms | 200ms (parallel scan) | 9x |
| Delete old data | 45,000ms (DELETE) | 15ms (DROP partition) | 3000x |

#### Partitioning Notification Log

The notification log is also partitioned for efficient querying and retention.

```sql
-- Partitioned notification log (automatically created by schema.sql)
CREATE TABLE tb_error_notification_log (
    notification_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    config_id UUID NOT NULL,
    error_id UUID NOT NULL,
    sent_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    channel_type TEXT NOT NULL,
    recipient TEXT,
    status TEXT NOT NULL,  -- 'sent', 'failed'
    error_message TEXT
) PARTITION BY RANGE (sent_at);

-- Monthly partitions automatically created:
-- tb_error_notification_log_2025_10
-- tb_error_notification_log_2025_11
-- ... etc.
```

Same partition management functions work for notification log (separate table name parameter).

#### Retention Policies

**Default Retention**: 6 months for both error occurrences and notification logs.

**Customize Retention**:

```sql
-- Keep errors for 12 months instead of 6
SELECT drop_old_error_occurrence_partitions(12);

-- Keep notification logs for 3 months
SELECT drop_old_notification_log_partitions(3);
```

**Storage Planning**:

| Traffic Level | Errors/Month | Storage/Month | 6-Month Total |
|--------------|--------------|---------------|---------------|
| Low (1K req/day) | ~10K errors | 15 MB | 90 MB |
| Medium (100K req/day) | ~100K errors | 150 MB | 900 MB |
| High (10M req/day) | ~1M errors | 1.5 GB | 9 GB |
| Very High (100M req/day) | ~10M errors | 15 GB | 90 GB |

**Cost Savings**: Dropping partitions is instant (15ms) vs DELETE operations (minutes to hours for large tables).

#### Monitoring Partition Health

**Check Partition Coverage**:

```sql
-- Verify partitions exist for next 3 months
SELECT
    generate_series(
        date_trunc('month', CURRENT_DATE),
        date_trunc('month', CURRENT_DATE + INTERVAL '3 months'),
        INTERVAL '1 month'
    )::DATE as required_month,
    EXISTS (
        SELECT 1 FROM pg_class
        WHERE relname = 'tb_error_occurrence_' ||
              to_char(generate_series, 'YYYY_MM')
    ) as partition_exists;

-- Required month | partition_exists
-- ---------------|-----------------
-- 2025-10-01     | true
-- 2025-11-01     | true
-- 2025-12-01     | true
-- 2026-01-01     | false  <- Missing! Run ensure_error_occurrence_partitions()
```

**Alert on Missing Partitions**:

```sql
-- Alert if current month or next month partition missing
SELECT
    'ALERT: Missing partition for ' ||
    to_char(check_month, 'YYYY-MM') as alert_message
FROM generate_series(
    date_trunc('month', CURRENT_DATE),
    date_trunc('month', CURRENT_DATE + INTERVAL '1 month'),
    INTERVAL '1 month'
) as check_month
WHERE NOT EXISTS (
    SELECT 1 FROM pg_class
    WHERE relname = 'tb_error_occurrence_' || to_char(check_month, 'YYYY_MM')
);
```

#### Backup & Restore

**Backup Specific Partitions**:

```bash
# Backup only recent partitions (last 3 months)
pg_dump -d myapp \
  -t tb_error_occurrence_2025_10 \
  -t tb_error_occurrence_2025_11 \
  -t tb_error_occurrence_2025_12 \
  > errors_recent.sql

# Backup all partitions
pg_dump -d myapp -t 'tb_error_occurrence*' > errors_all.sql
```

**Archive Old Partitions**:

```bash
# Export old partition before dropping
pg_dump -d myapp -t tb_error_occurrence_2025_04 > archive_2025_04.sql

# Drop partition
psql -d myapp -c "DROP TABLE tb_error_occurrence_2025_04;"
```

#### Troubleshooting

**Issue: Writes failing with "no partition found"**

```sql
-- Check if partition exists for current month
SELECT EXISTS (
    SELECT 1 FROM pg_class
    WHERE relname = 'tb_error_occurrence_' || to_char(CURRENT_DATE, 'YYYY_MM')
);

-- If false, create immediately:
SELECT create_error_occurrence_partition(CURRENT_DATE);
```

**Issue: Queries scanning all partitions**

```sql
-- Ensure WHERE clause includes partitioning key (occurred_at)
-- ✅ GOOD (partition pruning works):
SELECT * FROM tb_error_occurrence
WHERE occurred_at > '2025-10-01' AND error_id = '...';

-- ❌ BAD (scans all partitions):
SELECT * FROM tb_error_occurrence
WHERE error_id = '...';  -- Missing occurred_at filter!
```

**Issue: Old partitions not dropping**

```sql
-- Manually drop specific partition
DROP TABLE IF EXISTS tb_error_occurrence_2024_01;

-- Verify no foreign key constraints blocking drop
SELECT
    conname as constraint_name,
    conrelid::regclass as table_name
FROM pg_constraint
WHERE confrelid = 'tb_error_occurrence'::regclass;
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
