# FraiseQL Observability Model: Comprehensive Monitoring, Tracing, and Alerting

**Date:** January 2026
**Status:** Complete System Specification
**Audience:** Operations engineers, SRE teams, platform architects, application developers

---

## Executive Summary

FraiseQL provides a **comprehensive observability model** covering three pillars:

1. **Metrics** — Quantitative measurements (queries/second, latency, errors)
2. **Logs** — Structured event records (per-request, debug, errors)
3. **Traces** — Distributed request flow (end-to-end execution path)

All observability data includes **rich context** (user ID, query plan, authorization rules, database engine, cache status) enabling root-cause analysis without excessive debugging.

**Core principle**: Observable by default. Every operation produces telemetry; zero configuration needed for basic observability.

---

## 1. Metrics Framework

### 1.1 Metric Categories

FraiseQL produces metrics across five dimensions:

```text
┌─────────────────────────┐
│ Operation Metrics       │ Queries/second, mutations/second, subscriptions active
├─────────────────────────┤
│ Latency Metrics         │ Query duration, mutation duration, database query time
├─────────────────────────┤
│ Error Metrics           │ Error rate by code, error category distribution
├─────────────────────────┤
│ Resource Metrics        │ Database connections, query memory, cache size
├─────────────────────────┤
│ Business Metrics        │ Custom defined by application (user signups, revenue)
└─────────────────────────┘
```text

### 1.2 Core Metrics (Always Available)

**1.2.1 Query Metrics**

```text
fraiseql_query_requests_total
  {
    operation_name="GetUserPosts",
    status="success",     # success, error, timeout
    error_code="",        # Empty if success
    database="postgresql",
    cache_hit=true
  }
  5000  # 5000 queries in this time window

fraiseql_query_duration_seconds
  {
    operation_name="GetUserPosts",
    quantile="p50"  # p50, p95, p99, p99.9
  }
  0.045  # 45ms

fraiseql_query_duration_seconds
  {
    operation_name="GetUserPosts",
    quantile="p99"
  }
  0.250  # 250ms

fraiseql_query_database_time_seconds
  {
    operation_name="GetUserPosts"
  }
  0.035  # Database query took 35ms out of 45ms total

fraiseql_query_rows_returned
  {
    operation_name="GetUserPosts"
  }
  1500  # Average rows per query
```text

**1.2.2 Mutation Metrics**

```text
fraiseql_mutation_requests_total
  {
    operation_name="CreatePost",
    status="success",
    database="postgresql"
  }
  250  # 250 mutations this window

fraiseql_mutation_duration_seconds
  {
    operation_name="CreatePost",
    quantile="p95"
  }
  0.150  # 150ms

fraiseql_mutation_rows_affected
  {
    operation_name="CreatePost"
  }
  1  # Rows modified

fraiseql_mutation_deadlock_retries
  {
    operation_name="UpdatePost"
  }
  12  # 12 deadlock retries in this window
```text

**1.2.3 Subscription Metrics**

```text
fraiseql_subscriptions_active
  {
    subscription_name="OnPostCreated",
    database="postgresql"
  }
  450  # 450 active subscriptions

fraiseql_subscription_events_published
  {
    subscription_name="OnPostCreated",
    status="delivered"  # delivered, dropped, failed
  }
  15000  # 15K events delivered this window

fraiseql_subscription_event_delay_seconds
  {
    subscription_name="OnPostCreated",
    quantile="p95"
  }
  0.050  # 50ms event delay p95

fraiseql_subscription_buffer_utilization
  {
    subscription_name="OnPostCreated"
  }
  0.45  # 45% of buffer used (1000 event capacity)
```text

**1.2.4 Authorization Metrics**

```text
fraiseql_authorization_checks_total
  {
    result="allowed",    # allowed, denied
    rule_type="owner_only"
  }
  45000  # 45K authorization checks

fraiseql_authorization_duration_seconds
  {
    rule_type="owner_only",
    quantile="p95"
  }
  0.005  # 5ms for authorization check

fraiseql_authorization_denials
  {
    reason="insufficient_role",  # insufficient_role, row_level_denied
    rule_type="admin_only"
  }
  15  # 15 denials this window
```text

**1.2.5 Cache Metrics**

