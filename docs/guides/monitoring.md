# Monitoring & Observability Guide

**Version**: FraiseQL v1.8+
**Status:** Complete
**Topics**: Prometheus, OpenTelemetry, Health Checks, APQ Metrics, Query Analytics, Error Tracking

---

## Prerequisites

**Required Knowledge:**
- Prometheus metrics format and scrape configuration
- OpenTelemetry (OTLP) protocol and exporters
- Grafana dashboard design and queries
- Kubernetes health probe concepts (liveness, readiness)
- Query complexity analysis and cost calculation
- Error grouping and fingerprinting techniques
- HTTP/REST health check conventions

**Required Software:**
- FraiseQL v2.0.0-alpha.1 or later (with observability features)
- Prometheus 2.40+ (for metrics scraping and storage)
- Grafana 9.0+ (for visualization and dashboards)
- Jaeger or Zipkin (for distributed tracing)
- A text editor for configuration files
- curl or Postman (for health check testing)
- Optional: Python/Go/Node for custom exporters

**Required Infrastructure:**
- FraiseQL server instance with metrics endpoint exposed
- PostgreSQL 14+ database (for APQ cache and error tracking)
- Prometheus server with storage
- Grafana server for dashboarding
- Jaeger collector or Zipkin server (for tracing)
- Network connectivity between all monitoring components
- 5-10GB storage for metrics time-series data

**Optional but Recommended:**
- AlertManager for alert routing and deduplication
- Custom Grafana datasources (DataDog, New Relic, Splunk)
- Kubernetes monitoring stack (Prometheus Operator)
- Webhook integration for custom alerting
- Custom exporters for third-party systems
- Performance baseline tracking tools

**Time Estimate:** 30-60 minutes for basic Prometheus setup, 2-4 hours for production dashboards and alerts

## Overview

FraiseQL provides comprehensive monitoring and observability features for production deployments:

- **Prometheus Metrics**: 15+ metric types for queries, mutations, cache, database, and errors
- **OpenTelemetry Integration**: Distributed tracing with OTLP, Jaeger, and Zipkin exporters
- **Health Checks**: Kubernetes-compatible liveness and readiness probes
- **APQ Metrics**: Automatic Persisted Queries performance tracking and dashboard
- **Query Analytics**: Complexity scoring, depth analysis, cost calculation
- **Database Monitoring**: Query metrics, pool statistics, slow query tracking
- **Error Tracking**: PostgreSQL-native error grouping with fingerprinting
- **Security Logging**: Audit trail of authentication, authorization, and sensitive operations

---

## Quick Start

### Minimal Setup

```python
from fastapi import FastAPI
from fraiseql.monitoring import setup_metrics, MetricsConfig
from fraiseql.health import setup_health_endpoints

app = FastAPI()

# Enable Prometheus metrics
setup_metrics(app, MetricsConfig(enabled=True))

# Enable health check endpoints
setup_health_endpoints(app)
```

**Available Endpoints**:

- `GET /metrics` - Prometheus metrics
- `GET /health` - Full health status
- `GET /health/ready` - Readiness probe (Kubernetes)
- `GET /health/live` - Liveness probe (Kubernetes)

### Complete Setup

```python
from fastapi import FastAPI
from fraiseql.monitoring import setup_metrics, MetricsConfig
from fraiseql.tracing import setup_tracing, TracingConfig
from fraiseql.health import setup_health_endpoints
from fraiseql.monitoring import init_error_tracker

app = FastAPI()

# Prometheus metrics
setup_metrics(app, MetricsConfig(
    enabled=True,
    namespace="myapp",
    metrics_path="/metrics"
))

# OpenTelemetry tracing
setup_tracing(app, TracingConfig(
    enabled=True,
    service_name="myapp",
    export_format="otlp",
    export_endpoint="localhost:4317"  # OTLP collector
))

# Health checks
setup_health_endpoints(app)

# Error tracking
tracker = init_error_tracker(
    db_pool,
    environment="production",
    release_version="1.0.0",
    enable_notifications=True
)
```

---

## Prometheus Metrics

### Metric Types

FraiseQL exports 15+ metrics covering all operational aspects:

#### Query Metrics

