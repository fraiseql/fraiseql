# Performance Monitoring & Operations

Production monitoring, alerting, and operational runbooks for FraiseQL performance management.

> **For comprehensive monitoring and observability**, see the [Observability Guide](./observability.md) and [Production Monitoring Guide](./monitoring.md) which cover error tracking, metrics, alerting, and integrated observability architecture.

## Overview

Effective performance monitoring is crucial for maintaining optimal FraiseQL application performance. This guide covers monitoring setup, key metrics, alerting strategies, and operational procedures.

## Key Performance Metrics

### Response Time Metrics

```promql
# Query response time percentiles
histogram_quantile(0.95, rate(fraiseql_query_duration_seconds_bucket[5m]))
histogram_quantile(0.99, rate(fraiseql_query_duration_seconds_bucket[5m]))

# Cache performance
rate(fraiseql_cache_hits_total[5m]) / rate(fraiseql_cache_requests_total[5m])
```

### Database Metrics

```promql
# Connection pool utilization
fraiseql_db_connection_pool_active / fraiseql_db_connection_pool_total

# Query execution time
rate(fraiseql_db_query_duration_seconds_sum[5m])
/ rate(fraiseql_db_query_duration_seconds_count[5m])

# Slow query count
rate(fraiseql_db_slow_queries_total[5m])
```

### System Resources

```promql
# Memory usage
process_resident_memory_bytes / 1024 / 1024  # MB

# CPU utilization
rate(process_cpu_user_seconds_total[5m]) * 100

# Database connections
pg_stat_activity_count{state="active"}
```

## Monitoring Setup

### Prometheus Configuration

```yaml
# prometheus.yml
scrape_configs:
  - job_name: 'fraiseql'
    static_configs:
      - targets: ['localhost:8000']
    metrics_path: '/metrics'
    scrape_interval: 15s

  - job_name: 'postgres'
    static_configs:
      - targets: ['localhost:5432']
    scrape_interval: 30s
```

### Grafana Dashboards

**Core Performance Dashboard:**
- Query response time (P50, P95, P99)
- Cache hit rate over time
- Database connection pool utilization
- Error rate by endpoint
- Throughput (requests/second)

**Database Performance Dashboard:**
- Query execution time by type
- Index usage statistics
- Table bloat monitoring
- Vacuum and analyze status
- Lock contention monitoring

## Alerting Strategy

### Critical Alerts (Immediate Response)

```yaml
# High error rate
alert: HighErrorRate
expr: rate(fraiseql_errors_total[5m]) / rate(fraiseql_requests_total[5m]) > 0.05
for: 2m
labels:
  severity: critical

# Database connection pool exhausted
alert: DBConnectionPoolExhausted
expr: fraiseql_db_connection_pool_active / fraiseql_db_connection_pool_total > 0.95
for: 1m
labels:
  severity: critical
```

### Warning Alerts (Investigate)

```yaml
# High latency
alert: HighLatency
expr: histogram_quantile(0.95, rate(fraiseql_query_duration_seconds_bucket[5m])) > 2
for: 5m
labels:
  severity: warning

# Low cache hit rate
alert: LowCacheHitRate
expr: rate(fraiseql_cache_hits_total[5m]) / rate(fraiseql_cache_requests_total[5m]) < 0.7
for: 10m
labels:
  severity: warning
```

### Info Alerts (Monitor Trends)

```yaml
# Increasing memory usage
alert: MemoryUsageIncreasing
expr: increase(process_resident_memory_bytes[1h]) > 100 * 1024 * 1024  # 100MB/hour
for: 0m
labels:
  severity: info
```

## Operational Runbooks

### Database Performance Degradation

**Detection:**
- GraphQL queries taking >5 seconds
- Database connection pool utilization >80%
- Query timeout errors increasing

**Immediate Actions:**

1. **Check Active Connections**
   ```sql
   SELECT count(*) as active_connections FROM pg_stat_activity
   WHERE state = 'active';
   ```

2. **Identify Slow Queries**
   ```sql
   SELECT pid, now() - query_start as duration, query
   FROM pg_stat_activity
   WHERE state = 'active' AND now() - query_start > interval '30 seconds'
   ORDER BY duration DESC;
   ```

