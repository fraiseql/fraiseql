# FraiseQL Grafana Dashboards

Production-ready Grafana dashboards for monitoring FraiseQL applications with PostgreSQL-native observability.

## Overview

This directory contains 5 comprehensive Grafana dashboards that provide complete observability for FraiseQL applications:

1. **Error Monitoring** - Track errors, resolution status, and affected users
2. **Performance Metrics** - Request rates, latency percentiles, and slow operations
3. **Cache Hit Rate** - Cache effectiveness and performance savings
4. **Database Pool** - Connection pool health and query performance
5. **APQ Effectiveness** - Automatic Persisted Queries performance and bandwidth savings

## Quick Start

### Prerequisites

- Grafana 9.0+ installed and running
- PostgreSQL datasource configured in Grafana
- FraiseQL application with observability enabled

### Automatic Import

Run the import script to automatically install all dashboards:

```bash
cd grafana/

# Using default Grafana settings (localhost:3000, admin/admin)
./import_dashboards.sh

# Or with custom settings
GRAFANA_URL=https://grafana.mycompany.com \
GRAFANA_USER=admin \
GRAFANA_PASSWORD=secret \
./import_dashboards.sh
```

The script will:
- ✅ Create a "FraiseQL" folder in Grafana
- ✅ Import all 5 dashboards
- ✅ Configure dashboard settings
- ✅ Provide direct links to each dashboard

### Manual Import

If you prefer to import dashboards manually:

1. Open Grafana UI
2. Go to **Dashboards → Import**
3. Upload each `.json` file from this directory
4. Select your PostgreSQL datasource
5. Click **Import**

## Dashboard Details

### 1. Error Monitoring Dashboard

**File**: `error_monitoring.json`

**Panels**:
- Error rate over time (timeseries)
- Error distribution by type (pie chart)
- Top 10 error fingerprints (table)
- Error resolution status (stat)
- Errors by environment (bar gauge)
- Recent errors (table)
- Users affected by errors (timeseries)

**Key Queries**:
```sql
-- Error rate over time
SELECT
  date_trunc('minute', occurred_at) as time,
  COUNT(*) as error_count
FROM monitoring.errors
WHERE occurred_at >= $__timeFrom()
  AND occurred_at <= $__timeTo()
  AND environment = '$environment'
GROUP BY time;

-- Top error fingerprints
SELECT
    fingerprint,
    exception_type,
    message,
    COUNT(*) as occurrences,
    MAX(occurred_at) as last_seen,
    COUNT(DISTINCT context->>'user_id') as affected_users
FROM monitoring.errors
WHERE occurred_at > NOW() - INTERVAL '24 hours'
  AND resolved_at IS NULL
GROUP BY fingerprint, exception_type, message
ORDER BY occurrences DESC
LIMIT 10;
```

**Use Cases**:
- Monitor production error rates
- Identify frequently occurring errors
- Track error resolution progress
- Analyze user impact of errors

---

### 2. Performance Metrics Dashboard

**File**: `performance_metrics.json`

**Panels**:
- Request rate (req/sec) (timeseries)
- Response time percentiles (P50, P95, P99) (timeseries)
- Slowest operations table
- Database query performance table
- Trace status distribution (pie chart)
- Requests by operation (bar gauge)
- Error rate by operation (timeseries)
- Average response time (stat)

**Key Queries**:
```sql
-- Request rate
SELECT
  date_trunc('minute', start_time) as time,
  COUNT(*) / 60.0 as requests_per_second
FROM monitoring.traces
WHERE start_time >= $__timeFrom()
  AND start_time <= $__timeTo()
GROUP BY time;

-- P95 latency
SELECT
  date_trunc('minute', start_time) as time,
  percentile_cont(0.95) WITHIN GROUP (ORDER BY duration_ms) as p95_latency
FROM monitoring.traces
WHERE start_time >= $__timeFrom()
  AND start_time <= $__timeTo()
GROUP BY time;

-- Slowest operations
SELECT
    operation_name,
    COUNT(*) as request_count,
    percentile_cont(0.99) WITHIN GROUP (ORDER BY duration_ms) as p99_ms
FROM monitoring.traces
WHERE start_time > NOW() - INTERVAL '1 hour'
GROUP BY operation_name
HAVING COUNT(*) > 10
ORDER BY p99_ms DESC
LIMIT 20;
```

**Use Cases**:
- Monitor application performance
- Identify slow operations
- Track SLA compliance (P95/P99 targets)
- Detect performance regressions

---

### 3. Cache Hit Rate Dashboard

**File**: `cache_hit_rate.json`

