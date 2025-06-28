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

### 2. Advanced Alert Rules

```yaml
# fraiseql-alerts.yml
groups:
- name: fraiseql-business-critical
  interval: 30s
  rules:
  - alert: FraiseQLServiceDown
    expr: up{job="fraiseql"} == 0
    for: 1m
    labels:
      severity: critical
      team: sre
      service: fraiseql
      runbook: service-down
    annotations:
      summary: "FraiseQL service is down"
      description: "FraiseQL service has been down for more than 1 minute. Instance: {{ $labels.instance }}"
      impact: "All GraphQL API traffic is failing"
      action: "Investigate service health, check logs and container status"

  - alert: DatabaseConnectionsExhausted
    expr: |
      (fraiseql_db_connections_active / fraiseql_db_connections_total) > 0.95
    for: 2m
    labels:
      severity: critical
      team: backend
      component: database
    annotations:
      summary: "Database connection pool near exhaustion"
      description: "{{ $value | humanizePercentage }} of database connections are in use"
      impact: "New requests may be rejected due to no available connections"
      action: "Scale database pool or investigate connection leaks"

  - alert: MemoryLeakDetected
    expr: |
      increase(process_resident_memory_bytes{job="fraiseql"}[30m]) > 100000000  # 100MB increase
    for: 30m
    labels:
      severity: warning
      team: backend
      component: memory
    annotations:
      summary: "Potential memory leak detected"
      description: "Memory usage increased by {{ $value | humanizeBytes }} in 30 minutes"
      impact: "Service may become unstable or crash due to OOM"
      action: "Review recent deployments, check for memory leaks in application code"

- name: fraiseql-performance
  interval: 1m
  rules:
  - alert: HighQueryLatency
    expr: |
      histogram_quantile(0.99, 
        sum(rate(fraiseql_graphql_query_duration_seconds_bucket[5m])) by (le, operation_name)
      ) > 5
    for: 5m
    labels:
      severity: warning
      team: backend
      component: graphql
    annotations:
      summary: "High GraphQL query latency detected"
      description: "99th percentile latency is {{ $value }}s for operation {{ $labels.operation_name }}"
      impact: "Poor user experience due to slow API responses"
      action: "Review query complexity, check database performance, analyze traces"

  - alert: N1QueriesDetected
    expr: |
      increase(fraiseql_n1_queries_detected_total[15m]) > 10
    for: 0m
    labels:
      severity: warning
      team: backend
      component: optimization
    annotations:
      summary: "N+1 queries detected"
      description: "{{ $value }} N+1 query patterns detected in the last 15 minutes"
      impact: "Degraded performance and increased database load"
      action: "Review GraphQL resolver implementation, optimize data loading patterns"

  - alert: LowCacheEfficiency
    expr: |
      (
        sum(rate(fraiseql_cache_hits_total[10m])) /
        (sum(rate(fraiseql_cache_hits_total[10m])) + sum(rate(fraiseql_cache_misses_total[10m])))
      ) < 0.6
    for: 10m
    labels:
      severity: warning
      team: backend
      component: cache
    annotations:
      summary: "Cache hit rate is low"
      description: "Cache hit rate is {{ $value | humanizePercentage }} (below 60%)"
      impact: "Increased database load and slower response times"
      action: "Review cache configuration, TTL settings, and cache key strategies"

- name: fraiseql-security
  interval: 1m
  rules:
  - alert: HighAuthenticationFailures
    expr: |
      sum(rate(fraiseql_auth_failures_total[5m])) > 10
    for: 2m
    labels:
      severity: warning
      team: security
      component: authentication
    annotations:
      summary: "High authentication failure rate"
      description: "{{ $value }} authentication failures per second"
      impact: "Potential brute force attack or misconfigured clients"
      action: "Review authentication logs, check for suspicious IPs, validate client configurations"

  - alert: UnauthorizedAccessAttempts
    expr: |
      sum(rate(fraiseql_graphql_queries_errors{error_type="authorization"}[5m])) > 5
    for: 5m
    labels:
      severity: warning
      team: security
      component: authorization
    annotations:
      summary: "High authorization failure rate"
      description: "{{ $value }} authorization failures per second"
      impact: "Users attempting to access unauthorized resources"
      action: "Review authorization policies, check for privilege escalation attempts"

  - alert: RateLimitingTriggered
    expr: |
      sum(rate(fraiseql_rate_limit_exceeded_total[5m])) > 1
    for: 1m
    labels:
      severity: info
      team: backend
      component: rate-limiting
    annotations:
      summary: "Rate limiting frequently triggered"
      description: "{{ $value }} rate limit violations per second"
      impact: "Some requests are being throttled"
      action: "Review rate limit thresholds and client request patterns"

- name: fraiseql-business-metrics
  interval: 5m
  rules:
  - alert: LowUserEngagement
    expr: |
      sum(rate(fraiseql_graphql_queries_total[1h])) < 100
    for: 30m
    labels:
      severity: info
      team: product
      component: engagement
    annotations:
      summary: "Low user engagement detected"
      description: "Only {{ $value }} queries per second in the last hour"
      impact: "Lower than expected API usage"
      action: "Check for service disruptions, review user communications"

  - alert: UnusualQueryPattern
    expr: |
      abs(
        sum(rate(fraiseql_graphql_queries_total[1h])) -
        avg_over_time(sum(rate(fraiseql_graphql_queries_total[1h]))[7d:1h])
      ) > (0.5 * avg_over_time(sum(rate(fraiseql_graphql_queries_total[1h]))[7d:1h]))
    for: 15m
    labels:
      severity: info
      team: sre
      component: anomaly-detection
    annotations:
      summary: "Unusual query traffic pattern"
      description: "Current traffic deviates by {{ $value }}% from 7-day average"
      impact: "Potential traffic spike or drop requiring investigation"
      action: "Investigate traffic source, check for marketing campaigns or outages"
```