3. **Connection Pool Configuration**
   ```python
   # Temporary increase for immediate relief
   pool = DatabasePool(
       dsn=database_url,
       min_size=20,      # Increased from 10
       max_size=100,     # Increased from 50
       max_idle_time=60  # Reduced from 300
   )
   ```

**Root Cause Analysis:**

1. **Index Issues**
   ```sql
   -- Check for missing indexes
   SELECT schemaname, tablename, seq_scan, seq_tup_read
   FROM pg_stat_user_tables
   WHERE seq_scan > 1000
   ORDER BY seq_tup_read DESC;
   ```

2. **Query Optimization**
   ```sql
   -- Enable query logging temporarily
   ALTER SYSTEM SET log_statement = 'all';
   ALTER SYSTEM SET log_duration = ON;
   SELECT pg_reload_conf();
   ```

3. **Resource Contention**
   - Check CPU, memory, and I/O utilization
   - Monitor for lock contention
   - Verify autovacuum is running

### Cache Performance Issues

**Detection:**
- Cache hit rate drops below 70%
- Increased response times for cached queries
- Cache size growing rapidly

**Diagnosis:**

1. **Cache Hit Rate Analysis**
   ```python
   # Check cache statistics
   stats = await cache.get_stats()
   print(f"Hit rate: {stats.hit_rate:.1%}")
   print(f"Total requests: {stats.total_requests}")
   print(f"Eviction rate: {stats.evictions}")
   ```

2. **Cache Key Analysis**
   ```sql
   -- Most frequent cache keys
   SELECT cache_key, COUNT(*) as hits
   FROM fraiseql_cache
   GROUP BY cache_key
   ORDER BY hits DESC
   LIMIT 10;
   ```

3. **TTL Optimization**
   ```python
   # Adjust TTL based on data freshness requirements
   cache = ResultCache(
       backend=postgres_cache,
       default_ttl=600,  # Increased from 300
       max_size_mb=256   # Reduced if memory pressure
   )
   ```

### Memory Pressure

**Detection:**
- Application memory usage >80% of available RAM
- Increased garbage collection frequency
- Out of memory errors

**Resolution:**

1. **Cache Size Reduction**
   ```python
   cache = ResultCache(
       backend=postgres_cache,
       max_size_mb=128  # Reduced from 256
   )
   ```

2. **Connection Pool Optimization**
   ```python
   pool = DatabasePool(
       max_size=50,      # Reduced from 100
       max_idle_time=60  # Reduced from 300
   )
   ```

3. **Memory Leak Investigation**
   ```python
   import tracemalloc

   tracemalloc.start()
   # Run application for monitoring period
   current, peak = tracemalloc.get_traced_memory()
   print(f"Current: {current / 1024 / 1024:.1f} MB")
   print(f"Peak: {peak / 1024 / 1024:.1f} MB")
   ```

## Capacity Planning

### Performance Baselines

| Workload Type | Target P95 Latency | Target Throughput | Cache Hit Rate |
|---------------|-------------------|-------------------|----------------|
| Simple CRUD | <50ms | 1,000 req/sec | 90% |
| Complex Queries | <200ms | 500 req/sec | 85% |
| Analytics | <500ms | 100 req/sec | 70% |
| Real-time | <20ms | 2,000 req/sec | 95% |

### Scaling Guidelines

**Vertical Scaling:**
- Add CPU cores for increased query parallelism
- Increase RAM for larger cache sizes
- Use faster storage for database performance

**Horizontal Scaling:**
- Multiple application instances behind load balancer
- Read replicas for query distribution
- Sharded databases for extreme scale

### Database Optimization

```sql
-- Autovacuum tuning for high-write workloads
ALTER SYSTEM SET autovacuum_max_workers = 6;
ALTER SYSTEM SET autovacuum_naptime = '20s';
ALTER SYSTEM SET autovacuum_vacuum_threshold = 50;
ALTER SYSTEM SET autovacuum_analyze_threshold = 50;

-- Work memory for complex queries
ALTER SYSTEM SET work_mem = '128MB';
ALTER SYSTEM SET maintenance_work_mem = '512MB';

SELECT pg_reload_conf();
```