| Metric | Type | Labels | Description |
|--------|------|--------|---|
| `fraiseql_graphql_queries_total` | Counter | operation_type, operation_name | Total queries executed |
| `fraiseql_graphql_query_duration_seconds` | Histogram | operation_type, operation_name | Query execution time (includes distribution) |
| `fraiseql_graphql_queries_success` | Counter | operation_type | Successful queries |
| `fraiseql_graphql_queries_errors` | Counter | operation_type | Failed queries |

**Labels**:

- `operation_type`: `query`, `mutation`, `subscription`
- `operation_name`: GraphQL operation name (if named)

#### Mutation Metrics

| Metric | Type | Labels | Description |
|--------|------|--------|---|
| `fraiseql_graphql_mutations_total` | Counter | mutation_name | Total mutations |
| `fraiseql_graphql_mutations_success` | Counter | mutation_name, result_type | Successful mutations |
| `fraiseql_graphql_mutations_errors` | Counter | mutation_name, error_type | Failed mutations |
| `fraiseql_graphql_mutation_duration_seconds` | Histogram | mutation_name | Mutation execution time |

#### Database Metrics

| Metric | Type | Labels | Description |
|--------|------|--------|---|
| `fraiseql_db_connections_active` | Gauge | - | Active database connections |
| `fraiseql_db_connections_idle` | Gauge | - | Idle connections in pool |
| `fraiseql_db_connections_total` | Gauge | - | Total pool size |
| `fraiseql_db_queries_total` | Counter | query_type, table_name | Total database queries |
| `fraiseql_db_query_duration_seconds` | Histogram | query_type | Query duration |

**Labels**:

- `query_type`: `SELECT`, `INSERT`, `UPDATE`, `DELETE`, `TRUNCATE`
- `table_name`: Database table name

#### Cache Metrics

| Metric | Type | Labels | Description |
|--------|------|--------|---|
| `fraiseql_cache_hits_total` | Counter | cache_type | Cache hits |
| `fraiseql_cache_misses_total` | Counter | cache_type | Cache misses |
| `fraiseql_cache_hit_rate` | Gauge | cache_type | Hit rate percentage (0-100) |

**Labels**:

- `cache_type`: `result_cache`, `query_cache` (APQ), `http_cache`

#### Error Metrics

| Metric | Type | Labels | Description |
|--------|------|--------|---|
| `fraiseql_errors_total` | Counter | error_type, error_code, operation | Total errors |
| `fraiseql_error_rate` | Gauge | error_type | Error rate percentage |

**Labels**:

- `error_type`: `validation`, `authorization`, `database`, `timeout`, `internal`
- `error_code`: HTTP or GraphQL error code
- `operation`: Operation type causing error

#### HTTP Metrics

| Metric | Type | Labels | Description |
|--------|------|--------|---|
| `fraiseql_http_requests_total` | Counter | method, endpoint, status | HTTP requests |
| `fraiseql_http_request_duration_seconds` | Histogram | method, endpoint | HTTP duration |

**Labels**:

- `method`: `GET`, `POST`, `PUT`, `DELETE`
- `endpoint`: Request path
- `status`: HTTP status code

#### Performance Metrics

| Metric | Type | Description |
|--------|------|---|
| `fraiseql_response_time_seconds` | Histogram | Overall response time |

### Histogram Buckets

Default buckets (customizable):

```
[0.005s, 0.01s, 0.025s, 0.05s, 0.1s, 0.25s, 0.5s, 1s, 2.5s, 5s, 10s]
```

Buckets correspond to:

- `5ms` - Extremely fast (in-memory cache hits)
- `10ms` - Very fast (simple queries)
- `25ms` - Fast (normal queries)
- `50ms` - Good (moderate queries)
- `100ms` - Acceptable (more complex)
- `250ms` - Slow warning threshold
- `500ms` - Performance concern threshold
- `1s` - Significant performance issue
- `2.5s`, `5s`, `10s` - Critical slowdowns

### Configuration

```python
from fraiseql.monitoring import MetricsConfig, setup_metrics

config = MetricsConfig(
    enabled=True,                    # Enable/disable metrics
    namespace="fraiseql",            # Metric prefix
    metrics_path="/metrics",         # Prometheus endpoint
    buckets=[0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1, 2.5, 5, 10],
    exclude_paths={                  # Don't measure these paths
        "/metrics",
        "/health",
        "/ready",
        "/startup"
    },
    labels={                         # Add custom labels to all metrics
        "environment": "production",
        "version": "1.0.0",
        "datacenter": "us-east-1"
    }
)

setup_metrics(app, config)
```

