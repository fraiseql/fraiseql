# FraiseQL Service Level Agreement (SLA) and Objectives (SLO)

**Version**: 2.0.0-beta.2
**Last Updated**: February 2026
**Status**: Operational Standards

## Executive Summary

FraiseQL is a compiled GraphQL execution engine built on Rust with zero-runtime overhead for deterministic query execution. This document defines our service level commitments, performance targets, and operational guardrails for production deployments.

**Key Philosophy**: FraiseQL's deterministic SQL generation at compile-time enables predictable, measurable performance characteristics that form the foundation of these SLOs.

---

## 1. Availability SLO

### Definitions

- **Availability**: Percentage of time when FraiseQL server responds successfully to health checks and GraphQL queries
- **Downtime**: Any period where the service returns 5xx errors or is unreachable
- **Incident**: Any event causing unavailability exceeding 60 seconds

### Targets by Environment

| Environment | SLO | Monthly Allowance | Quarterly Allowance |
|-------------|-----|-------------------|---------------------|
| **Production** | 99.9% | 43.8 minutes | 131.4 minutes |
| **Staging** | 99.0% | 438 minutes | 1,314 minutes |
| **Development** | Best Effort | N/A | N/A |

### Measurement

Availability is calculated as:

```
Availability % = (Total Monitoring Seconds - Downtime Seconds) / Total Monitoring Seconds × 100
```

**Monitoring Points**:
- HTTP endpoint health check (`GET /health`)
- Sample GraphQL query execution against production schema (synthetic transaction)
- Database connectivity heartbeat

**Measurement Frequency**: Every 10 seconds from at least 3 independent regions

**Grace Periods**:
- 0-60 seconds: Not counted as incident (transient failures)
- 60+ seconds: Counted toward SLO calculation

### Excluded Incidents

The following do NOT count against availability SLO:

1. Scheduled maintenance (requires 72-hour notice, max 1 hour per month)
2. Infrastructure provider outages (cloud region failures beyond our control)
3. DDoS attacks on FraiseQL directly (if properly defended)
4. Customer misconfiguration (e.g., unreachable database, exhausted connection pools)
5. Network issues beyond our infrastructure boundary

### Availability Credits (if applicable)

For SaaS deployments, credits are issued automatically:

```
Production Availability:
99.5% - 99.9% → 10% credit
99.0% - 99.5% → 25% credit
95.0% - 99.0% → 50% credit
< 95.0%      → 100% credit
```

---

## 2. Latency SLO (GraphQL Queries)

### Definitions

FraiseQL queries complete deterministically based on compiled SQL, not runtime graph traversal. Latency targets reflect the compiled execution model.

- **Request Latency**: Time from HTTP POST request to first byte of response
- **P50, P95, P99**: Percentile response times under sustained load
- **Query Type**: Impact varies by compiled query complexity

### Targets by Query Type

| Query Type | P50 | P95 | P99 | P99.9 | Notes |
|------------|-----|-----|-----|-------|-------|
| **Simple** (1-2 table join) | < 5ms | < 25ms | < 100ms | < 500ms | Most queries |
| **Moderate** (3-5 joins, aggregation) | < 10ms | < 50ms | < 200ms | < 1000ms | Typical transactional |
| **Complex** (5+ joins, nested batches) | < 50ms | < 150ms | < 500ms | < 2000ms | Analytical queries |
| **Bulk Operations** (1000+ records) | < 100ms | < 500ms | < 2000ms | < 5000ms | Batch processing |

### Measurement Conditions

**Load Profile**:
- 1,000 requests/second sustained (simple queries)
- 500 requests/second sustained (moderate queries)
- 100 requests/second sustained (complex queries)
- Warmed connection pool (minimum 10 idle connections)
- Database with < 10ms network latency

**Not Included in Latency SLO**:
- Network latency beyond FraiseQL's datacenter boundary
- Client-side request serialization time
- Time spent in database query execution beyond compiled SQL template
- Network egress time (response streaming)

### Performance Tiers

**Tier 1 (Standard Hosting)**
- Dedicated: Quad-core CPU, 16GB RAM, local SSD cache
- Expected: P99 < 200ms for moderate queries
- Throughput: > 2,000 rps (simple)

