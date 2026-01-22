# Fraisier Monitoring Setup Guide

**Version**: 0.1.0
**Stack**: Prometheus + Grafana + NATS

**Time to Production**: 20-30 minutes

---

## Overview

This guide covers setting up complete observability for Fraisier deployments:

- **Prometheus**: Metrics collection and storage
- **Grafana**: Visualization and dashboards
- **NATS**: Event streaming and real-time monitoring
- **Alerting**: Threshold-based alerts for incidents

---

## Quick Start

### Docker Compose Stack

All services included in `docker-compose.yml`:

```bash
docker-compose up -d prometheus grafana

# Access services
open http://localhost:9090  # Prometheus
open http://localhost:3000  # Grafana (admin/admin)
```

---

## Part 1: Prometheus Configuration

### Setup

Prometheus is included in docker-compose. Configuration file: `monitoring/prometheus.yml`

```yaml
global:
  scrape_interval: 15s      # Scrape metrics every 15 seconds
  evaluation_interval: 15s  # Evaluate rules every 15 seconds
  retention: 15d            # Keep metrics for 15 days

scrape_configs:
  # Fraisier metrics
  - job_name: 'fraisier'
    static_configs:
      - targets: ['localhost:9090']
    metrics_path: '/metrics'

  # NATS metrics
  - job_name: 'nats'
    static_configs:
      - targets: ['localhost:8222']
    metrics_path: '/metrics'

  # PostgreSQL metrics (if using postgres_exporter)
  - job_name: 'postgres'
    static_configs:
      - targets: ['localhost:9187']
```

### Key Metrics

**Deployment Metrics**:
- `fraisier_deployments_total` - Total deployments (gauge)
- `fraisier_deployment_duration_seconds` - Deployment duration (histogram)
- `fraisier_deployment_failures_total` - Failed deployments (counter)
- `fraisier_health_checks_passed` - Passed health checks (counter)
- `fraisier_health_checks_failed` - Failed health checks (counter)

**Service Metrics**:
- `fraisier_requests_total` - Total API requests
- `fraisier_request_duration_seconds` - Request latency
- `fraisier_errors_total` - Total errors

**System Metrics**:
- `fraisier_database_connections` - Active database connections
- `fraisier_database_query_duration_seconds` - Query latency
- `fraisier_nats_events_published` - NATS events published

### Query Examples

```promql
# Recent deployments
rate(fraisier_deployments_total[5m])

# Deployment success rate (%)
(fraisier_deployments_total{status="success"} / fraisier_deployments_total) * 100

# Average deployment time
avg(fraisier_deployment_duration_seconds)

# P95 deployment time
histogram_quantile(0.95, fraisier_deployment_duration_seconds)

# Failed deployments in last hour
increase(fraisier_deployment_failures_total[1h])

# Health check success rate
(fraisier_health_checks_passed / (fraisier_health_checks_passed + fraisier_health_checks_failed)) * 100
```

---

## Part 2: Grafana Dashboards

### Access Grafana

```
http://localhost:3000
Username: admin
Password: admin
```

### Add Prometheus Data Source

1. **Configuration → Data Sources**
2. Click **Add data source**
3. Select **Prometheus**
4. URL: `http://prometheus:9090`
5. Click **Save & Test**

### Import Dashboards

**Dashboard 1: Deployment Overview**

1. **Dashboards → Import**
2. Paste the JSON below or upload file

**Dashboard JSON**:

```json
{
  "dashboard": {
    "title": "Fraisier - Deployment Overview",
    "panels": [
      {
        "title": "Deployments Today",
        "targets": [
          {
            "expr": "increase(fraisier_deployments_total[1d])"
          }
        ]
      },
      {
        "title": "Success Rate (%)",
        "targets": [
          {
            "expr": "(fraisier_deployments_total{status=\"success\"} / fraisier_deployments_total) * 100"
          }
        ]
      },
      {
        "title": "Average Deployment Time",
        "targets": [
          {
            "expr": "avg(fraisier_deployment_duration_seconds)"
          }
        ]
      },
      {
        "title": "Failed Deployments",
        "targets": [
          {
            "expr": "increase(fraisier_deployment_failures_total[1d])"
          }
        ]
      }
    ]
  }
}
```

**Dashboard 2: Health Checks**

```json
{
  "dashboard": {
    "title": "Fraisier - Health Checks",
    "panels": [
      {
        "title": "Health Check Success Rate (%)",
        "targets": [
          {
            "expr": "(fraisier_health_checks_passed / (fraisier_health_checks_passed + fraisier_health_checks_failed)) * 100"
          }
        ]
      },
      {
        "title": "Passed Health Checks",
        "targets": [
          {
            "expr": "increase(fraisier_health_checks_passed[1h])"
          }
        ]
      },
      {
        "title": "Failed Health Checks",
        "targets": [
          {
            "expr": "increase(fraisier_health_checks_failed[1h])"
          }
        ]
      }
    ]
  }
}
```

### Create Custom Dashboards

1. **Dashboards → Create New**
2. Add panels:
   - **Single Stat**: Deployments today
   - **Graph**: Deployment success rate over time
   - **Heatmap**: Deployment duration distribution
   - **Table**: Recent failed deployments

---

## Part 3: Alerting Rules

### Create Alert Rules

In `monitoring/prometheus.rules.yml`:

```yaml
groups:
  - name: fraisier
    interval: 1m
    rules:
      # High failure rate alert
      - alert: HighDeploymentFailureRate
        expr: |
          (
            increase(fraisier_deployment_failures_total[1h]) /
            increase(fraisier_deployments_total[1h])
          ) > 0.1  # More than 10% failures
        for: 5m
        annotations:
          summary: "High deployment failure rate"
          description: "Deployment failure rate > 10% in the last hour"

      # Health check failures alert
      - alert: HealthCheckFailures
        expr: increase(fraisier_health_checks_failed[1h]) > 5
        for: 5m
        annotations:
          summary: "Multiple health check failures"
          description: "{{ $value }} health checks failed in the last hour"

      # NATS connection lost alert
      - alert: NatsConnectionLost
        expr: nats_server_state == 0
        for: 1m
        annotations:
          summary: "NATS connection lost"
          description: "NATS server is not responding"

      # Database connection pool exhausted
      - alert: DatabasePoolExhausted
        expr: fraisier_database_connections >= 95
        for: 5m
        annotations:
          summary: "Database connection pool almost exhausted"
          description: "{{ $value }}% of database connections in use"
```

### Configure Alert Notifications

#### Slack

1. Create Slack webhook: https://api.slack.com/messaging/webhooks
2. Configure in `prometheus.yml`:

```yaml
alerting:
  alertmanagers:
    - static_configs:
        - targets: ['localhost:9093']

rule_files:
  - 'prometheus.rules.yml'
```

3. Configure AlertManager (`alertmanager.yml`):

```yaml
global:
  slack_api_url: 'YOUR_SLACK_WEBHOOK_URL'

route:
  receiver: 'slack'

receivers:
  - name: 'slack'
    slack_configs:
      - channel: '#alerts'
        title: 'Fraisier Alert'
        text: '{{ range .Alerts }}{{ .Annotations.description }}{{ end }}'
```

#### Email

```yaml
global:
  smtp_smarthost: 'smtp.example.com:587'
  smtp_auth_username: 'your-email@example.com'
  smtp_auth_password: 'your-password'
  smtp_from: 'alerts@example.com'

receivers:
  - name: 'email'
    email_configs:
      - to: 'team@example.com'
        headers:
          Subject: 'Fraisier Alert: {{ .GroupLabels.alertname }}'
```

#### PagerDuty

```yaml
receivers:
  - name: 'pagerduty'
    pagerduty_configs:
      - service_key: 'YOUR_PAGERDUTY_KEY'
        description: '{{ .GroupLabels.alertname }}'
```

---

## Part 4: NATS Integration

### Enable NATS Metrics

NATS exposes metrics on `localhost:8222/metrics`.

Add to Prometheus config:

```yaml
scrape_configs:
  - job_name: 'nats'
    static_configs:
      - targets: ['localhost:8222']
    metrics_path: '/metrics'
```

### NATS Key Metrics

- `nats_server_uptime_seconds` - Server uptime
- `nats_server_connections` - Current connections
- `nats_server_total_connections` - Total connections since start
- `nats_server_in_msgs` - Messages received
- `nats_server_out_msgs` - Messages sent
- `nats_jetstream_accounts` - JetStream accounts
- `nats_jetstream_streams` - JetStream streams

### Monitor Event Bus

Create Grafana panel to track NATS events:

```promql
# Events per minute
rate(nats_jetstream_messages[1m])

# Event delivery rate
nats_jetstream_consumers_message_delivered

# Event pending
nats_jetstream_consumers_messages_pending
```

---

## Part 5: Advanced Monitoring

### Database Monitoring

Install postgres_exporter:

```bash
docker run -d \
  --name postgres_exporter \
  -e DATA_SOURCE_NAME="postgresql://user:password@postgres:5432/fraisier?sslmode=disable" \
  -p 9187:9187 \
  prometheuscommunity/postgres-exporter
```

Key metrics:

```promql
# Query performance
pg_stat_statements_mean_exec_time

# Replication lag
pg_replication_lag_seconds

# Cache hit ratio
pg_stat_database_blks_hit / (pg_stat_database_blks_hit + pg_stat_database_blks_read)
```

### Application Tracing

Use distributed tracing with traces from NATS events:

```bash
# View traces in Jaeger (optional)
docker run -d \
  -p 6831:6831/udp \
  -p 16686:16686 \
  jaegertracing/all-in-one
```

### Log Aggregation

Use ELK Stack or Loki:

```yaml
# Loki configuration (stores logs)
loki:
  image: grafana/loki:latest
  ports:
    - "3100:3100"
  volumes:
    - ./monitoring/loki-config.yml:/etc/loki/local-config.yaml
```

---

## Part 6: Monitoring Checklist

### Real-Time Alerts

- [ ] Deployment failure rate > 10%
- [ ] Health check failures > 5/hour
- [ ] NATS connection lost
- [ ] Database connection pool > 95%
- [ ] API response time P95 > 1 second
- [ ] API error rate > 1%

### Weekly Reviews

- [ ] Deployment trends
- [ ] Service reliability (uptime %)
- [ ] Performance trends
- [ ] Resource utilization
- [ ] Security events

### Monthly Analysis

- [ ] Capacity planning
- [ ] Cost optimization
- [ ] SLA compliance
- [ ] Incident root causes

---

## Reference

- [Prometheus Documentation](https://prometheus.io/docs/)
- [Grafana Documentation](https://grafana.com/docs/)
- [Alerting Rules](https://prometheus.io/docs/prometheus/latest/configuration/alerting_rules/)
- [NATS Monitoring](https://docs.nats.io/running-a-nats-service/nats_admin/monitoring)