### Environment Variables

```bash
# Enable/disable
FRAISEQL_METRICS_ENABLED=true

# Metric prefix
FRAISEQL_METRICS_NAMESPACE=myapp

# Endpoint path
FRAISEQL_METRICS_PATH=/internal/metrics

# Histogram buckets (comma-separated)
FRAISEQL_METRICS_BUCKETS=0.005,0.01,0.025,0.05,0.1,0.25,0.5,1,2.5,5,10
```

### Prometheus Configuration

Add to `prometheus.yml`:

```yaml
scrape_configs:
  - job_name: 'fraiseql'
    static_configs:
      - targets: ['localhost:8000']  # Your FraiseQL server
    metrics_path: '/metrics'
    scrape_interval: 15s
    scrape_timeout: 10s
```

### Alerting Rules

Example Prometheus alerting rules:

```yaml
groups:
  - name: fraiseql
    interval: 30s
    rules:
      # High error rate
      - alert: FraiseQLHighErrorRate
        expr: rate(fraiseql_graphql_queries_errors_total[5m]) > 0.05
        for: 5m
        annotations:
          summary: "FraiseQL error rate > 5%"

      # Slow queries
      - alert: FraiseQLSlowQueries
        expr: histogram_quantile(0.95, fraiseql_graphql_query_duration_seconds) > 1
        for: 5m
        annotations:
          summary: "95th percentile query time > 1s"

      # Database connection pool exhaustion
      - alert: FraiseQLPoolNearFull
        expr: fraiseql_db_connections_active / fraiseql_db_connections_total > 0.8
        for: 5m
        annotations:
          summary: "Database connection pool > 80% utilized"

      # Low cache hit rate
      - alert: FraiseQLLowCacheHitRate
        expr: fraiseql_cache_hit_rate < 50
        for: 10m
        annotations:
          summary: "Cache hit rate < 50%"
```

---

## OpenTelemetry Tracing

### Overview

FraiseQL integrates with OpenTelemetry for distributed tracing across microservices.

**Supported Exporters**:

- **OTLP** (gRPC) - Recommended, standard OpenTelemetry Protocol
- **Jaeger** (Thrift) - Native Jaeger integration
- **Zipkin** (HTTP) - Zipkin-compatible format

### Setup

#### OTLP (Recommended)

```python
from fraiseql.tracing import setup_tracing, TracingConfig

config = TracingConfig(
    enabled=True,
    service_name="fraiseql-api",
    service_version="1.0.0",
    deployment_environment="production",
    export_format="otlp",
    export_endpoint="localhost:4317",  # OTLP Collector
    sample_rate=1.0,                   # 100% sampling
    attributes={
        "region": "us-east-1",
        "cluster": "prod-1"
    }
)

setup_tracing(app, config)
```

#### Jaeger

```python
config = TracingConfig(
    enabled=True,
    service_name="fraiseql-api",
    export_format="jaeger",
    export_endpoint="localhost:6831",  # Jaeger agent
    sample_rate=0.1                    # 10% sampling
)

setup_tracing(app, config)
```

#### Zipkin

```python
config = TracingConfig(
    enabled=True,
    service_name="fraiseql-api",
    export_format="zipkin",
    export_endpoint="http://zipkin:9411/api/v2/spans",
    sample_rate=0.5                    # 50% sampling
)

setup_tracing(app, config)
```

### Configuration

```python
@dataclass
class TracingConfig:
    enabled: bool = True
    service_name: str = "fraiseql"
    service_version: str = "unknown"
    deployment_environment: str = "development"
    sample_rate: float = 1.0              # 0.0-1.0
    export_endpoint: str | None = None    # Host:port or URL
    export_format: str = "otlp"           # otlp, jaeger, zipkin
    export_timeout_ms: int = 30000        # Timeout for exports
    propagate_traces: bool = True         # W3C Trace Context
    exclude_paths: set[str] = {           # Don't trace these
        "/health",
        "/ready",
        "/metrics",
        "/docs",
        "/openapi.json"
    }
    attributes: dict[str, Any] = {}       # Custom attributes
```