**Tier 2 (Performance Hosting)**
- Dedicated: 8+ core CPU, 32GB RAM, high-speed cache backend (Redis)
- Expected: P99 < 100ms for moderate queries
- Throughput: > 5,000 rps (simple)

**Tier 3 (Enterprise Hosting)**
- Dedicated: 16+ core CPU, 64GB RAM, distributed cache, Vault integration
- Expected: P99 < 50ms for moderate queries
- Throughput: > 10,000 rps (simple)

### Latency Optimization Opportunities

1. **Schema Compilation**: Pre-compile frequently accessed queries
2. **Query Caching**: Enable in-memory LRU cache for repeated queries
3. **Automatic Persisted Queries (APQ)**: Reduce client-to-server payload size
4. **Connection Pooling**: Reuse database connections (deadpool, default 30-50 connections)
5. **Redis Integration**: Optional cache backend for computed fields
6. **Vault Integration**: Pre-load secrets at startup (enterprise)

---

## 3. Throughput SLO

### Sustained Throughput Targets

FraiseQL is tested for sustained throughput under full feature operation (logging, observability, all features enabled).

| Scenario | RPS | Concurrent Connections | Notes |
|----------|-----|------------------------|-------|
| **Simple Queries** | > 2,000 | 100-500 | Typical single-table reads |
| **Mixed Workload** | > 1,500 | 500-1,000 | Mix of read/write operations |
| **Complex Queries** | > 500 | 200-500 | Multi-table joins, aggregations |
| **Bulk Operations** | > 100 | 50-200 | Large result sets (1000+ rows) |

### Connection Limits

| Resource | Limit | Default | Notes |
|----------|-------|---------|-------|
| **Max Concurrent Connections** | 10,000 | 1,000 | Configurable via `Server::with_max_connections()` |
| **Connection Pool Size** | 2,000 | 50 | Per-database backend pool |
| **Idle Connection Timeout** | 30 minutes | 30min | Connections closed after inactivity |
| **Request Queue Depth** | 10,000 | 10,000 | Queued requests before 503 |

### Backpressure Behavior

When throughput limits are exceeded:

1. **0-100% Capacity**: Normal operation, latency < SLO
2. **100-110% Capacity**: Requests queued, latency begins increasing
3. **110-120% Capacity**: P99 latency > SLO, some requests slow
4. **120%+ Capacity**: Return 503 Service Unavailable with `Retry-After: 5` header

**Recommended Scaling**:
- Monitor P99 latency continuously
- Scale horizontally when P99 > SLO + 50%
- Use load balancer with connection draining

---

## 4. Error Rate SLO

### Error Classification

| Category | Examples | SLO | Impact |
|----------|----------|-----|--------|
| **5xx Errors** | Internal errors, panics, timeout | < 0.1% | Availability incident |
| **4xx Errors** | Invalid schema, auth failure, validation | < 1% | User error, not our fault |
| **3xx Redirects** | Never used | N/A | Not applicable |
| **2xx Success** | All successful responses | > 99.9% | Primary target |

### Acceptable Error Rates (Monthly)

With 10M requests/month:

- **5xx Errors**: < 10,000 errors/month (1 in 1,000)
- **4xx Errors**: < 100,000 errors/month (1 in 100)
- **Soft 4xx**: Up to 5% of 4xx may be transient (retryable)

### Error Response Format

All errors follow FraiseQL's standard error envelope:

```json
{
  "errors": [
    {
      "message": "Sanitized error message",
      "extensions": {
        "code": "QUERY_EXECUTION_ERROR",
        "path": ["User", "posts", 0],
        "timestamp": "2026-02-19T14:30:00Z"
      }
    }
  ]
}
```

**Error Sanitization**: Production environments never expose stack traces, internal SQL, or database details in error messages.

### Common 5xx Errors to Monitor

1. **INTERNAL_SERVER_ERROR** (500)
   - Unexpected panic in query executor
   - Database connection pool exhausted
   - Out of memory condition

2. **TEMPORARY_FAILURE** (500)
   - Database briefly unavailable
   - Vault connection timeout
   - Cache backend unreachable

3. **QUERY_TIMEOUT** (504 Gateway Timeout)
   - Query execution exceeded timeout (default 30s)
   - Database slow to respond
   - External data source timeout

### Common 4xx Errors (Not Counted Against SLO)

