# Production Monitoring & Observability

Complete guide for monitoring FraiseQL in production with metrics, logging, and distributed tracing.

## Table of Contents
- [Overview](#overview)
- [Metrics with Prometheus](#metrics-with-prometheus)
- [Visualization with Grafana](#visualization-with-grafana)
- [Distributed Tracing](#distributed-tracing)
- [Logging](#logging)
- [Alerting](#alerting)
- [SLO/SLI Monitoring](#slosli-monitoring)
- [Dashboard Examples](#dashboard-examples)

## Overview

FraiseQL provides comprehensive observability through:
- **Metrics**: Prometheus-compatible metrics endpoint
- **Tracing**: OpenTelemetry integration
- **Logging**: Structured JSON logging
- **Health Checks**: Liveness and readiness probes

### Architecture

```
┌─────────────┐     ┌────────────┐     ┌─────────────┐
│   FraiseQL  │────▶│ Prometheus │────▶│   Grafana   │
│  /metrics   │     └────────────┘     └─────────────┘
└─────────────┘            │                    ▲
       │                   ▼                    │
       │            ┌────────────┐              │
       └───────────▶│   Jaeger   │              │
         traces     └────────────┘              │
                           │                    │
                           └────────────────────┘
```

## Metrics with Prometheus

### 1. Configure FraiseQL Metrics

```python
# app.py
from fraiseql import create_fraiseql_app
from fraiseql.monitoring import setup_metrics, MetricsConfig

# Configure metrics
metrics_config = MetricsConfig(
    enabled=True,
    namespace="fraiseql_prod",
    labels={
        "environment": "production",
        "region": "us-east-1",
        "service": "api"
    }
)

app = create_fraiseql_app(
    database_url=DATABASE_URL,
    production=True
)

# Setup metrics
metrics = setup_metrics(app, metrics_config)
```

### 2. Prometheus Configuration

```yaml
# prometheus.yml
global:
  scrape_interval: 15s
  evaluation_interval: 15s
  external_labels:
    cluster: 'production'
    region: 'us-east-1'

scrape_configs:
  - job_name: 'fraiseql'
    kubernetes_sd_configs:
    - role: pod
      namespaces:
        names:
        - fraiseql
    relabel_configs:
    - source_labels: [__meta_kubernetes_pod_annotation_prometheus_io_scrape]
      action: keep
      regex: true
    - source_labels: [__meta_kubernetes_pod_annotation_prometheus_io_path]
      action: replace
      target_label: __metrics_path__
      regex: (.+)
    - source_labels: [__address__, __meta_kubernetes_pod_annotation_prometheus_io_port]
      action: replace
      regex: ([^:]+)(?::\d+)?;(\d+)
      replacement: $1:$2
      target_label: __address__
    - action: labelmap
      regex: __meta_kubernetes_pod_label_(.+)

  - job_name: 'postgresql'
    static_configs:
    - targets: ['postgres-exporter:9187']
```

### 3. Key Metrics to Monitor

| Metric | Description | Alert Threshold |
|--------|-------------|-----------------|
| `fraiseql_graphql_queries_total` | Total queries | - |
| `fraiseql_graphql_query_duration_seconds` | Query latency | P95 > 1s |
| `fraiseql_graphql_queries_errors` | Failed queries | Rate > 1% |
| `fraiseql_db_connections_active` | Active DB connections | > 80% of pool |
| `fraiseql_cache_hit_rate` | Cache effectiveness | < 70% |
| `fraiseql_http_requests_total` | HTTP requests | - |
| `fraiseql_response_time_seconds` | Overall response time | P99 > 2s |

## Visualization with Grafana

### 1. Import FraiseQL Dashboard

```json
{
  "dashboard": {
    "title": "FraiseQL Production Metrics",
    "panels": [
      {
        "title": "Request Rate",
        "gridPos": {"h": 8, "w": 12, "x": 0, "y": 0},
        "targets": [{
          "expr": "sum(rate(fraiseql_graphql_queries_total[5m])) by (operation_type)",
          "legendFormat": "{{operation_type}}"
        }]
      },
      {
        "title": "Error Rate %",
        "gridPos": {"h": 8, "w": 12, "x": 12, "y": 0},
        "targets": [{
          "expr": "100 * sum(rate(fraiseql_graphql_queries_errors[5m])) / sum(rate(fraiseql_graphql_queries_total[5m]))",
          "legendFormat": "Error Rate"
        }]
      },
      {
        "title": "Response Time (P50, P95, P99)",
        "gridPos": {"h": 8, "w": 24, "x": 0, "y": 8},
        "targets": [
          {
            "expr": "histogram_quantile(0.5, sum(rate(fraiseql_response_time_seconds_bucket[5m])) by (le))",
            "legendFormat": "P50"
          },
          {
            "expr": "histogram_quantile(0.95, sum(rate(fraiseql_response_time_seconds_bucket[5m])) by (le))",
            "legendFormat": "P95"
          },
          {
            "expr": "histogram_quantile(0.99, sum(rate(fraiseql_response_time_seconds_bucket[5m])) by (le))",
            "legendFormat": "P99"
          }
        ]
      },
      {
        "title": "Database Connections",
        "gridPos": {"h": 8, "w": 12, "x": 0, "y": 16},
        "targets": [
          {
            "expr": "fraiseql_db_connections_active",
            "legendFormat": "Active"
          },
          {
            "expr": "fraiseql_db_connections_idle",
            "legendFormat": "Idle"
          },
          {
            "expr": "fraiseql_db_connections_total",
            "legendFormat": "Total"
          }
        ]
      },
      {
        "title": "Cache Performance",
        "gridPos": {"h": 8, "w": 12, "x": 12, "y": 16},
        "targets": [
          {
            "expr": "rate(fraiseql_cache_hits_total[5m])",
            "legendFormat": "Hits"
          },
          {
            "expr": "rate(fraiseql_cache_misses_total[5m])",
            "legendFormat": "Misses"
          }
        ]
      }
    ]
  }
}
```

### 2. Query-Specific Dashboards

```json
{
  "title": "GraphQL Query Performance",
  "panels": [
    {
      "title": "Top 10 Slowest Queries",
      "targets": [{
        "expr": "topk(10, avg by (operation_name) (rate(fraiseql_graphql_query_duration_seconds_sum[5m]) / rate(fraiseql_graphql_query_duration_seconds_count[5m])))",
        "format": "table"
      }]
    },
    {
      "title": "Query Frequency",
      "targets": [{
        "expr": "topk(10, sum by (operation_name) (rate(fraiseql_graphql_queries_total[5m])))",
        "format": "table"
      }]
    }
  ]
}
```

## Distributed Tracing

### 1. Configure OpenTelemetry

```python
# app.py
from fraiseql.tracing import setup_tracing, TracingConfig

# Configure tracing
tracing_config = TracingConfig(
    service_name="fraiseql-api",
    service_version="1.0.0",
    deployment_environment="production",
    sample_rate=0.1,  # 10% sampling
    export_endpoint="http://jaeger-collector:4317",
    export_format="otlp"
)

# Setup tracing
tracer = setup_tracing(app, tracing_config)
```

### 2. Jaeger Setup

```yaml
# jaeger-production.yaml
apiVersion: v1
kind: ConfigMap
metadata:
  name: jaeger-config
data:
  sampling.json: |
    {
      "service_strategies": [
        {
          "service": "fraiseql-api",
          "type": "adaptive",
          "max_traces_per_second": 100
        }
      ]
    }
---
apiVersion: apps/v1
kind: Deployment
metadata:
  name: jaeger
spec:
  template:
    spec:
      containers:
      - name: jaeger-collector
        image: jaegertracing/jaeger-collector:1.45
        env:
        - name: SPAN_STORAGE_TYPE
          value: elasticsearch
        - name: ES_SERVER_URLS
          value: http://elasticsearch:9200
        ports:
        - containerPort: 4317  # OTLP gRPC
        - containerPort: 4318  # OTLP HTTP
```

### 3. Trace Analysis Queries

```python
# Custom trace attributes
@trace_graphql_operation("query", "getUser")
async def get_user(user_id: int):
    span = trace.get_current_span()
    span.set_attribute("user.id", user_id)
    span.set_attribute("user.type", "premium")

    # Your logic here
    return user
```

## Logging

### 1. Structured Logging Configuration

```python
# logging_config.py
import structlog
from pythonjsonlogger import jsonlogger

# Configure structured logging
structlog.configure(
    processors=[
        structlog.stdlib.filter_by_level,
        structlog.stdlib.add_logger_name,
        structlog.stdlib.add_log_level,
        structlog.stdlib.PositionalArgumentsFormatter(),
        structlog.processors.TimeStamper(fmt="iso"),
        structlog.processors.StackInfoRenderer(),
        structlog.processors.format_exc_info,
        structlog.processors.UnicodeDecoder(),
        structlog.processors.JSONRenderer()
    ],
    context_class=dict,
    logger_factory=structlog.stdlib.LoggerFactory(),
    cache_logger_on_first_use=True,
)

# Configure Python logging
logging.basicConfig(
    format='%(message)s',
    stream=sys.stdout,
    level=logging.INFO,
)

# Add JSON formatter
logHandler = logging.StreamHandler()
formatter = jsonlogger.JsonFormatter()
logHandler.setFormatter(formatter)
logging.root.handlers = [logHandler]
```

### 2. Application Logging

```python
import structlog
logger = structlog.get_logger()

# Log with context
logger.info(
    "graphql_query_executed",
    operation_name="getUser",
    operation_type="query",
    duration_ms=45.3,
    user_id=context.user.id,
    trace_id=span.get_span_context().trace_id
)

# Log errors with full context
try:
    result = await execute_query(query)
except Exception as e:
    logger.error(
        "query_execution_failed",
        operation_name=operation_name,
        error=str(e),
        error_type=type(e).__name__,
        exc_info=True
    )
    raise
```

### 3. Log Aggregation with ELK

```yaml
# filebeat.yml
filebeat.inputs:
- type: container
  paths:
    - /var/log/containers/*fraiseql*.log
  processors:
    - decode_json_fields:
        fields: ["message"]
        target: ""
        overwrite_keys: true
    - add_kubernetes_metadata:
        host: ${NODE_NAME}
        matchers:
        - logs_path:
            logs_path: "/var/log/containers/"

output.elasticsearch:
  hosts: ['${ELASTICSEARCH_HOST:elasticsearch}:${ELASTICSEARCH_PORT:9200}']
  index: "fraiseql-%{[agent.version]}-%{+yyyy.MM.dd}"
```

## Alerting

### 1. Prometheus Alert Rules

```yaml
# alerts.yml
groups:
- name: fraiseql
  interval: 30s
  rules:
  - alert: HighErrorRate
    expr: |
      100 * sum(rate(fraiseql_graphql_queries_errors[5m]))
      / sum(rate(fraiseql_graphql_queries_total[5m])) > 1
    for: 5m
    labels:
      severity: critical
      team: backend
    annotations:
      summary: "High error rate detected"
      description: "Error rate is {{ $value }}% for the last 5 minutes"
      runbook_url: "https://wiki.example.com/runbooks/fraiseql-errors"

  - alert: HighResponseTime
    expr: |
      histogram_quantile(0.95, sum(rate(fraiseql_response_time_seconds_bucket[5m])) by (le)) > 1
    for: 10m
    labels:
      severity: warning
      team: backend
    annotations:
      summary: "High response time"
      description: "95th percentile response time is {{ $value }}s"

  - alert: DatabaseConnectionPoolExhausted
    expr: |
      fraiseql_db_connections_active / fraiseql_db_connections_total > 0.9
    for: 5m
    labels:
      severity: critical
      team: backend
    annotations:
      summary: "Database connection pool almost exhausted"
      description: "{{ $value | humanizePercentage }} of connections are active"

  - alert: CacheHitRateLow
    expr: |
      sum(rate(fraiseql_cache_hits_total[5m]))
      / (sum(rate(fraiseql_cache_hits_total[5m])) + sum(rate(fraiseql_cache_misses_total[5m]))) < 0.7
    for: 15m
    labels:
      severity: warning
      team: backend
    annotations:
      summary: "Cache hit rate is low"
      description: "Cache hit rate is {{ $value | humanizePercentage }}"
```

### 2. PagerDuty Integration

```yaml
# alertmanager.yml
global:
  resolve_timeout: 5m
  pagerduty_url: 'https://events.pagerduty.com/v2/enqueue'

route:
  group_by: ['alertname', 'cluster', 'service']
  group_wait: 10s
  group_interval: 10s
  repeat_interval: 12h
  receiver: 'pagerduty-critical'
  routes:
  - match:
      severity: critical
    receiver: pagerduty-critical
  - match:
      severity: warning
    receiver: slack-warnings

receivers:
- name: 'pagerduty-critical'
  pagerduty_configs:
  - service_key: '<YOUR-SERVICE-KEY>'
    description: '{{ range .Alerts }}{{ .Annotations.summary }}{{ end }}'
    details:
      firing: '{{ .Alerts.Firing | len }}'
      resolved: '{{ .Alerts.Resolved | len }}'
      alertname: '{{ .GroupLabels.alertname }}'

- name: 'slack-warnings'
  slack_configs:
  - api_url: '<YOUR-SLACK-WEBHOOK>'
    channel: '#alerts'
    title: 'FraiseQL Warning'
    text: '{{ range .Alerts }}{{ .Annotations.summary }}{{ end }}'
```

## SLO/SLI Monitoring

### 1. Define SLIs

```yaml
# sli-queries.yml
slis:
  - name: availability
    query: |
      sum(rate(fraiseql_http_requests_total{status!~"5.."}[5m]))
      / sum(rate(fraiseql_http_requests_total[5m]))

  - name: latency
    query: |
      histogram_quantile(0.95,
        sum(rate(fraiseql_response_time_seconds_bucket{status!~"5.."}[5m])) by (le)
      ) < 1

  - name: error_rate
    query: |
      1 - (
        sum(rate(fraiseql_graphql_queries_errors[5m]))
        / sum(rate(fraiseql_graphql_queries_total[5m]))
      )
```

### 2. SLO Dashboard

```json
{
  "title": "FraiseQL SLO Dashboard",
  "panels": [
    {
      "title": "30-Day Availability SLO (99.9%)",
      "targets": [{
        "expr": "avg_over_time((sum(rate(fraiseql_http_requests_total{status!~\"5..\"}[5m])) / sum(rate(fraiseql_http_requests_total[5m])))[30d:5m])"
      }],
      "thresholds": [
        {"value": 0.999, "color": "green"},
        {"value": 0.995, "color": "yellow"},
        {"value": 0, "color": "red"}
      ]
    },
    {
      "title": "Error Budget Remaining",
      "targets": [{
        "expr": "(1 - 0.999) - (1 - avg_over_time((sum(rate(fraiseql_http_requests_total{status!~\"5..\"}[5m])) / sum(rate(fraiseql_http_requests_total[5m])))[30d:5m]))"
      }]
    }
  ]
}
```

## Dashboard Examples

### 1. Executive Dashboard

```json
{
  "title": "FraiseQL Executive Dashboard",
  "panels": [
    {
      "title": "Monthly Active Queries",
      "type": "stat",
      "targets": [{
        "expr": "sum(increase(fraiseql_graphql_queries_total[30d]))"
      }]
    },
    {
      "title": "Average Response Time",
      "type": "gauge",
      "targets": [{
        "expr": "avg(rate(fraiseql_response_time_seconds_sum[5m]) / rate(fraiseql_response_time_seconds_count[5m]))"
      }]
    },
    {
      "title": "Uptime %",
      "type": "stat",
      "targets": [{
        "expr": "avg_over_time(up{job=\"fraiseql\"}[30d]) * 100"
      }]
    }
  ]
}
```

### 2. Debugging Dashboard

```json
{
  "title": "FraiseQL Debug Dashboard",
  "panels": [
    {
      "title": "Slow Queries (> 1s)",
      "targets": [{
        "expr": "fraiseql_graphql_query_duration_seconds{quantile=\"0.95\"} > 1",
        "format": "table"
      }]
    },
    {
      "title": "Failed Queries by Error",
      "targets": [{
        "expr": "sum by (error_type) (rate(fraiseql_errors_total[5m]))",
        "format": "table"
      }]
    },
    {
      "title": "N+1 Query Detection",
      "targets": [{
        "expr": "increase(fraiseql_n1_queries_detected_total[1h])",
        "format": "table"
      }]
    }
  ]
}
```

## Best Practices

1. **Sample appropriately** - 10% tracing in production
2. **Use structured logging** - JSON format for parsing
3. **Set meaningful SLOs** - Based on user expectations
4. **Alert on symptoms, not causes** - User-facing impact
5. **Dashboard hierarchy** - Executive → Team → Debug
6. **Correlate metrics and traces** - Use trace IDs in logs
7. **Regular review** - Monthly SLO and alert reviews
8. **Capacity planning** - Use metrics for forecasting

## Next Steps

- [Docker Deployment](./docker.md) - Container deployment guide
- [Kubernetes Deployment](./kubernetes.md) - Container orchestration