### Span Types

FraiseQL automatically creates spans for:

| Span Type | Description | Attributes |
|-----------|---|---|
| `graphql.query.{name}` | GraphQL query execution | operation_name, is_introspection |
| `graphql.mutation.{name}` | GraphQL mutation execution | mutation_name, is_introspection |
| `db.{type}.{table}` | Database operation | query_type, table_name, rows_affected |
| `cache.{op}.{type}` | Cache operation | operation (hit/miss/store), cache_type |
| `http.request` | HTTP request/response | method, path, status_code |

### Environment Variables

```bash
FRAISEQL_TRACING_ENABLED=true
FRAISEQL_TRACING_SERVICE_NAME=my-service
FRAISEQL_TRACING_SERVICE_VERSION=1.0.0
FRAISEQL_TRACING_ENVIRONMENT=production
FRAISEQL_TRACING_SAMPLE_RATE=1.0
FRAISEQL_TRACING_EXPORT_FORMAT=otlp
FRAISEQL_TRACING_EXPORT_ENDPOINT=localhost:4317
FRAISEQL_TRACING_EXPORT_TIMEOUT_MS=30000
```

---

## Health Checks

### Kubernetes-Compatible Endpoints

FraiseQL provides three Kubernetes probe endpoints:

#### **Liveness Probe** (`/health/live`)

Quick response indicating if the process is still running.

```yaml
# Kubernetes deployment spec
livenessProbe:
  httpGet:
    path: /health/live
    port: 8000
  initialDelaySeconds: 10
  periodSeconds: 10
```

#### **Readiness Probe** (`/health/ready`)

Comprehensive check indicating if the service is ready to accept traffic.

```yaml
readinessProbe:
  httpGet:
    path: /health/ready
    port: 8000
  initialDelaySeconds: 5
  periodSeconds: 5
```

#### **Full Health** (`/health`)

Complete health status with detailed information.

```bash
GET /health
```

Response:

```json
{
  "status": "healthy",
  "timestamp": "2025-01-11T15:30:00Z",
  "checks": {
    "database": {
      "status": "healthy",
      "message": "Database connection pool OK",
      "pool": {
        "active": 5,
        "idle": 10,
        "total": 15,
        "utilization": 0.33
      }
    },
    "cache": {
      "status": "healthy",
      "message": "Cache hit rate 75%",
      "hit_rate": 0.75
    },
    "graphql": {
      "status": "healthy",
      "message": "No recent errors",
      "success_rate": 0.98
    },
    "tracing": {
      "status": "healthy",
      "message": "OpenTelemetry active"
    }
  }
}
```

### Specialized Health Checks

```bash
# Database only
GET /health/database

# Cache only
GET /health/cache

# GraphQL only
GET /health/graphql

# Tracing only
GET /health/tracing
```

### Health Assessment Rules

**Database**:

- Pool utilization > 90% → Critical
- Pool utilization > 80% → Degraded
- Error rate > 5% → Critical
- Error rate > 1% → Degraded
- Slow query rate > 5% → Degraded

**Cache**:

- Hit rate < 50% → Critical
- Hit rate < 60% → Degraded
- Eviction rate high → Warning

**GraphQL**:

- Success rate < 90% → Critical
- Success rate < 95% → Degraded
- Operation latency high → Warning

### Configuration

```python
from fraiseql.health import setup_health_endpoints, HealthConfig

config = HealthConfig(
    pool_utilization_warning=0.8,     # 80%
    pool_utilization_critical=0.9,    # 90%
    slow_query_threshold_ms=100,
    slow_query_rate_warning=0.05,     # 5%
    error_rate_warning=0.01,          # 1%
    error_rate_critical=0.05,         # 5%
    cache_hit_rate_target=0.6,        # 60%
)

setup_health_endpoints(app, config)
```

---

## APQ Metrics & Dashboard

### Overview

Automatic Persisted Queries (APQ) metrics track query caching performance and hit rates.

### Endpoints

#### Dashboard (`/admin/apq/dashboard`)

Interactive HTML dashboard with charts and statistics.

```bash
GET /admin/apq/dashboard
```

Features:

