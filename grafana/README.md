# FraiseQL Grafana Dashboards

This directory contains Grafana dashboard configurations for monitoring FraiseQL applications.

## Dashboards

1. **Error Monitoring** (`dashboards/error-monitoring.json`) - Track application errors
2. **OpenTelemetry Traces** (`dashboards/opentelemetry-traces.json`) - Distributed tracing
3. **Performance Metrics** (`dashboards/performance-metrics.json`) - Application performance

## Quick Setup

### 1. Install Grafana

```bash
# Docker
docker run -d -p 3000:3000 --name=grafana grafana/grafana

# Or use your cloud provider's managed Grafana
```

### 2. Add PostgreSQL Data Source

1. Open Grafana (http://localhost:3000)
2. Go to Configuration → Data Sources
3. Add PostgreSQL data source:
   - Host: `your-postgres-host:5432`
   - Database: `your-database`
   - User: `your-user`
   - Password: `your-password`
   - TLS Mode: `require` (for production)

### 3. Import Dashboards

1. Go to Dashboards → Import
2. Upload JSON files from `dashboards/` directory
3. Select your PostgreSQL data source
4. Click Import

## Dashboard Overview

### Error Monitoring

**Panels:**
- Active Errors (last 24h)
- Error Rate Trend
- Top Errors by Occurrence
- Errors by Severity
- Errors by Environment
- Recent Error Timeline
- Error Resolution Time

**Use Cases:**
- Monitor application health
- Identify critical issues
- Track error trends
- Prioritize bug fixes

### OpenTelemetry Traces

**Panels:**
- Request Rate
- P95/P99 Latency
- Slow Traces (top 10)
- Trace Count by Operation
- Error Rate by Service
- Trace Duration Histogram

**Use Cases:**
- Identify slow operations
- Track service dependencies
- Optimize performance bottlenecks
- Monitor distributed systems

### Performance Metrics

**Panels:**
- Request Throughput
- Response Time Distribution
- Database Query Performance
- Cache Hit Rate
- CPU/Memory Usage (via OpenTelemetry)

**Use Cases:**
- Capacity planning
- Performance optimization
- Resource utilization tracking

## Custom Queries

You can create custom panels using SQL queries against the PostgreSQL tables:

### Example: Error Rate by Hour

```sql
SELECT
  date_trunc('hour', occurred_at) AS time,
  COUNT(*) as error_count
FROM tb_error_occurrence
WHERE occurred_at > NOW() - INTERVAL '24 hours'
GROUP BY time
ORDER BY time
```

### Example: Slowest Endpoints

```sql
SELECT
  operation_name,
  PERCENTILE_CONT(0.95) WITHIN GROUP (ORDER BY duration_ms) as p95_ms,
  COUNT(*) as request_count
FROM otel_traces
WHERE start_time > NOW() - INTERVAL '1 hour'
GROUP BY operation_name
ORDER BY p95_ms DESC
LIMIT 10
```

### Example: Error Frequency by Type

```sql
SELECT
  error_type,
  COUNT(*) as error_count,
  MAX(last_seen) as last_occurrence
FROM tb_error_log
WHERE status = 'unresolved'
  AND last_seen > NOW() - INTERVAL '7 days'
GROUP BY error_type
ORDER BY error_count DESC
```

## Alerting

Configure Grafana alerts based on your dashboards:

### Example Alert: High Error Rate

- Condition: Error count > 10 in last 5 minutes
- Notification: Send to Slack/Email
- Auto-resolve: When error count < 5

### Example Alert: Slow Traces

- Condition: P95 latency > 1000ms
- Notification: Escalate to on-call
- Auto-resolve: When P95 < 500ms

## Best Practices

1. **Use Variables** - Add dashboard variables for environment, service, etc.
2. **Set Time Ranges** - Default to last 1 hour, allow customization
3. **Add Annotations** - Mark deployments, incidents, etc.
4. **Create Folders** - Organize dashboards by service/team
5. **Share Dashboards** - Export/import via JSON for version control

## Troubleshooting

### Dashboard shows no data

- Check PostgreSQL connection in data source
- Verify tables exist: `SELECT * FROM tb_error_log LIMIT 1`
- Ensure time range includes recent data
- Check query syntax in panel editor

### Slow queries

- Add indexes on frequently queried columns
- Use materialized views for complex aggregations
- Limit time range to recent data
- Consider using Grafana query caching

## Resources

- [Grafana Documentation](https://grafana.com/docs/)
- [PostgreSQL Data Source](https://grafana.com/docs/grafana/latest/datasources/postgres/)
- [Dashboard Best Practices](https://grafana.com/docs/grafana/latest/best-practices/)