**Panels**:
- Overall cache hit rate (stat)
- Cache operations over time (hits/misses) (timeseries)
- Cache hit rate over time (timeseries)
- Cache performance by type (table)
- Cache savings (time saved) (stat)
- Cache operations rate (timeseries)
- Query cache vs APQ cache comparison (bar gauge)

**Key Queries**:
```sql
-- Overall hit rate
SELECT
  ROUND(100.0 * SUM(CASE WHEN labels->>'result' = 'hit' THEN metric_value ELSE 0 END) /
    NULLIF(SUM(metric_value), 0), 2) as hit_rate_percent
FROM monitoring.metrics
WHERE metric_name IN ('cache_hits_total', 'cache_misses_total')
  AND timestamp > NOW() - INTERVAL '1 hour';

-- Cache performance by type
WITH cache_stats AS (
  SELECT
    labels->>'cache_type' as cache_type,
    SUM(CASE WHEN metric_name = 'cache_hits_total' THEN metric_value ELSE 0 END) as total_hits,
    SUM(CASE WHEN metric_name = 'cache_misses_total' THEN metric_value ELSE 0 END) as total_misses
  FROM monitoring.metrics
  WHERE metric_name IN ('cache_hits_total', 'cache_misses_total')
    AND timestamp > NOW() - INTERVAL '1 hour'
  GROUP BY cache_type
)
SELECT
  cache_type,
  total_hits,
  total_misses,
  ROUND(100.0 * total_hits / NULLIF(total_hits + total_misses, 0), 2) as hit_rate_percent
FROM cache_stats;
```

**Use Cases**:
- Monitor cache effectiveness
- Optimize cache strategies
- Calculate time/cost savings
- Compare different cache types

---

### 4. Database Pool Dashboard

**File**: `database_pool.json`

**Panels**:
- Active connections (stat)
- Idle connections (stat)
- Total connections (stat)
- Connection pool over time (timeseries)
- Database query rate (timeseries)
- Query types distribution (pie chart)
- Database query duration (P50/P95) (timeseries)
- Top tables by query count (table)
- Pool utilization rate (gauge)

**Key Queries**:
```sql
-- Connection pool metrics
SELECT
  metric_value as active_connections
FROM monitoring.metrics
WHERE metric_name = 'db_connections_active'
ORDER BY timestamp DESC
LIMIT 1;

-- Query rate by type
SELECT
  date_trunc('minute', timestamp) as time,
  labels->>'query_type' as query_type,
  SUM(metric_value) / 60.0 as queries_per_second
FROM monitoring.metrics
WHERE metric_name = 'db_queries_total'
  AND timestamp >= $__timeFrom()
  AND timestamp <= $__timeTo()
GROUP BY time, query_type;

-- Pool utilization
SELECT
  ROUND(100.0 * active / NULLIF(total, 0), 2) as utilization_percent
FROM (
  SELECT
    (SELECT metric_value FROM monitoring.metrics
     WHERE metric_name = 'db_connections_active'
     ORDER BY timestamp DESC LIMIT 1) as active,
    (SELECT metric_value FROM monitoring.metrics
     WHERE metric_name = 'db_connections_total'
     ORDER BY timestamp DESC LIMIT 1) as total
) pool_stats;
```

**Use Cases**:
- Monitor connection pool health
- Detect connection pool exhaustion
- Optimize pool size configuration
- Identify high-volume tables

---

### 5. APQ Effectiveness Dashboard

**File**: `apq_effectiveness.json`

**Panels**:
- APQ hit rate (stat)
- Total APQ requests (stat)
- Bandwidth saved (stat)
- APQ operations over time (timeseries)
- APQ hit rate over time (timeseries)
- Stored persisted queries (stat)
- APQ storage growth (timeseries)
- Top persisted queries by usage (table)
- APQ request types (pie chart)
- Bandwidth savings over time (timeseries)