- Query hit rate chart (historical)
- Top queries by usage
- Storage statistics
- Health status indicator
- Real-time updates

#### Statistics (`/admin/apq/stats`)

Comprehensive JSON statistics.

```bash
GET /admin/apq/stats
```

Response:

```json
{
  "query_cache": {
    "hits": 15000,
    "misses": 2000,
    "hit_rate": 0.88,
    "stores": 2000
  },
  "response_cache": {
    "hits": 8000,
    "misses": 7000,
    "hit_rate": 0.53,
    "stores": 7000
  },
  "storage": {
    "stored_queries": 45,
    "cached_responses": 120,
    "storage_bytes": 5242880
  },
  "performance": {
    "total_requests": 17000,
    "avg_query_parse_time_ms": 2.1,
    "overall_hit_rate": 0.72
  },
  "health": {
    "status": "healthy",
    "assessment": "Good hit rate"
  }
}
```

#### Top Queries (`/admin/apq/top-queries`)

Most frequently accessed queries.

```bash
GET /admin/apq/top-queries?limit=10
```

Response:

```json
{
  "queries": [
    {
      "hash": "a1b2c3d4...",
      "hit_count": 5000,
      "miss_count": 500,
      "hit_rate": 0.91,
      "avg_parse_time_ms": 1.5,
      "first_seen": "2025-01-10T10:00:00Z",
      "last_seen": "2025-01-11T15:30:00Z"
    }
  ]
}
```

#### Health (`/admin/apq/health`)

APQ system health status.

```bash
GET /admin/apq/health
```

### Metrics Collected

**Query Cache Metrics**:

- Total hits, misses, stores
- Hit rate (%)
- Average parse time (ms)

**Response Cache Metrics**:

- Total hits, misses, stores
- Hit rate (%)
- Estimated memory usage (bytes)

**Storage Statistics**:

- Unique queries stored
- Unique responses cached
- Total storage bytes

**Performance Indicators**:

- Requests per second (derived)
- P50, P95, P99 response times

---

## Query Analytics

### Query Complexity Scoring

Analyze GraphQL query complexity before execution.

```python
from fraiseql.analysis.query_complexity import analyze_query_complexity

query = """
{
  users(limit: 100) {
    id
    name
    posts(limit: 50) {
      id
      title
      comments(limit: 20) {
        id
        text
      }
    }
  }
}
"""

score = analyze_query_complexity(query, schema)
print(f"Complexity score: {score.total_score}")
print(f"Field count: {score.field_count}")
print(f"Max depth: {score.max_depth}")
print(f"Cache weight: {score.cache_weight}")
```

### Complexity Score Breakdown

```python
@dataclass
class ComplexityScore:
    field_count: int              # Base: 1 per field
    max_depth: int                # Max nesting level
    array_field_count: int        # Fields with array results
    type_diversity: int           # Unique types accessed
    fragment_count: int           # Reusable fragments
    total_score: float            # Composite metric
    cache_weight: float           # 0.1-10.0 (>3.0 avoid caching)
```

### Decision Making

```python
from fraiseql.analysis.query_complexity import should_cache_query

if should_cache_query(query, threshold=200):
    # Cache this query result
    pass
else:
    # Too complex, don't cache
    pass
```

---

## Database Monitoring

### Query Metrics

Track individual database queries.

```python
from fraiseql.monitoring import DatabaseMonitor

monitor = DatabaseMonitor(
    max_recent_queries=1000,
    slow_query_threshold_ms=100.0
)

# Record a query
await monitor.record_query(QueryMetrics(
    query_id="uuid...",
    query_hash="sha256...",
    query_type="SELECT",
    duration_ms=45.2,
    rows_affected=100,
    is_slow=False
))
```

### Query Statistics

Get aggregated statistics.

```python
stats = await monitor.get_query_statistics()
print(f"Total: {stats.total_count}")
print(f"Success rate: {stats.success_rate}%")
print(f"Average duration: {stats.avg_duration_ms}ms")
print(f"P95 duration: {stats.p95_duration_ms}ms")
print(f"Slow queries: {stats.slow_count}")
```

### Slow Query Detection

Find queries exceeding threshold.

```python
slow_queries = await monitor.get_slow_queries(limit=50)
for query in slow_queries:
    print(f"{query.query_type} on {query.table}: {query.duration_ms}ms")
```

