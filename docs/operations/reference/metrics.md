# FraiseQL v2 Metrics Reference

**Version**: 1.0
**Last Updated**: 2026-01-31
**Audience**: DevOps engineers, SREs, database administrators

---

## Overview

This document lists all Prometheus metrics exposed by FraiseQL v2 for monitoring and observability.

**Metrics Endpoint**: `GET /metrics` (Prometheus text format)

---

## Metrics Categories

### 1. GraphQL Query Metrics

Metrics tracking GraphQL query execution.

#### `graphql_requests_total`

- **Type**: Counter
- **Description**: Total number of GraphQL requests received
- **Labels**: (none)
- **Example**: `graphql_requests_total 1250`
- **Use Case**: Query volume trends

#### `graphql_errors_total`

- **Type**: Counter
- **Description**: Total number of GraphQL requests that resulted in errors
- **Labels**: (none)
- **Example**: `graphql_errors_total 15`
- **Use Case**: Error rate calculation, SLO tracking
- **Formula**: `error_rate = graphql_errors_total / graphql_requests_total`

#### `graphql_duration_ms`

- **Type**: Gauge
- **Description**: Average GraphQL query execution time in milliseconds
- **Labels**: (none)
- **Example**: `graphql_duration_ms 23.5`
- **Use Case**: Performance monitoring, latency SLI
- **Note**: Updates with each request

### 2. Database Metrics

Metrics tracking database operations.

#### `database_queries_total`

- **Type**: Counter
- **Description**: Total database queries executed (underlying SQL operations)
- **Labels**: (none)
- **Example**: `database_queries_total 3847`
- **Use Case**: Database load assessment
- **Note**: One GraphQL query may generate multiple database queries

#### `database_query_duration_ms`

- **Type**: Gauge
- **Description**: Average database query execution time
- **Labels**: (none)
- **Example**: `database_query_duration_ms 8.2`
- **Use Case**: Database performance analysis

### 3. Cache Metrics

Metrics tracking cache effectiveness.

#### `cache_hit_ratio`

- **Type**: Gauge
- **Description**: Cache hit ratio (0.0 to 1.0)
- **Labels**: (none)
- **Example**: `cache_hit_ratio 0.65`
- **Use Case**: Cache effectiveness monitoring
- **Interpretation**:
  - `0.0`: No hits (cache not working)
  - `0.5`: 50% hit rate (moderate effectiveness)
  - `1.0`: 100% hit rate (all queries cached)
- **Target**: > 0.6 for read-heavy workloads

#### `cache_entries`

- **Type**: Gauge
- **Description**: Current number of entries in cache
- **Labels**: (none)
- **Example**: `cache_entries 450`
- **Use Case**: Cache size monitoring, memory usage estimation

### 4. Validation and Error Metrics

Metrics tracking validation errors and failures.

#### `validation_errors_total`

- **Type**: Counter
- **Description**: Schema validation errors
- **Labels**: (none)
- **Example**: `validation_errors_total 3`
- **Use Case**: Schema quality monitoring, client error detection

#### `parse_errors_total`

- **Type**: Counter
- **Description**: GraphQL query parse errors
- **Labels**: (none)
- **Example**: `parse_errors_total 5`
- **Use Case**: Malformed query detection

#### `execution_errors_total`

- **Type**: Counter
- **Description**: Runtime execution errors (database errors, timeouts, etc.)
- **Labels**: (none)
- **Example**: `execution_errors_total 7`
- **Use Case**: Production error tracking

---

## Prometheus Queries (PromQL)

Common queries for monitoring FraiseQL:

### Query: Current Error Rate

```promql
# Current error rate (errors per second)
rate(graphql_errors_total[5m]) / rate(graphql_requests_total[5m])
```text

### Query: Queries Per Second

```promql
# QPS over last 5 minutes
rate(graphql_requests_total[5m])
```text

### Query: P95 Latency (with histogram)

*Note: Requires histogram metric - current implementation provides average*

```promql
# Average latency (current metric)
graphql_duration_ms

# Alert if average latency exceeds 100ms
graphql_duration_ms > 100
```text

### Query: Cache Hit Ratio

```promql
# Current cache hit ratio
cache_hit_ratio

# Alert if cache hit ratio below 50%
cache_hit_ratio < 0.5
```text

### Query: Database Load

```promql
# Database queries per second
rate(database_queries_total[5m])
```text

### Query: Error Budget (SLO)

```promql
# Errors allowed per minute for 99.9% uptime SLO
# (assuming 1000 QPS)
(1 - 0.999) * rate(graphql_requests_total[5m]) * 60

# Actual errors per minute
rate(graphql_errors_total[5m]) * 60

# Remaining budget
((1 - 0.999) * rate(graphql_requests_total[5m]) * 60)
- (rate(graphql_errors_total[5m]) * 60)
```text

---

## Alerting Rules

Example Prometheus alerting rules for FraiseQL:

```yaml
groups:

- name: fraiseql
  interval: 30s
  rules:

  # High error rate
  - alert: HighErrorRate
    expr: |
      (rate(graphql_errors_total[5m]) / rate(graphql_requests_total[5m])) > 0.01
    for: 2m
    labels:
      severity: critical
    annotations:
      summary: "High error rate detected"
      description: "Error rate: {{ $value | humanizePercentage }}"

  # High latency
  - alert: HighLatency
    expr: graphql_duration_ms > 100
    for: 5m
    labels:
      severity: warning
    annotations:
      summary: "High query latency"
      description: "Average latency: {{ $value }}ms"

  # Low cache hit ratio
  - alert: LowCacheHitRatio
    expr: cache_hit_ratio < 0.5
    for: 10m
    labels:
      severity: warning
    annotations:
      summary: "Low cache hit ratio"
      description: "Cache hit ratio: {{ $value | humanizePercentage }}"

  # Database is slow
  - alert: SlowDatabase
    expr: database_query_duration_ms > 50
    for: 5m
    labels:
      severity: warning
    annotations:
      summary: "Slow database queries"
      description: "Avg database query time: {{ $value }}ms"

  # Too many validation errors
  - alert: HighValidationErrorRate
    expr: rate(validation_errors_total[5m]) > 0.1
    for: 5m
    labels:
      severity: info
    annotations:
      summary: "High schema validation error rate"
      description: "{{ $value }} validation errors/sec"
```text

---

## Grafana Dashboards

### Dashboard 1: Service Overview

**Panels**:

- **QPS Chart** (line): `rate(graphql_requests_total[5m])`
- **Error Rate Chart** (line): `rate(graphql_errors_total[5m]) / rate(graphql_requests_total[5m])`
- **Latency Gauge**: `graphql_duration_ms`
- **Cache Hit Ratio Gauge**: `cache_hit_ratio`

### Dashboard 2: Performance Monitoring

**Panels**:

- **Query Latency** (line): `graphql_duration_ms` over time
- **Database Latency** (line): `database_query_duration_ms` over time
- **Database QPS** (line): `rate(database_queries_total[5m])`
- **Cache Size** (line): `cache_entries` over time

### Dashboard 3: Error Analysis

**Panels**:

- **Error Rate** (line): `rate(graphql_errors_total[5m])`
- **Validation Errors** (line): `rate(validation_errors_total[5m])`
- **Parse Errors** (line): `rate(parse_errors_total[5m])`
- **Execution Errors** (line): `rate(execution_errors_total[5m])`

---

## Metric Collection Examples

### Using curl

```bash
# Get all metrics
curl http://localhost:8000/metrics

# Example output:
# # HELP graphql_requests_total Total GraphQL requests
# # TYPE graphql_requests_total counter
# graphql_requests_total 1250
#
# # HELP graphql_errors_total Total GraphQL errors
# # TYPE graphql_errors_total counter
# graphql_errors_total 15
#
# # HELP graphql_duration_ms Average duration
# # TYPE graphql_duration_ms gauge
# graphql_duration_ms 23.5
```text

### Using Prometheus Scrape Config

```yaml
# prometheus.yml
global:
  scrape_interval: 15s

scrape_configs:
  - job_name: fraiseql
    static_configs:
      - targets:
          - localhost:8000
    metrics_path: /metrics
    scrape_interval: 10s
    scrape_timeout: 5s
```text

### Using Grafana Data Source

```json
{
  "type": "prometheus",
  "name": "Prometheus",
  "url": "http://prometheus:9090",
  "access": "proxy",
  "isDefault": true
}
```text

---

## Metric Naming Convention

All FraiseQL metrics follow these conventions:

**Format**: `{subsystem}_{name}_{unit}`

- `graphql_*` - GraphQL execution metrics
- `database_*` - Database operation metrics
- `cache_*` - Cache metrics
- `*_total` - Counter metrics (monotonically increasing)
- `*_ms` - Millisecond values
- `*_ratio` - Ratio values (0.0 to 1.0)

---

## SLO Examples

### Example 1: 99.9% Uptime SLO

```text
Allowed error budget: (1 - 0.999) = 0.1% of requests
Per month (assuming 1M requests/day):

- Total requests: 30M
- Allowed errors: 30,000
- Available errors per request rate
```text

**Alerting Strategy**:

```promql
# Burn rate alerts
rate(graphql_errors_total[5m]) / rate(graphql_requests_total[5m]) > 0.001
```text

### Example 2: P95 Latency < 100ms SLO

```promql
# Current metric is average, not P95
# For accurate P95: use histogram implementation
histogram_quantile(0.95, rate(graphql_duration_ms_bucket[5m])) > 100
```text

---

## Performance Baselines

**Typical Production Values**:

- QPS: 100-10,000 (depends on schema complexity)
- Latency: 10-100ms (depends on database and query complexity)
- Error rate: < 0.1%
- Cache hit ratio: 0.4-0.8 (depends on workload)

---

## References

- [Prometheus Metrics Types](https://prometheus.io/docs/concepts/metric_types/)
- [PromQL Documentation](https://prometheus.io/docs/prometheus/latest/querying/basics/)
- [Grafana Dashboard Documentation](https://grafana.com/docs/grafana/latest/dashboards/)
- [Observability Guide](../../observability/)
- [Operations Guide](../guide.md)

---

**Last Updated**: 2026-01-31