- **400 Bad Request**: Malformed GraphQL, invalid JSON
- **401 Unauthorized**: Missing or invalid authentication token
- **403 Forbidden**: User lacks permission for query
- **404 Not Found**: Requested type/field doesn't exist in schema
- **422 Unprocessable Entity**: Validation failure (invalid variables)

---

## 5. Recovery and Incident Response SLO

### Incident Detection

FraiseQL includes built-in health monitoring:

| Signal | Detection Time | Action |
|--------|---|--------|
| **Health Check Failure** | < 30 seconds | Alert on-call engineer |
| **Error Rate Spike** | < 60 seconds | Alert + auto-log |
| **Latency Degradation** | < 60 seconds | Alert + check dependencies |
| **Connection Pool Exhaustion** | < 30 seconds | Alert + consider circuit break |
| **Panic/Crash** | < 10 seconds | Alert immediately |

### Recovery Time Objectives (RTO)

| Severity | Detection | Mitigation | Resolution | Alert Channel |
|----------|-----------|-----------|-----------|---|
| **Critical** (P1) | < 60s | < 15min | < 4h | PagerDuty + SMS + Slack |
| **High** (P2) | < 2min | < 30min | < 8h | PagerDuty + Slack |
| **Medium** (P3) | < 5min | < 1h | < 24h | Slack + email |
| **Low** (P4) | < 1h | < 4h | < 1 week | Email + weekly report |

### Recovery Time Objectives (RTO) by Scenario

| Failure Mode | Detection | Mitigation | Resolution |
|---|---|---|---|
| **Single Instance Down** | 10-30s | Auto-failover to replica | < 5 min |
| **Database Unavailable** | 10-30s | Circuit break, return cached response | < 15 min |
| **Redis Cache Down** | 10-30s | Fall through to database | < 15 min |
| **Vault Unavailable** | 30-60s | Use cached secrets, restrict auth ops | < 15 min |
| **All Replicas Down** | 60s | Manual intervention | < 30 min |
| **Data Corruption** | Detection varies | Restore from backup | < 4 h |

### Graceful Degradation (See Section 7)

The system automatically mitigates the impact of dependency failures.

---

## 6. Monitoring, Alerting, and Observability

### Metrics to Monitor

**FraiseQL publishes metrics on `/metrics` endpoint (Prometheus format)**:

```
# HELP fraiseql_query_duration_seconds Query execution time in seconds
# TYPE fraiseql_query_duration_seconds histogram
fraiseql_query_duration_seconds_bucket{le="0.005",query_type="simple"} 1250
fraiseql_query_duration_seconds_bucket{le="0.025",query_type="simple"} 1498
fraiseql_query_duration_seconds_bucket{le="0.1",query_type="simple"} 1500

# HELP fraiseql_query_total Total GraphQL queries executed
# TYPE fraiseql_query_total counter
fraiseql_query_total{status="success"} 50000
fraiseql_query_total{status="error"} 100
fraiseql_query_total{status="timeout"} 5

# HELP fraiseql_db_connections_active Active database connections
# TYPE fraiseql_db_connections_active gauge
fraiseql_db_connections_active 42

# HELP fraiseql_cache_hit_ratio Cache hit ratio
# TYPE fraiseql_cache_hit_ratio gauge
fraiseql_cache_hit_ratio{cache="query_lru"} 0.85
fraiseql_cache_hit_ratio{cache="redis"} 0.92

# HELP fraiseql_auth_failures_total Authentication/authorization failures
# TYPE fraiseql_auth_failures_total counter
fraiseql_auth_failures_total{reason="invalid_token"} 15
fraiseql_auth_failures_total{reason="expired_token"} 8
fraiseql_auth_failures_total{reason="insufficient_scope"} 3
```

### Grafana Dashboards

**Standard Dashboards** (included in installation):

1. **Overview Dashboard**
   - Availability % (monthly)
   - Request rate (RPS)
   - Error rate (% of 5xx)
   - P50, P95, P99 latency

2. **Query Performance Dashboard**
   - Latency by query type (simple/moderate/complex)
   - Throughput by operation (Query/Mutation)
   - Slow queries (> P99 threshold)
   - Query cache hit rate

3. **Infrastructure Dashboard**
   - Database connection pool utilization
   - Memory usage and GC pauses
   - CPU utilization
   - Network I/O (in/out bytes)