### Performance Reports

Time-windowed analysis.

```python
from datetime import datetime, timedelta

end = datetime.utcnow()
start = end - timedelta(hours=1)

report = await monitor.get_performance_report(start, end)
print(f"Queries/min: {report.queries_per_minute}")
print(f"Slow queries: {report.slow_percentage}%")
```

### Connection Pool Monitoring

```python
pool_stats = monitor.get_pool_stats()
print(f"Active: {pool_stats.active_connections}")
print(f"Idle: {pool_stats.idle_connections}")
print(f"Utilization: {pool_stats.utilization * 100:.1f}%")
```

---

## Error Tracking

### Setup

```python
from fraiseql.monitoring import init_error_tracker

tracker = init_error_tracker(
    db_pool=db_pool,
    environment="production",
    release_version="1.0.0",
    enable_notifications=True,
    notification_channels={
        "slack": {
            "webhook_url": "https://hooks.slack.com/...",
            "rate_limit": "1/minute"
        },
        "email": {
            "recipients": ["ops@example.com"],
            "rate_limit": "1/minute"
        }
    }
)
```

### Capturing Errors

```python
try:
    result = await execute_query(...)
except Exception as e:
    error_id = await tracker.capture_exception(
        e,
        context={
            "user_id": "user-123",
            "request_id": "req-456"
        },
        tags=["critical", "graphql"]
    )
```

### Error Grouping

Errors are automatically grouped by fingerprint:

```
SHA256({error_type}:{filename}:{line_number}:{function_name})
```

Same errors from different requests are grouped together.

### Query Errors

```json
{
  "error_id": "err_a1b2c3d4e5f6",
  "error_fingerprint": "a1b2c3d4e5f6g7h8",
  "error_type": "QueryExecutionError",
  "error_message": "Timeout executing query",
  "stack_trace": "...",
  "request_context": {
    "method": "POST",
    "url": "/graphql",
    "headers": {...},
    "ip": "203.0.113.1",
    "user_agent": "Apollo Client"
  },
  "application_context": {
    "environment": "production",
    "release_version": "1.0.0"
  },
  "user_context": {
    "user_id": "user-123",
    "email": "user@example.com"
  },
  "trace_id": "...",
  "severity": "error",
  "first_seen": "2025-01-11T15:00:00Z",
  "last_seen": "2025-01-11T15:30:00Z",
  "occurrence_count": 47,
  "status": "unresolved"
}
```

### Error Management

```python
# Get error details
error = await tracker.get_error(error_id)

# Resolve error
await tracker.resolve_error(
    error_id,
    resolved_by="ops@example.com",
    notes="Applied hotfix in v1.0.1"
)

# Ignore error
await tracker.ignore_error(error_id)

# Get unresolved errors
unresolved = await tracker.get_unresolved_errors(limit=50)

# Get error statistics
stats = await tracker.get_error_stats(hours=24)
```

---

## Security & Audit Logging

### Security Events

Automatically logged security-relevant events:

```python
SecurityEventType:
AUTH_SUCCESS, AUTH_FAILURE, AUTH_TOKEN_EXPIRED
AUTHZ_DENIED, AUTHZ_FIELD_DENIED
RATE_LIMIT_EXCEEDED
CSRF_TOKEN_INVALID
QUERY_COMPLEXITY_EXCEEDED
DATA_ACCESS_DENIED
CONFIG_CHANGED
SYSTEM_INTRUSION_ATTEMPT
```

### Audit Event Structure

```json
{
  "event_type": "AUTH_FAILURE",
  "severity": "warning",
  "timestamp": "2025-01-11T15:30:00Z",
  "user_id": "user-123",
  "user_email": "user@example.com",
  "ip_address": "203.0.113.1",
  "user_agent": "Mozilla/5.0...",
  "request_id": "req-a1b2c3d4",
  "resource": "users",
  "action": "query",
  "result": "denied",
  "reason": "Invalid API key",
  "metadata": {
    "attempt_count": 3
  }
}
```

### Accessing Audit Logs

```python
from fraiseql.audit import get_security_logger

logger = get_security_logger()
events = await logger.get_events(
    event_type="AUTH_FAILURE",
    hours=24,
    limit=100
)

for event in events:
    print(f"{event.timestamp} {event.event_type}: {event.reason}")
```