```text
fraiseql_cache_requests_total
  {
    result="hit",    # hit, miss, expire
    operation_name="GetUserPosts"
  }
  3500  # 3500 cache hits

fraiseql_cache_requests_total
  {
    result="miss",
    operation_name="GetUserPosts"
  }
  500   # 500 cache misses (87% hit rate)

fraiseql_cache_size_bytes
  {
    cache_backend="redis"
  }
  536870912  # 512MB used

fraiseql_cache_ttl_max_seconds
  {}
  300  # Max TTL 5 minutes
```text

**1.2.6 Database Connection Metrics**

```text
fraiseql_db_connections_active
  {
    database="postgresql",
    pool_size=50
  }
  43  # 43 connections active

fraiseql_db_connection_wait_time_seconds
  {
    database="postgresql",
    quantile="p95"
  }
  0.001  # 1ms max wait for connection

fraiseql_db_query_rows_scanned
  {
    operation_name="GetUserPosts"
  }
  1000  # Scanned 1000 rows to return 20

fraiseql_db_indexes_used
  {
    operation_name="GetUserPosts",
    index="idx_post_published"
  }
  1  # Index was used (1 = yes)
```text

**1.2.7 Error Metrics**

```text
fraiseql_errors_total
  {
    error_code="E_DB_QUERY_TIMEOUT_302",
    category="DATABASE_ERROR"
  }
  25  # 25 timeout errors this window

fraiseql_errors_total
  {
    error_code="E_AUTH_PERMISSION_401",
    category="AUTHORIZATION_ERROR"
  }
  500  # 500 authorization denials

fraiseql_errors_total
  {
    error_code="E_VALIDATION_INVALID_TYPE_103",
    category="VALIDATION_ERROR"
  }
  80   # 80 input validation errors
```text

### 1.3 Custom Metrics (Application-Defined)

Applications can define custom business metrics:

```python
@FraiseQL.type
class User:
    id: ID
    email: str

    @FraiseQL.metric(name="user_created", type="counter")
    def track_creation(self):
        """Track when users are created"""
        # Automatically incremented on mutation

@FraiseQL.type
class Order:
    id: ID
    total: float

    @FraiseQL.metric(name="order_revenue", type="gauge", value_field="total")
    def track_revenue(self):
        """Track total order revenue"""
        # Value automatically updated

@FraiseQL.resolver
def custom_metric_handler():
    """Define custom metrics for business logic"""
    FraiseQL.metrics.gauge(
        name="active_users",
        value=count_active_users()
    )
    FraiseQL.metrics.gauge(
        name="pending_orders",
        value=count_pending_orders()
    )
```text

**Custom metric example:**

```text
fraiseql_custom_user_created
  {}
  1250  # 1250 users created

fraiseql_custom_order_revenue
  {
    currency="USD"
  }
  450000.50  # $450K in orders
```text

### 1.4 Metric Export Formats

**Prometheus format (default):**

```text
# HELP fraiseql_query_requests_total Total queries executed
# TYPE fraiseql_query_requests_total counter
fraiseql_query_requests_total{operation_name="GetUserPosts",status="success"} 5000

# HELP fraiseql_query_duration_seconds Query execution duration
# TYPE fraiseql_query_duration_seconds histogram
fraiseql_query_duration_seconds_bucket{operation_name="GetUserPosts",le="0.01"} 500
fraiseql_query_duration_seconds_bucket{operation_name="GetUserPosts",le="0.05"} 4500
fraiseql_query_duration_seconds_bucket{operation_name="GetUserPosts",le="0.1"} 4800
fraiseql_query_duration_seconds_bucket{operation_name="GetUserPosts",le="+Inf"} 5000
fraiseql_query_duration_seconds_sum{operation_name="GetUserPosts"} 225
fraiseql_query_duration_seconds_count{operation_name="GetUserPosts"} 5000
```text

**JSON export:**

```json
{
  "metrics": [
    {
      "name": "fraiseql_query_requests_total",
      "value": 5000,
      "labels": {
        "operation_name": "GetUserPosts",
        "status": "success"
      },
      "timestamp": "2026-01-15T10:30:45Z"
    }
  ]
}
```text

**CloudWatch format (AWS):**

```json
{
  "MetricData": [
    {
      "MetricName": "FraiseQLQueryRequests",
      "Dimensions": [
        {"Name": "OperationName", "Value": "GetUserPosts"}
      ],
      "Value": 5000,
      "Unit": "Count"
    }
  ]
}
```text

### 1.5 Metric Aggregation & Queries

**Prometheus queries:**