4. **Dependency Health Dashboard**
   - Database connectivity (latency, timeouts)
   - Redis cache health (hit rate, memory)
   - Vault secret access (latency, failures)
   - External data source availability

5. **Security Dashboard**
   - Authentication failure rate
   - Authorization denials
   - Audit log event rate
   - Rate limiting (requests dropped)

### Alert Rules

**Critical Alerts** (PagerDuty immediate):

```prometheus
# Availability below 99.5% over 5 minutes
alert: AvailabilityLow
  expr: fraiseql_availability_percent < 99.5
  for: 5m

# Error rate spike (> 1% 5xx)
alert: ErrorRateHigh
  expr: rate(fraiseql_query_total{status="error"}[5m]) / rate(fraiseql_query_total[5m]) > 0.01
  for: 1m

# Latency degradation (P99 > 2x SLO)
alert: LatencyDegraded
  expr: histogram_quantile(0.99, fraiseql_query_duration_seconds) > 0.4
  for: 2m

# Database connectivity issues
alert: DatabaseConnectionDown
  expr: fraiseql_db_connections_active == 0
  for: 30s

# Request queue backing up
alert: RequestQueueLarge
  expr: fraiseql_request_queue_depth > 5000
  for: 2m

# Panic/crash detected
alert: ProcessRestarted
  expr: increase(process_start_time_seconds[5m]) > 0
  for: 1m
```

**Warning Alerts** (Slack + email):

```prometheus
# Latency trending up
alert: LatencyTrend
  expr: rate(histogram_quantile(0.95, fraiseql_query_duration_seconds)[1h:5m]) > 0
  for: 15m

# Cache hit rate dropping
alert: CacheHitRateLow
  expr: fraiseql_cache_hit_ratio < 0.7
  for: 10m

# Connection pool approaching limit
alert: ConnectionPoolNearLimit
  expr: fraiseql_db_connections_active / fraiseql_db_connections_limit > 0.8
  for: 5m

# Vault secret rotation approaching expiration
alert: SecretRotationDue
  expr: fraiseql_secret_expiry_seconds < 3600
  for: 1h
```

### Synthetic Monitoring

Continuously execute synthetic transactions to verify end-to-end functionality:

```
Frequency: Every 10 seconds
Locations: US East, US West, Europe, Asia-Pacific
Tests:
  - Simple query (1 table): SELECT * WHERE id = 1
  - Moderate query (3 table join): User + Posts + Comments
  - Complex query (5 joins): Full graph traversal
  - Write operation: Mutation affecting multiple types
  - Auth flow: Token validation, scope checking
```

Success criteria: All synthetic tests pass within SLO latency bounds.

### Logging

**Log Levels**:
- `ERROR`: User-facing errors (5xx, 4xx), exceptions
- `WARN`: Degraded performance, dependency timeouts, rate limiting
- `INFO`: Request/response logging (sample 1%), metric milestones
- `DEBUG`: Query execution details, caching decisions (disabled in production)

**Log Retention**: 30 days (hot), 1 year (archive)

**Audit Logging**: Enabled by default for:
- All authentication events
- Authorization denials
- Data access patterns (if enabled in schema config)
- Configuration changes

---

## 7. Graceful Degradation

FraiseQL automatically mitigates dependency failures to maintain availability.

### Database Unavailability

**Scenario**: Primary database connection fails or timeouts

**Detection**: < 30 seconds (connection pool timeout)

**Mitigation**:
1. Fail-over to read replica (if configured)
2. If write query: Return `TEMPORARY_FAILURE` (503)
3. If read query: Serve from cache (if available)
4. Stop accepting new connections; drain existing

**Impact**:
- **Writes**: All mutations fail with 503 (unavoidable)
- **Reads**: Served from cache (LRU or Redis) if hit; otherwise 503
- **Latency**: Reads from cache: < 5ms; no cache: immediate 503
- **Availability**: Can maintain 95%+ availability for read-heavy workloads

**Recovery**:
- Attempts to reconnect every 5 seconds
- Exponential backoff up to 30 seconds
- Resumes normal operation once primary responds

### Redis Cache Unavailable

**Scenario**: Redis connection fails (if enabled)

**Detection**: < 10 seconds (Redis TCP timeout)