---

## Dashboards & Visualization

### Grafana Dashboard

Example Grafana JSON configuration:

```json
{
  "dashboard": {
    "title": "FraiseQL Monitoring",
    "panels": [
      {
        "title": "Query Success Rate",
        "targets": [
          {
            "expr": "rate(fraiseql_graphql_queries_success[5m]) / rate(fraiseql_graphql_queries_total[5m])"
          }
        ]
      },
      {
        "title": "P95 Query Latency",
        "targets": [
          {
            "expr": "histogram_quantile(0.95, fraiseql_graphql_query_duration_seconds)"
          }
        ]
      },
      {
        "title": "Cache Hit Rate",
        "targets": [
          {
            "expr": "fraiseql_cache_hit_rate{cache_type=\"result_cache\"}"
          }
        ]
      },
      {
        "title": "Active Database Connections",
        "targets": [
          {
            "expr": "fraiseql_db_connections_active"
          }
        ]
      }
    ]
  }
}
```

### Key Dashboards

1. **Overview**: Error rate, success rate, latency, throughput
2. **Database**: Connection pool, query types, slow queries
3. **Cache**: Hit rates, evictions, memory usage
4. **Errors**: Top errors, error rate trends, affected users
5. **APQ**: Query cache hit rate, top queries, registration rate
6. **Security**: Auth failures, rate limits, suspicious patterns

---

## Production Best Practices

### Sampling Strategy

**Development**:

```python
TracingConfig(sample_rate=1.0)  # 100%
MetricsConfig(enabled=True)      # All metrics
```

**Staging**:

```python
TracingConfig(sample_rate=0.5)   # 50%
MetricsConfig(enabled=True)      # All metrics
```

**Production**:

```python
TracingConfig(sample_rate=0.1)   # 10% (adjust based on volume)
MetricsConfig(enabled=True)      # All metrics
```

### Alert Thresholds

**Critical Alerts** (page on-call):

- Error rate > 5%
- P99 latency > 5s
- Database pool > 90%
- Availability < 99%

**Warning Alerts** (ticket):

- Error rate > 1%
- P95 latency > 1s
- Database pool > 80%
- Cache hit rate < 50%

### Retention Policies

**Metrics**: 15 days (Prometheus default)
**Traces**: 24-72 hours (depends on volume)
**Logs**: 30 days
**Errors**: 90 days (with fingerprinting for deduplication)

### Privacy Considerations

- Don't log sensitive query parameters
- Redact personal information in errors
- Use IP hashing for privacy
- Configure log retention appropriately
- Use separate audit log system for compliance

---

## Troubleshooting

### Metrics Not Appearing

1. Verify endpoint is accessible: `curl http://localhost:8000/metrics`
2. Check `MetricsConfig.enabled = True`
3. Verify Prometheus scrape configuration
4. Check firewall/network policies

### Traces Not Exported

1. Verify export endpoint is accessible
2. Check `TracingConfig.export_endpoint` configuration
3. Verify exporter library installed (`opentelemetry-exporter-otlp-proto-grpc`)
4. Check OpenTelemetry Collector logs

### Health Checks Failing

1. Verify database connectivity
2. Check connection pool size (may be exhausted)
3. Verify cache system is operational
4. Check for recent error spike

### High Latency Detected

1. Check `GET /health` for pool/cache issues
2. Identify slow queries: `monitor.get_slow_queries()`
3. Analyze query complexity: `analyze_query_complexity(query)`
4. Review database metrics and indexes

---

## Summary

FraiseQL provides production-grade monitoring:

✅ **Prometheus Metrics**: 15+ metrics for all operational aspects
✅ **Distributed Tracing**: OpenTelemetry with OTLP/Jaeger/Zipkin
✅ **Health Checks**: Kubernetes-compatible probes
✅ **APQ Dashboard**: Real-time query caching metrics
✅ **Error Tracking**: PostgreSQL-native error grouping
✅ **Security Audit**: Comprehensive event logging
✅ **Performance Analytics**: Query complexity, slow query tracking
✅ **Alerting**: Built-in threshold configuration

Start with the minimal setup and progressively add more detailed monitoring as needed.