```promql
# Average query latency
rate(fraiseql_query_duration_seconds_sum[5m]) / rate(fraiseql_query_duration_seconds_count[5m])

# Query error rate
rate(fraiseql_query_requests_total{status="error"}[5m]) / rate(fraiseql_query_requests_total[5m])

# Cache hit rate
rate(fraiseql_cache_requests_total{result="hit"}[5m]) / rate(fraiseql_cache_requests_total[5m])

# P99 query latency
histogram_quantile(0.99, rate(fraiseql_query_duration_seconds_bucket[5m]))

# Top queries by latency
topk(5, rate(fraiseql_query_duration_seconds_sum[5m]) / rate(fraiseql_query_duration_seconds_count[5m]))
```text

---

## 2. Structured Logging

### 2.1 Log Levels & Categories

FraiseQL produces structured logs at multiple levels:

| Level | Usage | Frequency | Retention |
|-------|-------|-----------|-----------|
| **DEBUG** | Development, detailed flow | High | 24 hours |
| **INFO** | Significant events | Medium | 7 days |
| **WARN** | Unusual but handled situations | Low | 30 days |
| **ERROR** | Failed operations | Low | 90 days |
| **FATAL** | System failures | Very low | 1 year |

### 2.2 Log Entry Format

All logs are structured JSON with consistent schema:

```json
{
  "timestamp": "2026-01-15T10:30:45.123Z",
  "level": "info",
  "logger": "FraiseQL.query",
  "message": "Query executed successfully",
  "context": {
    "request_id": "req-abc123",
    "trace_id": "trace-xyz789",
    "user_id": "user-456",
    "organization_id": "org-123"
  },
  "operation": {
    "type": "query",
    "name": "GetUserPosts",
    "status": "success",
    "duration_ms": 45
  },
  "database": {
    "engine": "postgresql",
    "query_time_ms": 35,
    "rows_affected": 20,
    "connection_id": "conn-789"
  },
  "cache": {
    "hit": true,
    "ttl_seconds": 300
  },
  "authorization": {
    "allowed": true,
    "rules_evaluated": 3,
    "time_ms": 2
  },
  "error": null,
  "metadata": {
    "version": "2.0.0",
    "environment": "production"
  }
}
```text

### 2.3 Query Execution Logs

**Query start (DEBUG):**

```json
{
  "timestamp": "2026-01-15T10:30:45.000Z",
  "level": "debug",
  "message": "Query execution started",
  "operation": {
    "type": "query",
    "name": "GetUserPosts"
  },
  "parameters": {
    "userId": "user-456",
    "limit": 20
  }
}
```text

**Query completion (INFO):**

```json
{
  "timestamp": "2026-01-15T10:30:45.045Z",
  "level": "info",
  "message": "Query executed successfully",
  "operation": {
    "type": "query",
    "name": "GetUserPosts",
    "status": "success",
    "duration_ms": 45
  },
  "database": {
    "query_time_ms": 35,
    "rows_affected": 20
  },
  "cache": {
    "hit": false,
    "cached": true,
    "ttl_seconds": 300
  }
}
```text

**Query timeout (ERROR):**

```json
{
  "timestamp": "2026-01-15T10:30:45.000Z",
  "level": "error",
  "message": "Query timeout",
  "operation": {
    "type": "query",
    "name": "GetUserPosts",
    "status": "timeout",
    "duration_ms": 30000
  },
  "error": {
    "code": "E_DB_QUERY_TIMEOUT_302",
    "message": "Query execution exceeded 30 second timeout",
    "retryable": true
  }
}
```text

### 2.4 Mutation Execution Logs

**Mutation start (DEBUG):**

```json
{
  "timestamp": "2026-01-15T10:30:45.000Z",
  "level": "debug",
  "message": "Mutation execution started",
  "operation": {
    "type": "mutation",
    "name": "CreatePost"
  },
  "input": {
    "title": "New Post",
    "content": "Content preview..."
  }
}
```text

**Mutation committed (INFO):**

```json
{
  "timestamp": "2026-01-15T10:30:45.050Z",
  "level": "info",
  "message": "Mutation committed",
  "operation": {
    "type": "mutation",
    "name": "CreatePost",
    "status": "success",
    "duration_ms": 50
  },
  "database": {
    "rows_affected": 1,
    "transaction_duration_ms": 48
  },
  "events": {
    "published": ["post_created"],
    "subscribers": 127
  }
}
```text

**Mutation rolled back (ERROR):**