**Mitigation**:
1. Fall through to database for all queries
2. Disable write-through cache updates
3. Continue normal query execution (no caching benefit)

**Impact**:
- **Latency**: +50-100ms (database queries instead of cache)
- **Throughput**: -20-30% (cache misses become DB hits)
- **Availability**: No impact (database is authoritative)

**Recovery**:
- Attempts to reconnect every 30 seconds
- Resumes cache operations once Redis is healthy
- Warm cache on recovery (scan recent queries)

### Vault (Secrets Management) Unavailable

**Scenario**: Vault is unreachable for secret rotation

**Detection**: < 60 seconds (Vault TCP timeout)

**Mitigation**:
1. Use cached secrets from startup
2. If secret is expired: Return 401 (require re-authentication)
3. If secret is valid: Continue using cached version

**Impact**:
- **Authentication**: Works for secrets valid < 1 hour
- **Authorization**: Revocation checks may be stale (max 1 hour)
- **Rate Limiting**: Uses cached limits
- **Availability**: Minimal impact for typical usage

**Recovery**:
- Attempts to reconnect every 60 seconds
- Re-syncs secrets once Vault is healthy
- Enforces re-authentication for expired credentials

### Multiple Failures (Cascading)

**Scenario**: Database + Redis + Vault all unavailable

**Behavior**:
1. Database down → Can't write, reads from cache only
2. Redis down → Can't cache, reads from database only
3. Database + Redis down → Return 503 for all requests (fallback to circuit breaker)
4. Add Vault down → Can't verify auth, return 401

**Failsafe Mode**: Circuit breaker automatically engages after:
- 5 consecutive connection failures in 10 seconds
- Pauses traffic for 30 seconds, then tries again
- Prevents cascading resource exhaustion

---

## 8. Capacity Planning

### Resource Requirements

**Minimum Production Setup** (Tier 1):
- **CPU**: 4 cores (2.5+ GHz modern CPU)
- **Memory**: 16 GB RAM
- **Storage**: 20 GB SSD (for logs, compiled schema, caching)
- **Database**: PostgreSQL 12+ or MySQL 8.0+
- **Network**: 1 Gbps connectivity to database

**Recommended Production Setup** (Tier 2):
- **CPU**: 8 cores (3+ GHz, multi-socket for large deployments)
- **Memory**: 32 GB RAM
- **Storage**: 100 GB SSD (NVMe recommended)
- **Cache**: Redis 6.0+ (optional, 8-16 GB)
- **Secrets**: HashiCorp Vault (optional)
- **Network**: 10 Gbps for high-throughput scenarios

**Enterprise Setup** (Tier 3):
- **CPU**: 16+ cores across multiple instances
- **Memory**: 64+ GB per instance
- **Storage**: 500+ GB distributed cache
- **Cache**: Distributed Redis cluster
- **Secrets**: Vault HA cluster
- **Load Balancer**: Application-aware with connection draining
- **Monitoring**: Prometheus + Grafana stack

### Scaling Guidelines

#### Vertical Scaling (Single Machine)

**Throughput Increase by CPU**:
```
2 cores:     ~500 rps (simple queries)
4 cores:   ~1,500 rps (simple queries)
8 cores:   ~4,000 rps (simple queries)
16 cores: ~10,000 rps (simple queries)
```

**Rule of Thumb**: Each core provides ~500-800 rps for simple queries (depending on CPU generation)

**Diminishing Returns**: After 8 cores, gains plateau due to cache coherency; horizontal scaling becomes more efficient.

#### Horizontal Scaling (Multiple Machines)

**Database Connection Pool Concerns**:
- Each FraiseQL instance: 30-50 database connections
- 10 instances × 50 connections = 500 total connections
- Most databases support 1,000+ connections; verify your database limits

**Load Balancer Configuration**:
- Use sticky sessions (connection pooling is per-client)
- Implement connection draining on shutdown (drain 30 seconds)
- Health check every 10 seconds

**Example Setup**:
```
1 load balancer (Layer 4 or 7)
  ├─ FraiseQL instance 1 (8 cores, 32GB) → 4,000 rps
  ├─ FraiseQL instance 2 (8 cores, 32GB) → 4,000 rps
  ├─ FraiseQL instance 3 (8 cores, 32GB) → 4,000 rps
  └─ FraiseQL instance 4 (8 cores, 32GB) → 4,000 rps
  = 16,000 rps total capacity
```