### 3. PagerDuty Integration

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
  receiver: 'default'
  routes:
  # Critical alerts go to PagerDuty immediately
  - match:
      severity: critical
    receiver: pagerduty-critical
    group_wait: 0s
    repeat_interval: 5m
  
  # Security alerts get special handling
  - match:
      team: security
    receiver: security-team
    group_wait: 30s
    
  # Performance warnings to Slack during business hours
  - match:
      severity: warning
      component: graphql
    receiver: slack-performance
    active_time_intervals:
    - business-hours
    
  # Low priority info alerts
  - match:
      severity: info
    receiver: slack-info
    group_wait: 5m
    repeat_interval: 4h

time_intervals:
- name: business-hours
  time_intervals:
  - times:
    - start_time: '09:00'
      end_time: '17:00'
    weekdays: ['monday:friday']
    location: 'America/New_York'

receivers:
- name: 'default'
  slack_configs:
  - api_url: '<SLACK-WEBHOOK-URL>'
    channel: '#alerts-default'

- name: 'pagerduty-critical'
  pagerduty_configs:
  - service_key: '<FRAISEQL-SERVICE-KEY>'
    severity: '{{ .GroupLabels.severity }}'
    client: 'FraiseQL Monitoring'
    client_url: 'https://grafana.company.com/dashboards/fraiseql'
    description: '{{ range .Alerts }}{{ .Annotations.summary }}{{ end }}'
    details:
      firing: '{{ .Alerts.Firing | len }}'
      resolved: '{{ .Alerts.Resolved | len }}'
      alertname: '{{ .GroupLabels.alertname }}'
      service: '{{ .GroupLabels.service }}'
      impact: '{{ range .Alerts }}{{ .Annotations.impact }}{{ end }}'
      action: '{{ range .Alerts }}{{ .Annotations.action }}{{ end }}'
      runbook: 'https://runbooks.company.com/fraiseql/{{ .GroupLabels.runbook }}'

- name: 'security-team'
  slack_configs:
  - api_url: '<SECURITY-SLACK-WEBHOOK>'
    channel: '#security-alerts'
    title: '🚨 Security Alert: {{ .GroupLabels.alertname }}'
    text: |
      *Impact:* {{ range .Alerts }}{{ .Annotations.impact }}{{ end }}
      *Action Required:* {{ range .Alerts }}{{ .Annotations.action }}{{ end }}
      *Details:* {{ range .Alerts }}{{ .Annotations.description }}{{ end }}
    actions:
    - type: button
      text: 'View Logs'
      url: 'https://kibana.company.com/app/kibana#/discover?_g=(time:(from:now-1h,to:now))'
    - type: button
      text: 'Incident Response'
      url: 'https://incident.company.com/new?service=fraiseql&severity={{ .GroupLabels.severity }}'

- name: 'slack-performance'
  slack_configs:
  - api_url: '<PERFORMANCE-SLACK-WEBHOOK>'
    channel: '#fraiseql-performance'
    title: '⚠️ Performance Issue: {{ .GroupLabels.alertname }}'
    text: |
      *Service:* {{ .GroupLabels.service }}
      *Component:* {{ .GroupLabels.component }}
      *Description:* {{ range .Alerts }}{{ .Annotations.description }}{{ end }}
      *Recommended Action:* {{ range .Alerts }}{{ .Annotations.action }}{{ end }}
    actions:
    - type: button
      text: 'View Dashboard'
      url: 'https://grafana.company.com/d/fraiseql-performance'
    - type: button
      text: 'Check Traces'
      url: 'https://jaeger.company.com/search?service=fraiseql-api&start={{ .StartsAt.Unix }}000000'