**Key Queries**:
```sql
-- APQ hit rate
WITH apq_stats AS (
  SELECT
    SUM(CASE WHEN labels->>'cache_type' = 'apq'
        AND metric_name = 'cache_hits_total' THEN metric_value ELSE 0 END) as hits,
    SUM(CASE WHEN labels->>'cache_type' = 'apq'
        AND metric_name = 'cache_misses_total' THEN metric_value ELSE 0 END) as misses
  FROM monitoring.metrics
  WHERE metric_name IN ('cache_hits_total', 'cache_misses_total')
    AND timestamp > NOW() - INTERVAL '24 hours'
)
SELECT
  ROUND(100.0 * hits / NULLIF(hits + misses, 0), 2) as hit_rate_percent
FROM apq_stats;

-- Bandwidth saved (assuming ~2KB per query)
SELECT
  SUM(metric_value) * 2048 / 1048576.0 as mb_saved
FROM monitoring.metrics
WHERE metric_name = 'cache_hits_total'
  AND labels->>'cache_type' = 'apq'
  AND timestamp > NOW() - INTERVAL '24 hours';

-- Top persisted queries
SELECT
  au.query_hash,
  LEFT(pq.query, 100) as query_preview,
  au.usage_count
FROM (
  SELECT
    labels->>'query_hash' as query_hash,
    SUM(metric_value) as usage_count
  FROM monitoring.metrics
  WHERE metric_name = 'cache_hits_total'
    AND labels->>'cache_type' = 'apq'
    AND timestamp > NOW() - INTERVAL '24 hours'
  GROUP BY query_hash
) au
LEFT JOIN tb_persisted_query pq ON au.query_hash = pq.hash
ORDER BY au.usage_count DESC
LIMIT 20;
```

**Use Cases**:
- Monitor APQ adoption and effectiveness
- Calculate bandwidth savings
- Identify most-used persisted queries
- Optimize client query strategies

---

## Configuration

### Environment Variables

All dashboards include an `environment` template variable for filtering data:

- **production** - Production environment
- **staging** - Staging environment
- **development** - Development environment

To change the environment:

1. Open dashboard
2. Click dropdown at top (default: "production")
3. Select desired environment

### Time Ranges

Default time ranges:

- **Error Monitoring**: Last 24 hours
- **Performance Metrics**: Last 1 hour
- **Cache Hit Rate**: Last 1 hour
- **Database Pool**: Last 1 hour
- **APQ Effectiveness**: Last 24 hours

All dashboards support custom time ranges via Grafana's time picker.

### Refresh Rates

- **Error Monitoring**: 30 seconds
- **Performance Metrics**: 30 seconds
- **Cache Hit Rate**: 30 seconds
- **Database Pool**: 10 seconds (faster for real-time monitoring)
- **APQ Effectiveness**: 30 seconds

## PostgreSQL Datasource Setup

### Create Datasource

1. Go to **Configuration → Data Sources → Add data source**
2. Select **PostgreSQL**
3. Configure settings:

```
Name: PostgreSQL
Host: your-postgres-host:5432
Database: your-database-name
User: grafana_readonly  (recommended)
Password: ***
SSL Mode: require (for production)
Version: 14+ (or your PostgreSQL version)
```

### Create Read-Only User (Recommended)

For security, create a dedicated read-only user for Grafana:

```sql
-- Create read-only user
CREATE USER grafana_readonly WITH PASSWORD 'secure_password';

-- Grant connection
GRANT CONNECT ON DATABASE your_database TO grafana_readonly;

-- Grant schema usage
GRANT USAGE ON SCHEMA monitoring TO grafana_readonly;

-- Grant SELECT on monitoring tables
GRANT SELECT ON ALL TABLES IN SCHEMA monitoring TO grafana_readonly;

-- Auto-grant SELECT on future tables
ALTER DEFAULT PRIVILEGES IN SCHEMA monitoring
    GRANT SELECT ON TABLES TO grafana_readonly;

-- If using tb_persisted_query for APQ dashboard
GRANT SELECT ON tb_persisted_query TO grafana_readonly;
```

### Test Connection

After configuration, click **Save & Test** to verify:
- ✅ Database connection successful
- ✅ Can execute queries
- ✅ PostgreSQL version detected

## Troubleshooting

### Dashboards Show "No Data"

**Possible causes**:

1. **Observability not enabled**
   - Verify `monitoring.errors`, `monitoring.traces`, `monitoring.metrics` tables exist
   - Check FraiseQL observability configuration

2. **Wrong environment selected**
   - Ensure environment variable matches your data
   - Check `environment` column in tables

3. **No data in time range**
   - Expand time range (e.g., last 7 days)
   - Verify application is generating data

**Debug query**:
```sql
-- Check if data exists
SELECT
  COUNT(*) as error_count,
  MAX(occurred_at) as latest_error
FROM monitoring.errors
WHERE occurred_at > NOW() - INTERVAL '7 days';
```

### Import Script Fails

**Error: "Cannot connect to Grafana"**

```bash
# Check Grafana is running
curl http://localhost:3000/api/health

# Verify credentials
GRAFANA_USER=admin GRAFANA_PASSWORD=your_password ./import_dashboards.sh
```

**Error: "PostgreSQL datasource not found"**

```bash
# Create datasource first via Grafana UI, or set env vars:
POSTGRES_HOST=localhost:5432 \
POSTGRES_DB=myapp \
POSTGRES_USER=grafana_readonly \
POSTGRES_PASSWORD=password \
./import_dashboards.sh
```

