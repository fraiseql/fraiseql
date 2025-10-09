# Production Monitoring

Comprehensive monitoring strategy for FraiseQL applications: metrics collection, logging, APM integration, alerting, and observability patterns.

## Overview

Production monitoring encompasses metrics, logs, traces, and alerts to ensure system health, performance, and rapid incident response.

**Key Components:**
- Prometheus metrics
- Structured logging
- APM integration (Datadog, New Relic, Sentry)
- Query performance monitoring
- Database pool monitoring
- Alerting strategies

## Table of Contents

- [Metrics Collection](#metrics-collection)
- [Logging](#logging)
- [APM Integration](#apm-integration)
- [Query Performance](#query-performance)
- [Database Monitoring](#database-monitoring)
- [Alerting](#alerting)
- [Dashboards](#dashboards)

## Metrics Collection

### Prometheus Integration

```python
from prometheus_client import Counter, Histogram, Gauge, generate_latest
from fastapi import FastAPI, Response

app = FastAPI()

# Metrics
graphql_requests_total = Counter(
    'graphql_requests_total',
    'Total GraphQL requests',
    ['operation', 'status']
)

graphql_request_duration = Histogram(
    'graphql_request_duration_seconds',
    'GraphQL request duration',
    ['operation'],
    buckets=[0.01, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0]
)

graphql_query_complexity = Histogram(
    'graphql_query_complexity',
    'GraphQL query complexity score',
    buckets=[10, 25, 50, 100, 250, 500, 1000]
)

db_pool_connections = Gauge(
    'db_pool_connections',
    'Database pool connections',
    ['state']  # active, idle
)

cache_hits = Counter('cache_hits_total', 'Cache hits')
cache_misses = Counter('cache_misses_total', 'Cache misses')

@app.get("/metrics")
async def metrics():
    """Prometheus metrics endpoint."""
    return Response(
        content=generate_latest(),
        media_type="text/plain"
    )

# Middleware to track metrics
@app.middleware("http")
async def metrics_middleware(request, call_next):
    import time

    start_time = time.time()

    response = await call_next(request)

    duration = time.time() - start_time

    # Track request duration
    if request.url.path == "/graphql":
        operation = request.headers.get("X-Operation-Name", "unknown")
        status = "success" if response.status_code < 400 else "error"

        graphql_requests_total.labels(operation=operation, status=status).inc()
        graphql_request_duration.labels(operation=operation).observe(duration)

    return response
```

### Custom Metrics

```python
from fraiseql.monitoring.metrics import MetricsCollector

class FraiseQLMetrics:
    """Custom metrics for FraiseQL operations."""

    def __init__(self):
        self.passthrough_queries = Counter(
            'fraiseql_passthrough_queries_total',
            'Queries using JSON passthrough'
        )

        self.turbo_router_hits = Counter(
            'fraiseql_turbo_router_hits_total',
            'TurboRouter cache hits'
        )

        self.apq_cache_hits = Counter(
            'fraiseql_apq_cache_hits_total',
            'APQ cache hits'
        )

        self.mutation_duration = Histogram(
            'fraiseql_mutation_duration_seconds',
            'Mutation execution time',
            ['mutation_name']
        )

    def track_query_execution(self, mode: str, duration: float, complexity: int):
        """Track query execution metrics."""
        if mode == "passthrough":
            self.passthrough_queries.inc()

        graphql_request_duration.labels(operation=mode).observe(duration)
        graphql_query_complexity.observe(complexity)

metrics = FraiseQLMetrics()
```

## Logging

### Structured Logging

```python
import logging
import json
from datetime import datetime

class StructuredFormatter(logging.Formatter):
    """JSON structured logging formatter."""

    def format(self, record):
        log_data = {
            "timestamp": datetime.utcnow().isoformat(),
            "level": record.levelname,
            "logger": record.name,
            "message": record.getMessage(),
            "module": record.module,
            "function": record.funcName,
            "line": record.lineno,
        }

        # Add extra fields
        if hasattr(record, "user_id"):
            log_data["user_id"] = record.user_id
        if hasattr(record, "query_id"):
            log_data["query_id"] = record.query_id
        if hasattr(record, "duration"):
            log_data["duration_ms"] = record.duration

        # Add exception info
        if record.exc_info:
            log_data["exception"] = self.formatException(record.exc_info)

        return json.dumps(log_data)

# Configure logging
logging.basicConfig(
    level=logging.INFO,
    handlers=[
        logging.StreamHandler()
    ]
)

# Set formatter
for handler in logging.root.handlers:
    handler.setFormatter(StructuredFormatter())

logger = logging.getLogger(__name__)

# Usage
logger.info(
    "GraphQL query executed",
    extra={
        "user_id": "user-123",
        "query_id": "query-456",
        "duration": 125.5,
        "complexity": 45
    }
)
```

### Request Logging Middleware

```python
from fastapi import Request
from starlette.middleware.base import BaseHTTPMiddleware
import time
import uuid

class RequestLoggingMiddleware(BaseHTTPMiddleware):
    async def dispatch(self, request: Request, call_next):
        request_id = str(uuid.uuid4())
        request.state.request_id = request_id

        # Log request
        logger.info(
            "Request started",
            extra={
                "request_id": request_id,
                "method": request.method,
                "path": request.url.path,
                "client_ip": request.client.host if request.client else None,
                "user_agent": request.headers.get("user-agent")
            }
        )

        start_time = time.time()

        try:
            response = await call_next(request)

            duration = (time.time() - start_time) * 1000

            # Log response
            logger.info(
                "Request completed",
                extra={
                    "request_id": request_id,
                    "status_code": response.status_code,
                    "duration_ms": duration
                }
            )

            # Add request ID to response headers
            response.headers["X-Request-ID"] = request_id

            return response

        except Exception as e:
            duration = (time.time() - start_time) * 1000

            logger.error(
                "Request failed",
                extra={
                    "request_id": request_id,
                    "duration_ms": duration,
                    "error": str(e)
                },
                exc_info=True
            )
            raise

app.add_middleware(RequestLoggingMiddleware)
```

## APM Integration

### Sentry Integration

```python
from fraiseql.monitoring.sentry import init_sentry, set_user, set_context

# Initialize Sentry
init_sentry(
    dsn=os.getenv("SENTRY_DSN"),
    environment="production",
    traces_sample_rate=0.1,  # 10% of traces
    profiles_sample_rate=0.1,
    release=f"fraiseql@{VERSION}"
)

# In GraphQL context
@app.middleware("http")
async def sentry_middleware(request: Request, call_next):
    # Set user context
    if hasattr(request.state, "user"):
        user = request.state.user
        set_user(
            user_id=user.user_id,
            email=user.email,
            username=user.name
        )

    # Set GraphQL context
    if request.url.path == "/graphql":
        query = await request.body()
        set_context("graphql", {
            "query": query.decode()[:1000],  # Limit size
            "operation": request.headers.get("X-Operation-Name")
        })

    response = await call_next(request)
    return response
```

### Datadog Integration

```python
from ddtrace import tracer, patch_all
from ddtrace.contrib.fastapi import patch as patch_fastapi

# Patch all supported libraries
patch_all()

# FastAPI tracing
patch_fastapi(app)

# Custom span
@query
async def get_user(info, id: str) -> User:
    with tracer.trace("get_user", service="fraiseql") as span:
        span.set_tag("user.id", id)
        span.set_tag("operation", "query")

        user = await fetch_user(id)

        span.set_tag("user.found", user is not None)

        return user
```

## Query Performance

### Query Timing

```python
from fraiseql.monitoring.metrics import query_duration_histogram

@app.middleware("http")
async def query_timing_middleware(request: Request, call_next):
    if request.url.path != "/graphql":
        return await call_next(request)

    import time
    start_time = time.time()

    # Parse query
    body = await request.json()
    query = body.get("query", "")
    operation_name = body.get("operationName", "unknown")

    response = await call_next(request)

    duration = time.time() - start_time

    # Track timing
    query_duration_histogram.labels(
        operation=operation_name
    ).observe(duration)

    # Log slow queries
    if duration > 1.0:  # Slower than 1 second
        logger.warning(
            "Slow query detected",
            extra={
                "operation": operation_name,
                "duration_ms": duration * 1000,
                "query": query[:500]
            }
        )

    return response
```

### Complexity Tracking

```python
from fraiseql.analysis.complexity import analyze_query_complexity

async def track_query_complexity(query: str, operation_name: str):
    """Track query complexity metrics."""
    complexity = analyze_query_complexity(query)

    graphql_query_complexity.observe(complexity.score)

    if complexity.score > 500:
        logger.warning(
            "High complexity query",
            extra={
                "operation": operation_name,
                "complexity": complexity.score,
                "depth": complexity.depth,
                "fields": complexity.field_count
            }
        )
```

## Database Monitoring

### Connection Pool Metrics

```python
from fraiseql.db import get_db_pool

async def collect_pool_metrics():
    """Collect database pool metrics."""
    pool = get_db_pool()
    stats = pool.get_stats()

    # Update Prometheus gauges
    db_pool_connections.labels(state="active").set(
        stats["pool_size"] - stats["pool_available"]
    )
    db_pool_connections.labels(state="idle").set(
        stats["pool_available"]
    )

    # Log if pool is saturated
    utilization = (stats["pool_size"] / pool.max_size) * 100
    if utilization > 90:
        logger.warning(
            "Database pool highly utilized",
            extra={
                "pool_size": stats["pool_size"],
                "max_size": pool.max_size,
                "utilization_pct": utilization
            }
        )

# Collect metrics periodically
import asyncio

async def metrics_collector():
    while True:
        await collect_pool_metrics()
        await asyncio.sleep(15)  # Every 15 seconds

asyncio.create_task(metrics_collector())
```

### Query Logging

```python
# Log all SQL queries in development
from fraiseql.fastapi.config import FraiseQLConfig

config = FraiseQLConfig(
    database_url="postgresql://...",
    database_echo=True  # Development only
)

# Production: Log slow queries only
# PostgreSQL: log_min_duration_statement = 1000  # Log queries > 1s
```

## Alerting

### Prometheus Alerts

```yaml
# prometheus-alerts.yml
groups:
  - name: fraiseql
    interval: 30s
    rules:
      # High error rate
      - alert: HighErrorRate
        expr: rate(graphql_requests_total{status="error"}[5m]) > 0.05
        for: 2m
        labels:
          severity: warning
        annotations:
          summary: "High GraphQL error rate"
          description: "Error rate is {{ $value }} errors/sec"

      # High latency
      - alert: HighLatency
        expr: histogram_quantile(0.99, rate(graphql_request_duration_seconds_bucket[5m])) > 1.0
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "High GraphQL latency"
          description: "P99 latency is {{ $value }}s"

      # Database pool saturation
      - alert: DatabasePoolSaturated
        expr: db_pool_connections{state="active"} / db_pool_max_connections > 0.9
        for: 2m
        labels:
          severity: critical
        annotations:
          summary: "Database pool saturated"
          description: "Pool utilization is {{ $value }}%"

      # Low cache hit rate
      - alert: LowCacheHitRate
        expr: rate(cache_hits_total[5m]) / (rate(cache_hits_total[5m]) + rate(cache_misses_total[5m])) < 0.5
        for: 10m
        labels:
          severity: info
        annotations:
          summary: "Low cache hit rate"
          description: "Cache hit rate is {{ $value }}"
```

### PagerDuty Integration

```python
import httpx

async def send_pagerduty_alert(
    summary: str,
    severity: str,
    details: dict
):
    """Send alert to PagerDuty."""
    payload = {
        "routing_key": os.getenv("PAGERDUTY_ROUTING_KEY"),
        "event_action": "trigger",
        "payload": {
            "summary": summary,
            "severity": severity,
            "source": "fraiseql",
            "custom_details": details
        }
    }

    async with httpx.AsyncClient() as client:
        await client.post(
            "https://events.pagerduty.com/v2/enqueue",
            json=payload
        )

# Example usage
if error_rate > 0.1:
    await send_pagerduty_alert(
        summary="High GraphQL error rate detected",
        severity="error",
        details={
            "error_rate": error_rate,
            "time_window": "5m",
            "affected_operations": ["getUser", "getOrders"]
        }
    )
```

## Dashboards

### Grafana Dashboard

```json
{
  "dashboard": {
    "title": "FraiseQL Production Metrics",
    "panels": [
      {
        "title": "Request Rate",
        "targets": [
          {
            "expr": "rate(graphql_requests_total[5m])",
            "legendFormat": "{{operation}}"
          }
        ]
      },
      {
        "title": "Latency (P50, P95, P99)",
        "targets": [
          {
            "expr": "histogram_quantile(0.50, rate(graphql_request_duration_seconds_bucket[5m]))",
            "legendFormat": "P50"
          },
          {
            "expr": "histogram_quantile(0.95, rate(graphql_request_duration_seconds_bucket[5m]))",
            "legendFormat": "P95"
          },
          {
            "expr": "histogram_quantile(0.99, rate(graphql_request_duration_seconds_bucket[5m]))",
            "legendFormat": "P99"
          }
        ]
      },
      {
        "title": "Error Rate",
        "targets": [
          {
            "expr": "rate(graphql_requests_total{status=\"error\"}[5m])",
            "legendFormat": "Errors/sec"
          }
        ]
      },
      {
        "title": "Database Pool",
        "targets": [
          {
            "expr": "db_pool_connections{state=\"active\"}",
            "legendFormat": "Active"
          },
          {
            "expr": "db_pool_connections{state=\"idle\"}",
            "legendFormat": "Idle"
          }
        ]
      }
    ]
  }
}
```

## Next Steps

- [Deployment](deployment.md) - Production deployment patterns
- [Security](security.md) - Security monitoring
- [Performance](../core/performance.md) - Performance optimization
- [Health Checks](../api-reference/health.md) - Health monitoring patterns