- name: 'slack-info'
  slack_configs:
  - api_url: '<INFO-SLACK-WEBHOOK>'
    channel: '#fraiseql-info'
    title: 'ℹ️ {{ .GroupLabels.alertname }}'

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

### 4. Runbook Integration

Create runbooks for each critical alert:

```yaml
# runbooks.yml - Alert to runbook mapping
alerts:
  FraiseQLServiceDown:
    runbook_url: "https://runbooks.company.com/fraiseql/service-down"
    severity: critical
    estimated_resolution_time: "15 minutes"
    escalation_path: "SRE → Engineering Manager → CTO"
    
  DatabaseConnectionsExhausted:
    runbook_url: "https://runbooks.company.com/fraiseql/db-connections"
    severity: critical
    estimated_resolution_time: "10 minutes"
    escalation_path: "Backend Team → Database Team"
    
  HighQueryLatency:
    runbook_url: "https://runbooks.company.com/fraiseql/performance"
    severity: warning
    estimated_resolution_time: "30 minutes"
    escalation_path: "Backend Team → Performance Team"
```

## Advanced Monitoring Features

### 1. Custom Metrics Collection

```python
# custom_metrics.py
from fraiseql.monitoring import MetricsCollector, Counter, Histogram, Gauge
from prometheus_client import CollectorRegistry

class FraiseQLCustomMetrics:
    """Custom business and application metrics for FraiseQL."""
    
    def __init__(self, registry: CollectorRegistry):
        self.registry = registry
        
        # Business metrics
        self.user_queries = Counter(
            'fraiseql_user_queries_total',
            'Total queries by user',
            ['user_id', 'user_type', 'subscription_tier'],
            registry=registry
        )
        
        self.query_complexity = Histogram(
            'fraiseql_query_complexity_score',
            'GraphQL query complexity scores',
            ['operation_name', 'user_type'],
            buckets=[1, 5, 10, 25, 50, 100, 200],
            registry=registry
        )
        
        self.active_subscriptions = Gauge(
            'fraiseql_active_subscriptions',
            'Number of active GraphQL subscriptions',
            ['subscription_type'],
            registry=registry
        )
        
        # Application-specific metrics
        self.schema_validation_errors = Counter(
            'fraiseql_schema_validation_errors_total',
            'Schema validation errors',
            ['error_type', 'field_path'],
            registry=registry
        )
        
        self.feature_usage = Counter(
            'fraiseql_feature_usage_total',
            'Feature usage tracking',
            ['feature_name', 'user_segment'],
            registry=registry
        )

    def record_user_query(self, user_id: str, user_type: str, tier: str):
        """Record a user query with context."""
        self.user_queries.labels(
            user_id=user_id,
            user_type=user_type,
            subscription_tier=tier
        ).inc()

    def record_query_complexity(self, operation: str, user_type: str, complexity: int):
        """Record query complexity score."""
        self.query_complexity.labels(
            operation_name=operation,
            user_type=user_type
        ).observe(complexity)

# Integration with FraiseQL
from fraiseql import create_fraiseql_app
from fraiseql.monitoring import setup_metrics

app = create_fraiseql_app()
metrics = setup_metrics(app)
custom_metrics = FraiseQLCustomMetrics(metrics.registry)

# Use in resolvers
@query
async def get_user(info, user_id: str) -> User:
    user_context = info.context["user"]
    custom_metrics.record_user_query(
        user_id=user_context.user_id,
        user_type=user_context.user_type,
        tier=user_context.subscription_tier
    )
    
    # Calculate query complexity
    complexity = calculate_query_complexity(info.field_nodes)
    custom_metrics.record_query_complexity(
        operation="getUser",
        user_type=user_context.user_type,
        complexity=complexity
    )
    
    # Your resolver logic here
    return await repository.get_user(user_id)
```

### 2. Real-time Monitoring Dashboard