### Monitoring for Scaling Triggers

**Scale Up When**:
- P99 latency > SLO + 50% for > 10 minutes
- Database connection pool > 80% utilization
- CPU utilization > 75% for > 5 minutes
- Memory usage > 85% of allocation

**Scale Down When** (after load reduction):
- P99 latency < SLO - 50% for > 30 minutes
- CPU utilization < 25% for > 30 minutes
- Database connection pool < 20% utilization

### Cost Estimation

**Per-Instance Monthly Cost** (approximate, US East region):

| Tier | CPU | Memory | Storage | Network | Monthly Cost |
|------|-----|--------|---------|---------|--------------|
| Small (4c/16GB) | $50 | $30 | $5 | $5 | ~$90 |
| Medium (8c/32GB) | $100 | $60 | $10 | $10 | ~$180 |
| Large (16c/64GB) | $200 | $120 | $20 | $20 | ~$360 |

**Managed Services** (RDS, ElastiCache, Vault-as-a-service):
- PostgreSQL HA: +$200-500/month
- Redis Cache: +$50-200/month
- Vault Cloud: +$50-100/month

### Auto-Scaling Configuration Example

```toml
# In fraiseql.toml or environment variables
[fraiseql.scaling]
min_instances = 2           # Always run 2 replicas
max_instances = 10          # Cap at 10 instances
cpu_threshold = 0.75        # Scale up at 75% CPU
memory_threshold = 0.85     # Scale up at 85% memory
p99_latency_threshold_ms = 200  # Scale up if P99 > SLO × 1.5
scale_up_cooldown = 60      # Wait 60s between scale-up decisions
scale_down_cooldown = 300   # Wait 5m between scale-down decisions
```

---

## 9. Compliance and Reporting

### Monthly SLO Report

**Distributed automatically on the 1st of each month**:

```
FraiseQL SLO Report - February 2026
===================================

Availability
  Production: 99.97% ✅ (Exceeded 99.9% target)
  Staging:    99.45% ✅ (Exceeded 99.0% target)

Latency (GraphQL Queries)
  Simple queries (P99):      78ms ✅ (Target: < 100ms)
  Moderate queries (P99):   142ms ✅ (Target: < 200ms)
  Complex queries (P99):    456ms ✅ (Target: < 500ms)

Throughput
  Peak RPS: 3,200 ✅ (Target: > 2,000 rps)
  Sustained RPS: 2,100 ✅ (Target: > 2,000 rps)

Error Rates
  5xx errors: 0.09% ✅ (Target: < 0.1%)
  4xx errors: 0.87% ✅ (Target: < 1%)

Recovery
  Avg incident detection: 22 seconds ✅
  Avg mitigation time: 8 minutes ✅
  Longest incident: 18 minutes

Dependencies
  Database availability: 99.98% ✅
  Redis availability: 99.96% ✅
  Vault availability: 99.99% ✅

Credits Issued: None (All SLOs met)
Incidents: 1 (Database failover, automatically recovered)
```

### Quarterly Business Review

**Scheduled every quarter** with stakeholders:

1. **SLO Compliance Summary**: 3-month aggregated metrics
2. **Trend Analysis**: Performance trends, regression detection
3. **Incident Review**: Root cause analysis of any outages
4. **Capacity Forecast**: Projected resource needs for next quarter
5. **Roadmap Impact**: How roadmap changes may affect SLOs

### Annual Review

**Annual audit and refresh** (before each calendar year):

1. **SLO Refresh**: Update targets based on technology changes
2. **Dependency Review**: Evaluate new versions, performance characteristics
3. **Compliance Certification**: Verify adherence to documented standards
4. **Documentation Update**: Keep this document current

---

## 10. SLO Exemptions and Escalation

### Scheduled Maintenance

**Allowed Maintenance Windows** (exempted from SLO):

- **Frequency**: Max 1 per month
- **Duration**: Max 1 hour per window
- **Notice**: Minimum 72 hours advance notice
- **Impact**: Full read/write downtime acceptable during window

**Common Maintenance Tasks**:
- FraiseQL version upgrade
- Database schema migration
- Redis cache rebuild
- Certificate rotation
- OS security patches

### Emergency Escalation

