# Monitoring Dashboard Specification

**Conducted By**: Observability Engineer
**Date**: January 26, 2026

---

## 1. Dashboard Architecture

```
┌──────────────────────────────────────────┐
│        Prometheus (Metrics Store)        │
│  - Scrapes /metrics endpoint every 15s   │
│  - 15-day retention, 1GB storage         │
└─────────────────┬──────────────────────┘
                  │
        ┌─────────┴──────────┐
        │                    │
┌───────▼────────┐  ┌─────────▼──────┐
│    Grafana     │  │   AlertManager │
│  (Dashboards)  │  │  (Notifications) │
└────────────────┘  └────────────────┘
```

---

## 2. Dashboard 1: Service Health

### Metrics

| Widget | Metric | Alert Threshold |
|--------|--------|-----------------|
| **Uptime** | `up{job="fraiseql"}` | < 1 for 1m |
| **Error Rate** | `rate(errors[5m])` | > 5% |
| **Latency P95** | `histogram_quantile(0.95, latency)` | > 200ms |
| **Latency P99** | `histogram_quantile(0.99, latency)` | > 500ms |
| **Throughput** | `rate(requests[1m])` | Baseline -30% |

### Layout

```
┌─────────────────────────────────────────────┐
│  FraiseQL Service Health Dashboard          │
├─────────────────────────────────────────────┤
│  Uptime: 99.98%  | Error Rate: 0.2%        │
│  P95: 145ms      | P99: 320ms              │
│  Throughput: 8.2k req/s                    │
├─────────────────────────────────────────────┤
│  [Error Rate Trend Graph]  [Latency Graph] │
│  [Throughput Graph]        [Uptime Graph]  │
└─────────────────────────────────────────────┘
```

---

## 3. Dashboard 2: Database Performance

### Metrics

```
fraiseql_db_connections{state}
fraiseql_db_query_duration_seconds{quantile}
fraiseql_db_rows_processed_total
fraiseql_db_errors_total{type}
fraiseql_db_slow_queries_total
```

### Alerts

```yaml
- alert: ConnectionPoolExhaustion
  expr: fraiseql_db_connections{state="active"} > 45
  for: 5m
  annotations:
    severity: warning
    runbook: connections-exhausted.md

- alert: SlowQueries
  expr: rate(fraiseql_db_slow_queries_total[5m]) > 10
  for: 5m
  annotations:
    severity: warning
    runbook: slow-queries.md
```

---

## 4. Dashboard 3: Security & Authentication

### Metrics

```
fraiseql_auth_success_total{method}
fraiseql_auth_failure_total{method,reason}
fraiseql_rate_limit_exceeded_total{key_type}
fraiseql_sql_injection_attempts_total
fraiseql_csrf_failures_total
fraiseql_tls_version_connections{version}
```

### Security Alerts

```yaml
- alert: AuthenticationSpike
  expr: rate(fraiseql_auth_failure_total[5m]) > 100
  for: 1m
  annotations:
    severity: high
    runbook: auth-spike.md

- alert: SQLInjectionAttempt
  expr: fraiseql_sql_injection_attempts_total > 0
  for: 0m
  annotations:
    severity: critical

- alert: RateLimitingActive
  expr: rate(fraiseql_rate_limit_exceeded_total[5m]) > 50
  for: 5m
  annotations:
    severity: warning
```

---

## 5. Dashboard 4: Caching & Performance

### Metrics

```
fraiseql_cache_hits_total{cache_type}
fraiseql_cache_misses_total{cache_type}
fraiseql_apq_store_size_bytes
fraiseql_query_complexity_score{quantile}
```

### Calculations

```
Cache Hit Ratio = hits / (hits + misses)

Alert: Low Cache Hit Ratio
expr: fraiseql_cache_hit_ratio < 0.7
for: 10m
```

---

## 6. Dashboard 5: Resource Utilization

### Metrics

```
process_resident_memory_bytes      (App memory)
process_virtual_memory_bytes       (Virtual memory)
process_cpu_seconds_total          (CPU time)
node_memory_MemAvailable_bytes     (System free)
node_cpu_seconds_total             (System CPU)
node_disk_io_bytes_total           (Disk I/O)
```

### Alerts

```yaml
- alert: MemoryLeak
  expr: rate(process_resident_memory_bytes[30m]) > 10_000_000
  for: 30m
  annotations:
    severity: critical

- alert: HighCPUUsage
  expr: rate(process_cpu_seconds_total[5m]) > 0.8
  for: 5m
  annotations:
    severity: warning
```

---

## 7. Dashboard 6: GraphQL Operations

### Metrics

```
fraiseql_queries_total{operation,status}
fraiseql_mutations_total{operation,status}
fraiseql_subscriptions_active
fraiseql_field_resolution_seconds{field,quantile}
fraiseql_query_complexity_score
```

### Visualization