```json
{
  "title": "FraiseQL Real-time Operations",
  "refresh": "5s",
  "panels": [
    {
      "title": "Live Query Rate",
      "type": "stat",
      "targets": [{
        "expr": "sum(rate(fraiseql_graphql_queries_total[1m]))",
        "refId": "A"
      }],
      "options": {
        "colorMode": "background",
        "graphMode": "area",
        "reduceOptions": {
          "values": false,
          "calcs": ["lastNotNull"]
        }
      },
      "fieldConfig": {
        "defaults": {
          "thresholds": {
            "steps": [
              {"color": "green", "value": 0},
              {"color": "yellow", "value": 100},
              {"color": "red", "value": 500}
            ]
          },
          "unit": "reqps"
        }
      }
    },
    {
      "title": "Error Rate %",
      "type": "stat", 
      "targets": [{
        "expr": "100 * sum(rate(fraiseql_graphql_queries_errors[1m])) / sum(rate(fraiseql_graphql_queries_total[1m]))",
        "refId": "B"
      }],
      "fieldConfig": {
        "defaults": {
          "thresholds": {
            "steps": [
              {"color": "green", "value": 0},
              {"color": "yellow", "value": 1},
              {"color": "red", "value": 5}
            ]
          },
          "unit": "percent"
        }
      }
    },
    {
      "title": "Response Time P99",
      "type": "stat",
      "targets": [{
        "expr": "histogram_quantile(0.99, sum(rate(fraiseql_response_time_seconds_bucket[1m])) by (le))",
        "refId": "C"
      }],
      "fieldConfig": {
        "defaults": {
          "thresholds": {
            "steps": [
              {"color": "green", "value": 0},
              {"color": "yellow", "value": 1},
              {"color": "red", "value": 3}
            ]
          },
          "unit": "s"
        }
      }
    },
    {
      "title": "Active Database Connections",
      "type": "gauge",
      "targets": [{
        "expr": "fraiseql_db_connections_active",
        "refId": "D"
      }],
      "options": {
        "min": 0,
        "max": 100
      },
      "fieldConfig": {
        "defaults": {
          "thresholds": {
            "steps": [
              {"color": "green", "value": 0},
              {"color": "yellow", "value": 70},
              {"color": "red", "value": 90}
            ]
          }
        }
      }
    },
    {
      "title": "Top Query Types by Volume",
      "type": "piechart",
      "targets": [{
        "expr": "topk(5, sum by (operation_type) (rate(fraiseql_graphql_queries_total[5m])))",
        "refId": "E"
      }]
    },
    {
      "title": "Cache Hit Rate Timeline",
      "type": "timeseries",
      "targets": [
        {
          "expr": "rate(fraiseql_cache_hits_total[1m])",
          "legendFormat": "Hits/sec",
          "refId": "F"
        },
        {
          "expr": "rate(fraiseql_cache_misses_total[1m])",
          "legendFormat": "Misses/sec", 
          "refId": "G"
        }
      ]
    }
  ]
}
```

### 3. Capacity Planning Queries

```promql
# Capacity planning queries for FraiseQL

# Predict when database connections will be exhausted (linear regression)
predict_linear(fraiseql_db_connections_active[1h], 3600) > fraiseql_db_connections_total * 0.9

# Memory growth trend over 6 hours
predict_linear(process_resident_memory_bytes{job="fraiseql"}[6h], 21600)

# Query volume growth (daily)
avg_over_time(sum(rate(fraiseql_graphql_queries_total[1h]))[1d:1h]) * 86400

# Storage growth estimation
predict_linear(pg_database_size_bytes{datname="fraiseql_production"}[7d], 86400 * 30)

# Network bandwidth utilization trend
predict_linear(rate(fraiseql_network_bytes_total[1h])[24h], 3600)
```

## Best Practices

### 1. Monitoring Strategy
- **Sample appropriately** - 10% tracing in production
- **Use structured logging** - JSON format for parsing
- **Set meaningful SLOs** - Based on user expectations
- **Alert on symptoms, not causes** - User-facing impact
- **Dashboard hierarchy** - Executive → Team → Debug → Real-time
- **Correlate metrics and traces** - Use trace IDs in logs
- **Regular review** - Monthly SLO and alert reviews
- **Capacity planning** - Use metrics for forecasting

### 2. Alert Design Principles
- **Actionable alerts only** - Every alert should require human action
- **Clear impact description** - What's broken and how it affects users
- **Specific remediation steps** - What to do to fix the issue
- **Appropriate severity levels** - Match urgency to actual impact
- **Escalation paths** - Who to contact when primary responder unavailable
- **Runbook integration** - Link to detailed troubleshooting procedures

### 3. Performance Monitoring
- **Focus on user experience** - Response time, error rate, availability
- **Track business metrics** - Query volume, user engagement, feature usage
- **Monitor dependencies** - Database, cache, external services
- **Capacity indicators** - Connection pools, memory, CPU, storage
- **Security metrics** - Authentication failures, authorization errors

### 4. Dashboard Design
- **Role-based views** - Executive summary, operational details, debugging info
- **Consistent time ranges** - Use same periods across related panels
- **Meaningful legends** - Clear labels for all metrics
- **Color coding** - Green/yellow/red for health status
- **Contextual links** - Connect dashboards to logs and traces

## Next Steps

- [Docker Deployment](./docker.md) - Container deployment guide
- [Kubernetes Deployment](./kubernetes.md) - Container orchestration