**When to Escalate**:
1. **If incident not resolved in 15 minutes**: Escalate to tier 2 (senior engineer)
2. **If incident not resolved in 1 hour**: Escalate to tier 3 (engineering manager)
3. **If incident not resolved in 4 hours**: Escalate to CTO + customer success

**Escalation Path**:
```
On-Call Engineer (Tier 1)
  ↓ (unresolved after 15 min)
Senior Engineer (Tier 2)
  ↓ (unresolved after 1 hour)
Engineering Manager (Tier 3)
  ↓ (unresolved after 4 hours)
CTO + Customer Success Director
```

---

## 11. Performance Testing and Benchmarking

### Baseline Performance Tests

**Executed monthly** to detect regressions:

```bash
# Simple query load test (100 concurrent, 60 seconds)
fraiseql-bench --query simple --concurrency 100 --duration 60s

# Expected results:
# Throughput:    2,100+ rps
# P50 latency:   < 5ms
# P95 latency:   < 25ms
# P99 latency:   < 100ms
# Error rate:    < 0.1%

# Moderate query load test
fraiseql-bench --query moderate --concurrency 50 --duration 60s

# Expected results:
# Throughput:    1,500+ rps
# P50 latency:   < 10ms
# P95 latency:   < 50ms
# P99 latency:   < 200ms
# Error rate:    < 0.1%

# Complex query load test
fraiseql-bench --query complex --concurrency 20 --duration 60s

# Expected results:
# Throughput:    500+ rps
# P50 latency:   < 50ms
# P95 latency:   < 150ms
# P99 latency:   < 500ms
# Error rate:    < 0.1%
```

### Regression Testing

**CI/CD Pipeline Integration**:

- Every PR run benchmarks against baseline
- Flag if P99 latency increases > 5%
- Flag if throughput decreases > 5%
- Block merge if regression exceeds threshold

### Load Test Scenarios

**Production Scenario 1: Normal Day**
```
3,000 simple queries/sec
500 moderate queries/sec
100 complex queries/sec
Expected: All P99 < SLO
```

**Production Scenario 2: Traffic Spike**
```
5,000 simple queries/sec (70% increase)
800 moderate queries/sec (60% increase)
150 complex queries/sec (50% increase)
Expected: P99 < SLO × 1.2, no 5xx errors
```

**Production Scenario 3: Database Slow**
```
Database latency degraded from 5ms to 50ms
Expected: P99 < SLO × 1.5, circuit breaker engages
```

---

## 12. FAQ and Troubleshooting

### Q: Why are latency SLOs based on query type, not a single number?

**A**: FraiseQL compiles GraphQL to SQL at build time. A simple 1-table query (compiled to `SELECT * FROM users WHERE id = $1`) has completely different performance characteristics than a complex 5-table join with aggregation. Measuring against a single latency number would be meaningless; it's like measuring a car by only its 0-60 time and ignoring top speed and fuel economy.

### Q: What counts toward the 99.9% availability SLO?

**A**: Only successful GraphQL query responses count. This includes:
- 200 OK with valid GraphQL response
- Properly formatted 4xx errors (auth failures, validation)
- Properly formatted 5xx errors with retryable flag

NOT included:
- Network timeouts (client-side issue)
- TLS handshake failures (certificate issue)
- DDoS attacks (infrastructure issue)
- Customer misconfiguration (their responsibility)

### Q: Can I get SLO credits if an incident was caused by my database being slow?

**A**: No. FraiseQL's SLO commits to responding correctly to requests. If your database is slow, FraiseQL will:
1. Return query results correctly (just slowly)
2. Serve from cache if available
3. Return 503 if database is completely unavailable (and cache miss)

Database performance is YOUR responsibility (RDS, managed PostgreSQL, etc.). We recommend:
- Use managed database services with their own SLA
- Monitor database metrics separately
- Configure query timeouts appropriate to your database performance

### Q: What's the difference between P99 and P99.9 latency?

**A**:
- **P99**: 99 out of 100 requests complete faster than this time
- **P99.9**: 999 out of 1000 requests complete faster than this time

For example, if P99 = 200ms and P99.9 = 1000ms:
- 1 in 100 requests takes > 200ms (normal outliers)
- 1 in 1000 requests takes > 1000ms (rare slowdowns)