```
Top Operations by Error Rate:
1. listUsers: 2.3% (↑ from 1.2% yesterday)
2. createPost: 1.1%
3. deleteComment: 0.8%

Slowest Operations (P95):
1. getFullUserProfile: 450ms
2. listUserPosts: 380ms
3. searchUsers: 290ms
```

---

## 8. Dashboard 7: Infrastructure Health

### Metrics

```
node_uname_info                    (Host info)
node_cpu_count                     (CPUs)
node_memory_MemTotal_bytes         (Total RAM)
node_disk_total_bytes              (Total disk)
node_network_transmit_bytes_total  (Network out)
node_network_receive_bytes_total   (Network in)
```

### Status

```
┌─────────────────────────────┐
│ Infrastructure Status       │
├─────────────────────────────┤
│ Instances: 3 (All healthy)  │
│ Database: 1 primary, 1 rpl  │
│ Network: 1Gbps (89% util)   │
│ Storage: 350GB / 500GB      │
└─────────────────────────────┘
```

---

## 9. Alert Routing

### Severity Levels

```yaml
# CRITICAL: Page on-call immediately
- Service down
- SQL injection detected
- Authentication system down
- Data breach detected

# HIGH: Alert to team, page if no ack in 15m
- Error rate > 5%
- Latency P95 > 5s
- Memory leak detected
- Rate limiting active

# MEDIUM: Alert to team
- Error rate > 1%
- Latency P95 > 1s
- Cache hit ratio < 50%

# LOW: Log only
- Error rate > 0.1%
- Cache hit ratio < 70%
```

### Notification Channels

```
CRITICAL → PagerDuty → Phone call
HIGH     → Slack (#alerts)
MEDIUM   → Slack (#alerts-digest)
LOW      → Email digest (daily)
```

---

## 10. Dashboards to Create

### Priority 1 (Pre-GA)
- [ ] Service Health Dashboard
- [ ] Database Performance Dashboard
- [ ] Security & Auth Dashboard

### Priority 2 (Q1 2026)
- [ ] Caching & Performance Dashboard
- [ ] Resource Utilization Dashboard
- [ ] GraphQL Operations Dashboard

### Priority 3 (Q2 2026)
- [ ] Infrastructure Health Dashboard
- [ ] Custom Business Metrics Dashboard
- [ ] Executive Summary Dashboard

---

## 11. Thresholds & SLIs

| SLI | Target | Alert |
|-----|--------|-------|
| **Latency P95** | < 200ms | > 250ms |
| **Latency P99** | < 500ms | > 750ms |
| **Error Rate** | < 0.1% | > 0.5% |
| **Availability** | 99.95% | < 99.90% |
| **Cache Hit Ratio** | > 75% | < 60% |

---

## 12. Sample Grafana JSON

```json
{
  "dashboard": {
    "title": "FraiseQL Service Health",
    "panels": [
      {
        "title": "Request Rate",
        "targets": [
          {
            "expr": "rate(fraiseql_requests_total[5m])"
          }
        ],
        "type": "graph"
      },
      {
        "title": "Error Rate",
        "targets": [
          {
            "expr": "rate(fraiseql_errors_total[5m])"
          }
        ],
        "type": "graph",
        "thresholds": [
          { "value": 0.05, "color": "red", "fill": true }
        ]
      }
    ]
  }
}
```

---

## 13. Log Aggregation

### ELK Stack Configuration

```yaml
# Filebeat config
filebeat.inputs:
  - type: log
    enabled: true
    paths:
      - /var/log/fraiseql/*.log
    fields:
      app: fraiseql
      environment: production

output.elasticsearch:
  hosts: ["elasticsearch:9200"]
  index: "fraiseql-%{+yyyy.MM.dd}"
```

### Important Logs to Index

```
- Query execution traces
- Authentication attempts
- Authorization decisions
- Error stack traces
- Performance metrics
- Security events
```

---

## 14. Runbooks

Create runbooks for each alert:

- [ ] AuthenticationSpike
- [ ] HighErrorRate
- [ ] HighLatency
- [ ] MemoryLeak
- [ ] SQLInjectionAttempt
- [ ] RateLimitingActive

**Format**:
```markdown
# Alert: HighErrorRate

## Symptoms
- Error rate > 5% for > 5 minutes

## Investigation
1. Check logs for error patterns
2. Identify affected endpoints
3. Check database status
4. Check recent deployments

## Mitigation
1. If bad deployment: Rollback
2. If DB issue: Failover
3. If overwhelmed: Scale up
```

---

## 15. Implementation Plan

| Phase | Dashboards | Timeline | Tools |
|-------|-----------|----------|-------|
| **Phase 1** | 3 dashboards | Weeks 1-2 | Grafana |
| **Phase 2** | 3 dashboards | Weeks 3-4 | Grafana |
| **Phase 3** | 3 dashboards | Weeks 5-6 | Grafana |
| **Phase 4** | Custom metrics | Week 7-8 | Prometheus |

---

**Specification Completed**: January 26, 2026
**Lead Engineer**: Observability Engineer
**Status**: Ready for implementation