```json
{
  "timestamp": "2026-01-15T10:30:45.050Z",
  "level": "error",
  "message": "Mutation rolled back due to constraint violation",
  "operation": {
    "type": "mutation",
    "name": "CreatePost",
    "status": "rolled_back",
    "duration_ms": 50
  },
  "error": {
    "code": "E_VALIDATION_DUPLICATE_VALUE_107",
    "message": "Post with title 'New Post' already exists",
    "retryable": false
  }
}
```text

### 2.5 Authorization Logs

**Authorization allowed (DEBUG):**

```json
{
  "timestamp": "2026-01-15T10:30:45.002Z",
  "level": "debug",
  "message": "Authorization check passed",
  "authorization": {
    "result": "allowed",
    "rule": "owner_or_admin",
    "field": "User.email",
    "user_id": "user-456",
    "resource_owner": "user-456",
    "duration_ms": 1
  }
}
```text

**Authorization denied (WARN):**

```json
{
  "timestamp": "2026-01-15T10:30:45.002Z",
  "level": "warn",
  "message": "Authorization check failed",
  "authorization": {
    "result": "denied",
    "rule": "admin_only",
    "field": "AdminPanel.api_keys",
    "user_id": "user-456",
    "user_roles": ["user"],
    "required_role": "admin",
    "duration_ms": 2
  }
}
```text

### 2.6 Error Logs

**All errors include:**

```json
{
  "error": {
    "code": "E_DB_QUERY_TIMEOUT_302",
    "category": "DATABASE_ERROR",
    "message": "Query exceeded timeout",
    "severity": "error",
    "retryable": true,
    "remediable": false,
    "trace": [
      "FraiseQL.runtime.execute_query:123",
      "FraiseQL.db.query:456",
      "tokio.timeout:789"
    ]
  }
}
```text

### 2.7 Log Filtering & Sampling

**Debug log sampling (production):**

```python
# By default, DEBUG logs are sampled (1 in 100)
FraiseQL.logging.configure({
    "debug_sampling": {
        "enabled": True,
        "rate": 0.01,  # 1% of debug logs
        "always_sample_errors": True  # Always log errors
    }
})
```text

**Dynamic log levels:**

```bash
# Change log level without restart
curl -X POST http://localhost:8000/admin/logging \
  -d '{
    "logger": "FraiseQL.query",
    "level": "debug"
  }'

# Result: FraiseQL.query logs now at DEBUG level
# Reverts after 1 hour or on restart
```text

---

## 3. Distributed Tracing

### 3.1 Trace Context Propagation

Every request includes **trace context** for distributed tracing:

```json
{
  "trace_id": "4bf92f3577b34da6a3ce929d0e0e4736",
  "span_id": "00f067aa0ba902b7",
  "parent_span_id": "00f067aa0ba902b7",
  "trace_flags": "01"  // Sampled
}
```text

**Context propagation:**

```text
Client Request
  ↓ (contains trace_id)
FraiseQL API
  ├─ Creates span: "query.execution"
  │  ├─ Creates span: "authorization.check"
  │  ├─ Creates span: "database.query"
  │  │  └─ Includes trace_id in database driver
  │  └─ Creates span: "response.transform"
  └─ Returns response with trace_id
```text

### 3.2 W3C Trace Context Headers

FraiseQL uses W3C standard for trace propagation:

```text
HTTP Request:
traceparent: 00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01
tracestate: congo=t61rcWpm35YzTP60
```text

**Header format:**

```text
traceparent: version-trace_id-parent_span_id-trace_flags
00         = version 0 (W3C spec v1)
4bf92...   = trace ID (16 bytes hex)
00f067...  = parent span ID (8 bytes hex)
01         = trace flags (01 = sampled)
```text

### 3.3 Span Hierarchy

Every query generates trace spans:

```text
Span: query.execution (root)
├─ start: 2026-01-15T10:30:45.000Z
├─ end: 2026-01-15T10:30:45.045Z
├─ duration: 45ms
├─ attributes:
│  ├─ operation_name: "GetUserPosts"
│  ├─ user_id: "user-456"
│  └─ status: "success"
│
├─ Span: validation (child)
│  ├─ start: 2026-01-15T10:30:45.000Z
│  ├─ end: 2026-01-15T10:30:45.002Z
│  ├─ duration: 2ms
│  └─ attributes:
│     └─ valid: true
│
├─ Span: authorization (child)
│  ├─ start: 2026-01-15T10:30:45.002Z
│  ├─ end: 2026-01-15T10:30:45.005Z
│  ├─ duration: 3ms
│  └─ attributes:
│     ├─ allowed: true
│     └─ rules_evaluated: 2
│
├─ Span: database.query (child)
│  ├─ start: 2026-01-15T10:30:45.005Z
│  ├─ end: 2026-01-15T10:30:45.040Z
│  ├─ duration: 35ms
│  └─ attributes:
│     ├─ database: "postgresql"
│     ├─ statement: "SELECT ... FROM v_post WHERE ..."
│     ├─ rows: 20
│     └─ status: "success"
│
└─ Span: response.transform (child)
   ├─ start: 2026-01-15T10:30:45.040Z
   ├─ end: 2026-01-15T10:30:45.045Z
   ├─ duration: 5ms
   └─ attributes:
      ├─ format: "json"
      └─ size_bytes: 5120
```text

### 3.4 Span Attributes (Events)

Each span records attributes about the operation:

```json
{
  "spans": [
    {
      "name": "database.query",
      "attributes": {
        "database.system": "postgresql",
        "database.connection.pool.name": "default",
        "database.connection.state": "in_use",
        "db.statement": "SELECT * FROM v_post WHERE ...",
        "db.rows_returned": 20,
        "db.rows_scanned": 1000,
        "db.execution_time_ms": 35,
        "db.prepared_statement": true,
        "db.indexes_used": ["idx_post_published"],
        "db.cache_hit": false
      }
    }
  ]
}
```text

### 3.5 Sampling Strategy

**Adaptive sampling based on error rate:**

```rust
// By default: Sample 1% of traces (cost reduction)
if error_rate > 0.01 {  // If >1% error rate
    sampling_rate = 0.10;  // Sample 10% (more visibility)
}

if error_rate > 0.05 {  // If >5% error rate
    sampling_rate = 1.0;   // Sample 100% (full debugging)
}
```text

**Request context sampling:**

```python
# Sample 100% of requests with errors
if response.status == "error":
    trace.sample_rate = 1.0

# Sample 100% of slow requests
elif response.duration_ms > timeout_threshold:
    trace.sample_rate = 1.0

# Sample 100% if user opt-in
elif user.prefer_full_tracing:
    trace.sample_rate = 1.0

# Default: Sample 1%
else:
    trace.sample_rate = 0.01
```text

### 3.6 Trace Export

**Jaeger format (default):**

```json
{
  "traceID": "4bf92f3577b34da6a3ce929d0e0e4736",
  "spans": [
    {
      "traceID": "4bf92f3577b34da6a3ce929d0e0e4736",
      "spanID": "00f067aa0ba902b7",
      "operationName": "query.execution",
      "startTime": 1642252245000000,
      "duration": 45000,
      "tags": {
        "operation_name": "GetUserPosts",
        "user_id": "user-456"
      },
      "logs": [
        {
          "timestamp": 1642252245005000,
          "fields": [
            {"key": "event", "value": "authorization_passed"}
          ]
        }
      ]
    }
  ]
}
```text

**OpenTelemetry format:**

```python
{
  "resourceSpans": [
    {
      "resource": {
        "attributes": [
          {"key": "service.name", "value": {"stringValue": "FraiseQL"}},
          {"key": "service.version", "value": {"stringValue": "2.0.0"}}
        ]
      },
      "scopeSpans": [
        {
          "span": [
            {
              "traceId": "4bf92f3577b34da6a3ce929d0e0e4736",
              "spanId": "00f067aa0ba902b7",
              "name": "query.execution",
              "startTimeUnixNano": 1642252245000000000,
              "endTimeUnixNano": 1642252245045000000
            }
          ]
        }
      ]
    }
  ]
}
```text

---

## 4. Alerting Rules

### 4.1 Pre-Built Alert Templates

FraiseQL includes pre-built Prometheus alert rules:

```yaml
groups:
  - name: FraiseQL.alerts
    interval: 30s
    rules:
      - alert: QueryLatencyHigh
        expr: histogram_quantile(0.95, fraiseql_query_duration_seconds_bucket) > 1.0
        for: 5m
        annotations:
          summary: "Query latency high (p95 > 1s)"
          description: "Queries taking > 1s on average"

      - alert: QueryErrorRateHigh
        expr: rate(fraiseql_query_requests_total{status="error"}[5m]) > 0.01
        for: 5m
        annotations:
          summary: "Query error rate > 1%"

      - alert: MutationDeadlockHigh
        expr: rate(fraiseql_mutation_deadlock_retries[5m]) > 10
        for: 5m
        annotations:
          summary: "High deadlock rate on mutations"

      - alert: CacheHitRateLow
        expr: rate(fraiseql_cache_requests_total{result="hit"}[5m]) / rate(fraiseql_cache_requests_total[5m]) < 0.5
        for: 10m
        annotations:
          summary: "Cache hit rate below 50%"
          remediation: "Check cache configuration or TTL"

      - alert: DatabaseConnectionPoolExhausted
        expr: fraiseql_db_connections_active / fraiseql_db_pool_size > 0.9
        for: 2m
        annotations:
          summary: "Database connection pool 90% utilized"
          remediation: "Increase pool size or check for connection leaks"

      - alert: SubscriptionBufferNearCapacity
        expr: fraiseql_subscription_buffer_utilization > 0.8
        for: 5m
        annotations:
          summary: "Subscription buffer > 80% utilized"
          remediation: "Check subscription delivery speed"

      - alert: AuthorizationDenialHigh
        expr: rate(fraiseql_authorization_denials[5m]) > 100
        for: 5m
        annotations:
          summary: "High authorization denial rate"

      - alert: DatabaseQueryTimeout
        expr: rate(fraiseql_errors_total{error_code="E_DB_QUERY_TIMEOUT_302"}[5m]) > 5
        for: 5m
        annotations:
          summary: "Database queries timing out"
          remediation: "Optimize slow queries or increase timeout"
```text

### 4.2 Custom Alerts

Applications can define custom alerts:

```python
@FraiseQL.alert
def high_order_value(context):
    """Alert if single order exceeds threshold"""
    return {
        "condition": f"order.total > 10000",
        "for": "immediately",
        "message": f"High-value order: ${context.order.total}"
    }

@FraiseQL.alert
def slow_query_detection(context):
    """Alert on slow queries"""
    return {
        "condition": f"context.operation.duration_ms > 5000",
        "for": "5m",
        "message": f"Query {context.operation.name} taking {context.operation.duration_ms}ms"
    }
```text

### 4.3 Alert Routing & Notification

```yaml
# AlertManager configuration
route:
  receiver: default
  routes:
    # Critical: Database down → PagerDuty
    - match:
        severity: critical
      receiver: pagerduty

    # High: Performance degradation → Slack
    - match:
        severity: high
      receiver: slack_eng

    # Medium: Warnings → Email
    - match:
        severity: medium
      receiver: email

receivers:
  - name: pagerduty
    pagerduty_configs:
      - routing_key: secret
  - name: slack_eng
    slack_configs:
      - api_url: "https://hooks.slack.com/..."
  - name: email
    email_configs:
      - to: alerts@company.com
```text

---

## 5. Health Checks & Readiness Probes

### 5.1 Health Check Endpoints

```bash
# Liveness probe (is runtime alive?)
GET /health/live
200 OK {"status": "alive"}

# Readiness probe (can runtime serve traffic?)
GET /health/ready
200 OK {
  "status": "ready",
  "database": "connected",
  "cache": "connected",
  "auth_provider": "connected"
}

# Detailed health status
GET /health
200 OK {
  "status": "healthy",
  "components": {
    "runtime": {"status": "up", "version": "2.0.0"},
    "database": {"status": "up", "latency_ms": 2},
    "cache": {"status": "up", "hit_rate": 0.87},
    "auth_provider": {"status": "up"},
    "subscription_manager": {"status": "up", "active_subscriptions": 450}
  },
  "uptime_seconds": 86400,
  "start_time": "2026-01-14T10:30:45Z"
}
```text

### 5.2 Kubernetes Probes

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: FraiseQL
spec:
  template:
    spec:
      containers:
      - name: FraiseQL
        livenessProbe:
          httpGet:
            path: /health/live
            port: 8000
          initialDelaySeconds: 10
          periodSeconds: 10
          timeoutSeconds: 2
          failureThreshold: 3

        readinessProbe:
          httpGet:
            path: /health/ready
            port: 8000
          initialDelaySeconds: 5
          periodSeconds: 5
          timeoutSeconds: 2
          failureThreshold: 2