P99.9 targets are less strict because they measure extreme outliers caused by external factors (GC pauses, network blips).

### Q: How do I optimize for FraiseQL's SLOs?

**A**: Follow this optimization pyramid (bottom to top):

```
     Peak Performance
    ↑
    Distributed Cache (Redis)
    ↑
    Query Caching (LRU)
    ↑
    Automatic Persisted Queries
    ↑
    Schema Compilation
    ↑
    Database Indexes ← Start here
```

Each layer builds on the previous. Database indexes are essential; caching is optional.

### Q: My throughput is lower than the stated SLO. What's wrong?

**A**: Common causes (in order):

1. **Small query batch size** (< 10 queries/batch)
   - Solution: Batch queries using GraphQL aliases

2. **Complex queries** (5+ joins)
   - Solution: Use simpler queries or cache computed fields

3. **Database slow** (> 50ms per query)
   - Solution: Optimize database, add indexes, consider read replicas

4. **Insufficient connection pool**
   - Solution: Increase `fraiseql.server.connection_pool_size` to 50-100

5. **Single-threaded test client**
   - Solution: Use load testing tool with concurrent connections

6. **Insufficient hardware**
   - Solution: Allocate more CPU cores or RAM

Run `fraiseql-bench` to identify the bottleneck.

### Q: What's the maximum number of concurrent connections?

**A**: Default is 10,000. This is a hard limit to prevent resource exhaustion. To change:

```toml
[fraiseql.server]
max_connections = 20000  # Be careful with this!
```

**Warning**: Each connection consumes memory and file descriptors. Operating system file descriptor limits (`ulimit -n`) must be increased accordingly.

Rule of thumb: Max connections ≈ (available RAM in MB) / 2

---

## Appendix: Useful Links and Commands

### Monitoring Setup

```bash
# Install Prometheus + Grafana
docker run -d --name prometheus prom/prometheus --config.file=/etc/prometheus/prometheus.yml
docker run -d --name grafana grafana/grafana

# Configure Prometheus to scrape FraiseQL metrics
# prometheus.yml:
# scrape_configs:
#   - job_name: 'fraiseql'
#     static_configs:
#       - targets: ['localhost:8001']  # Assuming metrics on port 8001
#     scrape_interval: 15s
```

### Load Testing

```bash
# Using Apache Bench (simple)
ab -n 10000 -c 100 http://localhost:8000/graphql

# Using wrk (sophisticated)
wrk --script load.lua --connections 100 --threads 4 --duration 60s http://localhost:8000/graphql

# Using FraiseQL's built-in bench tool
fraiseql-bench --config bench.toml

# Manual cURL test
curl -X POST http://localhost:8000/graphql \
  -H "Content-Type: application/json" \
  -d '{"query": "{ user(id: 1) { name email } }"}'
```

### Health Check

```bash
# Simple health check (should return 200 OK)
curl http://localhost:8000/health

# Detailed readiness check
curl http://localhost:8000/readiness

# Metrics endpoint (Prometheus format)
curl http://localhost:8000/metrics | grep fraiseql_
```

### Debugging High Latency

```bash
# Enable debug logging
RUST_LOG=debug fraiseql-server

# Check database connection pool
curl http://localhost:8000/metrics | grep fraiseql_db_connections

# Check cache hit rate
curl http://localhost:8000/metrics | grep fraiseql_cache_hit_ratio

# Sample slow queries (if enabled)
curl http://localhost:8000/metrics | grep fraiseql_slow_queries
```

---

## Document Control

| Version | Date | Author | Changes |
|---------|------|--------|---------|
| 1.0 | 2025-06-01 | FraiseQL Team | Initial SLA/SLO |
| 2.0 | 2026-02-19 | FraiseQL Team | v2.0.0-beta.2 update, detailed monitoring/degradation |

---

## Contact and Support

For questions about SLA/SLO:
- **Email**: sla@fraiseql.dev
- **Slack**: #sla-questions (internal) or support@fraiseql.dev (external)
- **Issues**: https://github.com/fraiseql/fraiseql/issues/label/sla

For on-call incident response:
- **PagerDuty**: fraiseql-incidents (prod) / fraiseql-staging (staging)
- **War Room**: https://zoom.fraiseql.dev/war-room

---

**Last Updated**: February 19, 2026
**Next Review**: February 19, 2027