## Health Checks

### Application Health

```python
from fastapi import APIRouter
from fraiseql.health import HealthChecker

router = APIRouter()
health_checker = HealthChecker()

@router.get("/health")
async def health_check():
    """Comprehensive health check endpoint."""
    results = await health_checker.run_checks()

    return {
        "status": "healthy" if all(r.passed for r in results) else "unhealthy",
        "checks": {
            "database": results.db_status,
            "cache": results.cache_status,
            "memory": results.memory_status,
            "connections": results.connection_status
        },
        "timestamp": datetime.utcnow().isoformat()
    }
```

### Database Health

```sql
-- Connection health check
SELECT 1 as health_check;

-- Replication lag (if using replicas)
SELECT
    client_addr,
    state,
    sent_lsn,
    write_lsn,
    flush_lsn,
    replay_lsn,
    pg_wal_lsn_diff(sent_lsn, replay_lsn) as lag_bytes
FROM pg_stat_replication;

-- Table bloat check
SELECT
    schemaname, tablename,
    n_dead_tup, n_live_tup,
    ROUND(n_dead_tup::float / (n_live_tup + n_dead_tup) * 100, 2) as bloat_ratio
FROM pg_stat_user_tables
WHERE n_dead_tup > 1000
ORDER BY bloat_ratio DESC;
```

### Dependency Health

```python
# External service health checks
async def check_external_services():
    """Check health of external dependencies."""
    checks = []

    # Redis cache
    try:
        await redis.ping()
        checks.append({"service": "redis", "status": "healthy"})
    except Exception as e:
        checks.append({"service": "redis", "status": "unhealthy", "error": str(e)})

    # Email service
    try:
        await email_service.health_check()
        checks.append({"service": "email", "status": "healthy"})
    except Exception as e:
        checks.append({"service": "email", "status": "unhealthy", "error": str(e)})

    return checks
```

## Incident Response

### Severity Levels

**SEV-1 (Critical):**
- Complete service outage
- Data corruption
- Security breach
- Response: Immediate, all-hands

**SEV-2 (High):**
- Significant performance degradation (>50% impact)
- Partial service outage
- Response: Within 15 minutes

**SEV-3 (Medium):**
- Minor performance issues
- Intermittent failures
- Response: Within 1 hour

### Escalation Process

1. **Detection** - Monitoring alerts trigger
2. **Triage** - On-call engineer assesses severity
3. **Investigation** - Root cause analysis begins
4. **Communication** - Stakeholders notified
5. **Resolution** - Fix implemented and deployed
6. **Post-mortem** - Incident review and prevention measures

### Communication Template

```
INCIDENT: [Brief Description]

Status: [Investigating/Resolving/Resolved]
Impact: [Affected users/services]
Timeline: [Detection → Triage → Resolution]
Root Cause: [Technical cause]
Resolution: [Fix implemented]
Prevention: [Future measures]
```

## Maintenance Procedures

### Regular Performance Reviews

**Weekly:**
- Review cache hit rates and adjust TTL as needed
- Check for new slow queries and optimize
- Monitor resource utilization trends

**Monthly:**
- Performance benchmark against previous month
- Database index analysis and cleanup
- Application dependency updates

### Database Maintenance

```sql
-- Regular vacuum and analyze
VACUUM ANALYZE;

-- Reindex if needed
REINDEX TABLE users;
REINDEX TABLE posts;

-- Update statistics
ANALYZE;
```

### Cache Maintenance

```python
# Periodic cache cleanup
async def maintenance_cleanup():
    """Weekly cache maintenance."""
    # Remove expired entries
    await cache.cleanup_expired()

    # Analyze cache usage patterns
    stats = await cache.get_stats()
    if stats.hit_rate < 0.8:
        # Investigate and optimize
        await analyze_cache_usage()
```

## Next Steps

- [Performance Tuning Guide](../guides/performance-tuning.md) - Developer optimization strategies
- [Alerting Configuration](../production/alerting.md) - Detailed alerting setup
- [Capacity Planning](../production/capacity-planning.md) - Long-term scaling strategies
- [Incident Response](../production/incident-response.md) - Detailed incident procedures