```text

---

## 6. Performance Profiling

### 6.1 Built-in Profiler

FraiseQL includes built-in CPU and memory profiling:

```bash
# Profile CPU (30 seconds)
GET /debug/pprof/profile?seconds=30

# Profile memory
GET /debug/pprof/heap

# Profile goroutines
GET /debug/pprof/goroutine

# Download as pprof format
curl http://localhost:8000/debug/pprof/profile > cpu.prof
go tool pprof cpu.prof
```text

### 6.2 Query Execution Plan Analysis

```bash
# Analyze query plan
curl -X POST http://localhost:8000/debug/analyze \
  -d '{
    "query": "query GetPosts { posts { id title } }"
  }'

Response:
{
  "query_plan": {
    "sql": "SELECT ... FROM v_post",
    "estimated_cost": 100,
    "estimated_rows": 1000,
    "indexes": ["idx_post_published"],
    "joins": 0,
    "nested_queries": 0
  },
  "recommendation": "Query is well-optimized"
}
```text

### 6.3 Slow Query Log

```json
{
  "timestamp": "2026-01-15T10:30:45.000Z",
  "level": "warn",
  "message": "Slow query detected",
  "operation": {
    "name": "ComplexUserSearch",
    "duration_ms": 5234
  },
  "database": {
    "query": "SELECT ... FROM v_user JOIN v_profile ...",
    "rows_scanned": 50000,
    "rows_returned": 10,
    "indexes_not_used": ["idx_user_email"],
    "suggestion": "Add WHERE clause to filter by email first"
  }
}
```text

---

## 7. Observability Configuration

### 7.1 Configuration Options

```python
FraiseQL.observability.configure({
    # Metrics
    "metrics": {
        "enabled": True,
        "export_interval_seconds": 60,
        "format": "prometheus",
        "include_high_cardinality": False,  # Limit label combinations
    },

    # Logging
    "logging": {
        "level": "info",
        "format": "json",
        "debug_sampling": {
            "enabled": True,
            "rate": 0.01,  # 1% of debug logs
        },
        "slow_query_threshold_ms": 1000,
    },

    # Tracing
    "tracing": {
        "enabled": True,
        "sample_rate": 0.01,  # 1% of requests
        "export_interval_seconds": 60,
        "exporters": ["jaeger", "datadog"],
    },

    # Health checks
    "health_checks": {
        "enabled": True,
        "check_interval_seconds": 30,
        "database_timeout_ms": 2000,
        "cache_timeout_ms": 500,
    },

    # Profiling
    "profiling": {
        "enabled": True,  # In non-production only
        "cpu_sample_rate": 0.01,
        "memory_sample_rate": 0.1,
    },
})
```text

### 7.2 Environment Variables

```bash
# Enable debug logging
FRAISEQL_LOG_LEVEL=debug

# Set trace sample rate
FRAISEQL_TRACE_SAMPLE_RATE=0.1

# Disable metrics (cost reduction)
FRAISEQL_METRICS_ENABLED=false

# Set slow query threshold
FRAISEQL_SLOW_QUERY_THRESHOLD_MS=5000

# Export metrics to Datadog
FRAISEQL_METRICS_EXPORTER=datadog
DD_AGENT_HOST=localhost
DD_AGENT_PORT=8125