### Query Performance Issues

If dashboard queries are slow (>2 seconds):

1. **Check indexes** (should be created by FraiseQL schema):
   ```sql
   -- Verify indexes exist
   SELECT indexname, tablename
   FROM pg_indexes
   WHERE schemaname = 'monitoring';
   ```

2. **Enable query optimization**:
   ```sql
   -- Analyze tables for better query plans
   ANALYZE monitoring.errors;
   ANALYZE monitoring.traces;
   ANALYZE monitoring.metrics;
   ```

3. **Consider table partitioning** (for high-volume data):
   - See `docs/production/observability.md` for partition setup

## Customization

### Adding Custom Panels

1. Open dashboard in Grafana
2. Click **Add panel**
3. Write SQL query against `monitoring.*` tables
4. Configure visualization
5. Save dashboard

Example custom panel:
```sql
-- Custom: Errors by user role
SELECT
  context->>'user_role' as role,
  COUNT(*) as error_count
FROM monitoring.errors
WHERE occurred_at > NOW() - INTERVAL '24 hours'
  AND context->>'user_role' IS NOT NULL
GROUP BY role
ORDER BY error_count DESC;
```

### Alerting

Set up Grafana alerts for critical metrics:

1. **High Error Rate**:
   ```sql
   SELECT COUNT(*) as error_count
   FROM monitoring.errors
   WHERE occurred_at > NOW() - INTERVAL '5 minutes'
     AND resolved_at IS NULL;
   ```
   Alert if: `error_count > 100`

2. **Low Cache Hit Rate**:
   ```sql
   SELECT
     100.0 * hits / NULLIF(hits + misses, 0) as hit_rate
   FROM (
     SELECT
       SUM(CASE WHEN metric_name = 'cache_hits_total' THEN metric_value ELSE 0 END) as hits,
       SUM(CASE WHEN metric_name = 'cache_misses_total' THEN metric_value ELSE 0 END) as misses
     FROM monitoring.metrics
     WHERE timestamp > NOW() - INTERVAL '5 minutes'
   ) stats;
   ```
   Alert if: `hit_rate < 50`

3. **Pool Exhaustion**:
   ```sql
   SELECT
     100.0 * active / NULLIF(total, 0) as utilization
   FROM (
     SELECT
       (SELECT metric_value FROM monitoring.metrics
        WHERE metric_name = 'db_connections_active'
        ORDER BY timestamp DESC LIMIT 1) as active,
       (SELECT metric_value FROM monitoring.metrics
        WHERE metric_name = 'db_connections_total'
        ORDER BY timestamp DESC LIMIT 1) as total
   ) pool;
   ```
   Alert if: `utilization > 90`

## Best Practices

1. **Use read-only database user** for Grafana (security)
2. **Set appropriate refresh rates** (balance freshness vs database load)
3. **Enable Grafana alerting** for critical metrics
4. **Create dedicated dashboard folder** for organization
5. **Document custom modifications** for team knowledge sharing
6. **Test dashboards in staging** before production deployment
7. **Monitor dashboard query performance** via Grafana query inspector

## Cost Comparison

**PostgreSQL-native observability** (FraiseQL + Grafana):
- **Cost**: $0 (self-hosted) or ~$50-100/month (managed Grafana)
- **Data retention**: Unlimited (configurable)
- **Query flexibility**: Full SQL

**External APM** (Datadog, New Relic, etc.):
- **Cost**: $500-5,000/month
- **Data retention**: Limited by plan (typically 15-90 days)
- **Query flexibility**: Limited query language

**Savings**: $6,000-60,000 per year with FraiseQL observability!

## Testing

FraiseQL maintains **very high quality standards**. All dashboards have comprehensive tests:

- **50 automated tests** covering JSON structure, SQL queries, and import script
- **Validates**: Correctness, performance, security, Grafana compatibility
- **Runs in**: <0.4 seconds with no external dependencies

```bash
# Run all dashboard tests
uv run pytest tests/grafana/ -v

# Expected: 50 passed, 1 skipped in 0.38s
```

See `tests/grafana/README.md` for detailed testing documentation.

## Support

- **Documentation**: See `docs/production/observability.md` for detailed observability setup
- **Tests**: See `tests/grafana/README.md` for testing guide
- **GitHub Issues**: Report dashboard issues at https://github.com/your-org/fraiseql/issues
- **Grafana Docs**: https://grafana.com/docs/

## License

MIT License - See LICENSE file for details

---

**Last Updated**: October 11, 2025
**FraiseQL Version**: 0.11.0+
**Grafana Version**: 9.0+
**Test Coverage**: 50 tests (JSON, SQL, Scripts)