# Export traces to Jaeger
FRAISEQL_TRACE_EXPORTER=jaeger
JAEGER_AGENT_HOST=localhost
JAEGER_AGENT_PORT=6831
```text

---

## 8. Dashboard Examples

### 8.1 Grafana Dashboard: Query Performance

```text
┌──────────────────────────────────────────────────────────┐
│ FraiseQL Query Performance Dashboard                      │
├──────────────────────────────────────────────────────────┤
│                                                            │
│  Queries/sec ↑ 8500    Errors ↑ 0.2%    Cache Hit ↑ 87%  │
│                                                            │
│  ┌─────────────────────┐  ┌──────────────────────────┐   │
│  │ Query Latency (p95) │  │ Top Slow Queries         │   │
│  │ ████░░░░░░  245ms   │  │ 1. ComplexSearch: 5.2s   │   │
│  └─────────────────────┘  │ 2. JoinedQuery: 3.1s     │   │
│                            │ 3. NestedFetch: 2.8s     │   │
│  ┌─────────────────────┐  │ 4. TimeSeriesAgg: 2.1s   │   │
│  │ Cache Hit Rate      │  │ 5. UserAnalytics: 1.9s   │   │
│  │ ██████████ 87%      │  └──────────────────────────┘   │
│  └─────────────────────┘                                  │
│                                                            │
│  Error Distribution        Database Connection Pool       │
│  ├─ Validation: 45%        Active:     43/50              │
│  ├─ Authorization: 30%      Waiting:    2                 │
│  ├─ Database: 20%           Idle:       5                 │
│  └─ Other: 5%               Utilization: 86%              │
│                                                            │
└──────────────────────────────────────────────────────────┘
```text

### 8.2 Grafana Dashboard: System Health

```text
┌──────────────────────────────────────────────────────────┐
│ FraiseQL System Health Dashboard                          │
├──────────────────────────────────────────────────────────┤
│                                                            │
│  Uptime: 28d 15h  Database Latency: 2.1ms  Memory: 512MB │
│                                                            │
│  ┌─────────────────────┐  ┌──────────────────────────┐   │
│  │ Request Rate        │  │ Active Subscriptions     │   │
│  │ ▁▁▂▂▃▃▄▄▅▅▆▆▇▇██   │  │ ▁▁▁▂▂▂▃▃▃▄▄▄▅▅▅▆▆▆▇▇  │   │
│  │ 10K req/s           │  │ 450 active               │   │
│  └─────────────────────┘  └──────────────────────────┘   │
│                                                            │
│  Authorization Latency      Cache Backend Status         │
│  │ owner_only: 2.1ms        Redis: ✅ Connected          │
│  │ role_based: 3.5ms        Size: 512MB / 1GB            │
│  │ custom_rule: 8.2ms       Hit Rate: 87%                │
│                                                            │
└──────────────────────────────────────────────────────────┘
```text

---

## 9. Troubleshooting Guide

### 9.1 Using Traces to Debug Slow Queries

**Problem**: Query taking 5 seconds

**Solution:**

```text

1. Check trace span: query.execution (5000ms)
   ├─ validation: 2ms ✓ Fast
   ├─ authorization: 3ms ✓ Fast
   ├─ database.query: 4990ms ✗ SLOW
   └─ response.transform: 5ms ✓ Fast

2. Database query is the culprit. Check database span:
   - Database: PostgreSQL
   - Statement: SELECT ... FROM v_user WHERE ...
   - Rows scanned: 500,000
   - Rows returned: 100
   - Indexes used: None

3. Recommendation: Add index on filter column
   CREATE INDEX idx_user_status ON tb_user(status);

4. After fix: Trace shows 35ms total (100x faster!)
```text

### 9.2 Using Logs to Debug Authorization

**Problem**: Some users can't access field

**Solution:**

```text

1. Find error log:
   "Authorization check failed"
   "rule": "owner_or_admin"
   "user_id": "user-456"
   "result": "denied"

2. Check if user is owner or admin:
   SELECT user_id, roles FROM tb_user WHERE id = 'user-456'
   → roles = ['user'] (not admin)

3. Check if user is resource owner:
   SELECT author_id FROM tb_post WHERE id = 'post-789'
   → author_id = 'user-123' (not user-456)

4. Conclusion: User denied correctly
   Recommendation: User must be owner or admin
```text

---

## 10. Best Practices

### 10.1 Observability Configuration

- ✅ Enable all metrics in production
- ✅ Use sampling for traces (1-10% by default)
- ✅ Sample debug logs (1% by default)
- ✅ Set appropriate alert thresholds
- ✅ Monitor cache hit rates
- ✅ Track query latency p95/p99
- ✅ Track mutation deadlock rates
- ✅ Monitor subscription buffer utilization

### 10.2 Using Traces Effectively

- ✅ Use traces to identify bottlenecks (query vs auth vs database)
- ✅ Compare traces of slow vs fast queries
- ✅ Look for unexpected database calls in response transform
- ✅ Check span duration breakdown
- ✅ Use custom span attributes for context

### 10.3 Interpreting Alerts

- ⚠️ High query latency → Check slow query log, optimize queries
- ⚠️ High error rate → Check error logs for patterns
- ⚠️ Low cache hit rate → Increase cache size or TTL
- ⚠️ Connection pool near capacity → Increase pool size or profile for leaks
- ⚠️ High deadlock rate → Add indexes or review transaction logic

---

**Document Version**: 1.0.0
**Last Updated**: January 2026
**Status**: Complete specification for framework v2.x

FraiseQL's observability model provides complete visibility into system behavior through metrics, logs, and traces. Every operation is observable; no special configuration needed.
