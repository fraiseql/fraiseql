# Production Case Study: Multi-Tenant SaaS Platform

> **Note**: This is an example case study demonstrating the template structure and type of metrics that should be collected. For actual production deployments, contact lionel.hamayon@evolution-digitale.fr to be featured.

## Company Information

- **Company**: [Example SaaS Company]
- **Industry**: SaaS - Project Management
- **Use Case**: Multi-tenant project management API serving web and mobile clients
- **Production Since**: March 2024
- **Team Size**: 4 backend developers
- **Contact**: [Contact available for verification]

## System Architecture

### Infrastructure
- **Hosting**: AWS (us-east-1, eu-west-1)
- **Database**: PostgreSQL 15.4 (Amazon RDS, db.r6g.xlarge)
- **Application**: FastAPI 0.109 + FraiseQL 0.11.0
- **Deployment**: Kubernetes (EKS) with 6 pods across 2 regions
- **Regions**: 2 (North America, Europe)

### FraiseQL Configuration
- **Version**: 0.11.0
- **Modules Used**:
  - [x] Core GraphQL
  - [x] PostgreSQL-native caching
  - [x] PostgreSQL-native error tracking
  - [x] Multi-tenancy (Row-Level Security)
  - [x] TurboRouter (query caching)
  - [x] APQ (Automatic Persisted Queries)

### Architecture Diagram

```
                    ┌─────────────────┐
                    │   CloudFront    │
                    │   (Global CDN)  │
                    └────────┬────────┘
                             │
              ┌──────────────┴──────────────┐
              │                             │
    ┌─────────▼─────────┐       ┌─────────▼─────────┐
    │   ALB (us-east-1) │       │   ALB (eu-west-1) │
    └─────────┬─────────┘       └─────────┬─────────┘
              │                             │
    ┌─────────▼─────────┐       ┌─────────▼─────────┐
    │  Kubernetes (EKS) │       │  Kubernetes (EKS) │
    │  3 pods × FastAPI │       │  3 pods × FastAPI │
    │    + FraiseQL     │       │    + FraiseQL     │
    └─────────┬─────────┘       └─────────┬─────────┘
              │                             │
    ┌─────────▼─────────────────────────────▼─────────┐
    │           PostgreSQL 15.4 (RDS)                  │
    │  • Core Data (logged tables)                    │
    │  • Cache (UNLOGGED tables)                      │
    │  • Error Tracking (tb_error_log)                │
    │  • Observability (otel_traces, otel_metrics)    │
    └─────────────────────────────────────────────────┘
```

## Performance Metrics

### Request Volume
- **Daily Requests**: 12.5M requests/day (average)
- **Peak Traffic**: 420 req/sec (business hours US Eastern)
- **Average Traffic**: 145 req/sec (24h average)
- **Query Types**: 78% queries, 22% mutations

### Response Times

| Metric | Value | Notes |
|--------|-------|-------|
| **P50** | 18 ms | Median response time |
| **P95** | 65 ms | 95th percentile |
| **P99** | 195 ms | 99th percentile |
| **P99.9** | 850 ms | Complex nested queries |

### Cache Performance

| Metric | Value | Notes |
|--------|-------|-------|
| **Hit Rate** | 73% | PostgreSQL UNLOGGED cache |
| **Miss Rate** | 27% | |
| **Avg Cache Latency** | 3.2 ms | Sub-millisecond for most |
| **Cache Size** | 4.8 GB | 2.1M cache entries |

### Database Performance

| Metric | Value | Notes |
|--------|-------|-------|
| **Avg Query Time** | 12 ms | Across all queries |
| **Pool Utilization** | 42% | 85/200 connections (per pod) |
| **Slow Queries** | 23/day | Queries > 1 second |
| **Database Size** | 185 GB | 140GB data + 45GB indexes + cache |

## Cost Analysis

### Before FraiseQL (Traditional Stack)

| Service | Monthly Cost | Purpose |
|---------|-------------|---------|
| Django + DRF + Strawberry GraphQL | $950 | Application layer (4 EC2 instances) |
| Redis Elasticache | $340 | Query & session caching |
| Sentry (Team Plan) | $890 | Error tracking & monitoring |
| PostgreSQL RDS | $580 | Database (db.r6g.large) |
| **Total** | **$2,760/month** | |

### After FraiseQL

| Service | Monthly Cost | Purpose |
|---------|-------------|---------|
| PostgreSQL RDS | $790 | Everything (API, cache, errors, logs) |
| EKS + Application | $620 | Kubernetes cluster + FastAPI pods |
| CloudWatch + Grafana | $65 | Metrics dashboard |
| **Total** | **$1,475/month** | |

### Cost Savings

- **Monthly Savings**: $1,285/month (46.5% reduction)
- **Annual Savings**: $15,420/year
- **Eliminated Services**:
  - Redis Elasticache: Replaced with PostgreSQL UNLOGGED tables
  - Sentry: Replaced with PostgreSQL error tracking
  - Simplified hosting: Moved from EC2 to Kubernetes (better resource utilization)

**Additional Benefits**:
- Reduced operational complexity (1 service to monitor instead of 4)
- Simplified backup strategy (single PostgreSQL backup covers everything)
- Easier disaster recovery (single restore point)

## Technical Wins

### Development Velocity

| Metric | Before | After | Improvement |
|--------|--------|-------|-------------|
| **API Development Time** | 3-4 days | 1-2 days | 62% faster |
| **Lines of Code** | ~45K LOC | ~22K LOC | 51% less |
| **API Changes** | 4-6 hrs | 1-2 hrs | 67% faster |
| **Onboarding Time** | 5 days | 2 days | 60% faster |

### Operational Benefits

1. **Unified Stack**: All data, caching, and observability in PostgreSQL. No context switching between Redis, Sentry, and application logs.

2. **Reduced Complexity**: Eliminated 3 external dependencies (Redis, Sentry, separate caching layer). Simplified deployment from 7 services to 3 (database, application, load balancer).

3. **Easier Debugging**: When an error occurs, all context is in PostgreSQL. Can correlate errors with cache state, database queries, and application traces using SQL JOINs.

4. **Simplified Deployments**: Single database connection string. No Redis endpoints, no Sentry DSN, no separate cache invalidation logic.

5. **Better Monitoring**: Direct SQL queries for all metrics. Example: `SELECT COUNT(*) FROM tb_error_log WHERE occurred_at > NOW() - INTERVAL '1 hour'` gives instant error rate.

## Challenges & Solutions

### Challenge 1: Initial Cache Hit Rate Was Low (52%)
**Problem**: After migration, cache hit rate was only 52%, below our target of 70%+. Investigation showed that our TTLs were too aggressive, causing frequent cache invalidations.

**Solution**:
- Analyzed query patterns using `SELECT key, COUNT(*) FROM cache_entries GROUP BY key ORDER BY COUNT(*) DESC`
- Discovered that user profile queries were being cached for only 60 seconds
- Adjusted TTLs:
  - User profiles: 60s → 300s (5 min)
  - Project lists: 120s → 600s (10 min)
  - Tenant settings: 300s → 3600s (1 hour)

**Outcome**: Cache hit rate increased from 52% to 73%, reducing average response time from 28ms to 18ms (36% improvement).

### Challenge 2: Partitioning Strategy for Error Logs
**Problem**: Error log table grew to 15GB after 3 months, causing slow queries on the monitoring dashboard.

**Solution**: Implemented monthly partitioning using PostgreSQL's native partitioning:
```sql
CREATE TABLE tb_error_occurrence (
    ...
) PARTITION BY RANGE (occurred_at);

-- Automatic monthly partition creation
SELECT create_error_occurrence_partition(NOW());
SELECT create_error_occurrence_partition(NOW() + INTERVAL '1 month');
```

**Outcome**:
- Query performance on error dashboard improved from 800ms to 45ms (94% faster)
- Implemented automatic cleanup: partitions older than 6 months are dropped
- Current error log size: 2.1GB (7x reduction)

### Challenge 3: Multi-Tenant Query Performance
**Problem**: Complex nested queries for large tenants (1000+ projects) were slow (>2 seconds), even with indexes.

**Solution**: Leveraged PostgreSQL materialized views for tenant-level aggregations:
```sql
CREATE MATERIALIZED VIEW v_tenant_project_summary AS
SELECT
    tenant_id,
    COUNT(*) as project_count,
    SUM(task_count) as total_tasks,
    array_agg(project_id) as project_ids
FROM projects
GROUP BY tenant_id;

-- Refresh every 5 minutes via cron
REFRESH MATERIALIZED VIEW CONCURRENTLY v_tenant_project_summary;
```

**Outcome**: Large tenant queries dropped from 2.3s to 85ms (96% improvement). Used FraiseQL's view-based approach to expose materialized view directly in GraphQL schema.

## Key Learnings

### What Worked Well

1. **PostgreSQL UNLOGGED Tables for Caching**: Performance matched Redis (sub-5ms read latency) while eliminating operational complexity. Cache survives server restarts (unlike Redis default), which prevented our "thundering herd" problem during deployments.

2. **Error Tracking in PostgreSQL**: Being able to write custom SQL queries for error analysis was game-changing. Example: "Show me all errors for tenant X that occurred during the 2pm deployment" is a simple SQL query, not a complex Sentry API call.

3. **Row-Level Security for Multi-Tenancy**: PostgreSQL RLS + FraiseQL made tenant isolation bulletproof. No application-level tenant filtering means zero chance of data leakage. Code review surface area reduced dramatically.

### What Required Adjustment

1. **Cache Warming Strategy**: Unlike Redis with explicit EXPIRE callbacks, PostgreSQL cache cleanup happens via periodic DELETE. We added a cache warming cron job to pre-populate frequently accessed keys before cleanup runs.

2. **Error Rate Limiting**: Initial notification implementation sent too many alerts during incident. Added rate limiting logic: notify on 1st error, then every 10th, then every 100th occurrence per fingerprint.

## Recommendations for Others

1. **Start with Partitioning from Day 1**: Don't wait until error logs are 15GB. Create monthly partitions immediately. Use the provided `ensure_error_occurrence_partitions()` function.

2. **Monitor Cache Hit Rate Closely**: Aim for 70%+ hit rate. If below 60%, analyze your TTLs. Use this query:
   ```sql
   -- Find cache keys with low hit rates
   SELECT
       key,
       hit_count,
       miss_count,
       ROUND(hit_count::numeric / NULLIF(hit_count + miss_count, 0) * 100, 2) as hit_rate_pct
   FROM cache_stats
   WHERE hit_rate_pct < 60
   ORDER BY (hit_count + miss_count) DESC
   LIMIT 20;
   ```

3. **Use Materialized Views for Complex Aggregations**: Don't be afraid of materialized views for tenant-level or dashboard aggregations. Refresh them every 5-15 minutes via cron. FraiseQL makes them trivially easy to expose in GraphQL.

4. **Set Up Prometheus Early**: Export PostgreSQL metrics to Prometheus from day one. Database pool utilization, cache hit rate, and query latency are critical early warning signals.

5. **Test Partition Cleanup**: Verify your partition cleanup strategy in staging first. Use `drop_old_error_occurrence_partitions(6)` to drop partitions older than 6 months.

## PostgreSQL-Native Features Usage

### Error Tracking (Sentry Alternative)

- **Errors Tracked**: ~850 errors/day (including warnings)
- **Error Grouping**: Automatic fingerprinting works well. 43 unique error types currently.
- **Cost Savings**: $890/month (vs Sentry Team Plan)
- **Experience**: Slightly less polished UI than Sentry (we query via SQL), but 10x more flexible. Can correlate errors with any business data via JOINs.

**Example Query We Use Daily**:
```sql
-- Top errors in last 24 hours with affected tenant count
SELECT
    e.error_fingerprint,
    e.error_type,
    e.error_message,
    COUNT(*) as occurrences,
    COUNT(DISTINCT e.user_context->>'tenant_id') as affected_tenants,
    MAX(e.last_seen) as last_occurrence
FROM tb_error_log e
WHERE e.last_seen > NOW() - INTERVAL '24 hours'
  AND e.environment = 'production'
  AND e.status = 'unresolved'
GROUP BY e.error_fingerprint, e.error_type, e.error_message
ORDER BY occurrences DESC
LIMIT 10;
```

### Caching (Redis Alternative)

- **Cache Hit Rate**: 73% (target: 70%+)
- **Cache Size**: 4.8GB (2.1M entries)
- **Cost Savings**: $340/month (vs Redis Elasticache m6g.large)
- **Experience**: Performance equivalent to Redis for our workload. Average read latency: 3.2ms (Redis was 2.1ms). The trade-off is worth it for operational simplicity.

**Example Caching Pattern**:
```python
from fraiseql.caching import PostgresCache

cache = PostgresCache(db_pool)

# Cache user profile for 5 minutes
@query
async def get_user_profile(info, user_id: str) -> UserProfile:
    # Try cache first
    cached = await cache.get(f"user_profile:{user_id}")
    if cached:
        return UserProfile(**cached)

    # Cache miss: fetch from database
    profile = await fetch_user_profile(user_id)
    await cache.set(f"user_profile:{user_id}", profile.dict(), ttl=300)

    return profile
```

### Multi-Tenancy (Row-Level Security)

- **Tenants**: 234 active tenants (ranging from 2 to 1,800 users each)
- **Isolation Strategy**: PostgreSQL Row-Level Security (RLS)
- **Performance Impact**: Minimal (<2ms overhead per query)

**RLS Policy Example**:
```sql
-- Enforce tenant isolation at database level
CREATE POLICY tenant_isolation_policy ON projects
    FOR ALL
    TO app_user
    USING (tenant_id = current_setting('app.current_tenant_id')::uuid);

-- FraiseQL sets current_tenant_id from JWT token automatically
SET LOCAL app.current_tenant_id = 'tenant-uuid-here';
```

## Testimonial

> "Migrating from Django + Strawberry + Redis + Sentry to FastAPI + FraiseQL was the best architectural decision we made in 2024. We cut our infrastructure costs in half, reduced our codebase by 50%, and shipped features 60% faster. The PostgreSQL-native approach means we have one service to monitor instead of four. When things go wrong, we can debug everything with SQL queries. No more juggling Sentry dashboards, Redis CLI, and application logs."
>
> — [Engineering Lead, Example SaaS Company]

## Metrics Timeline

### Month 1: Initial Deployment (March 2024)
- **Traffic**: 3.2M requests/day (migrated 25% of users)
- **P95 Latency**: 120ms (cache hit rate: 52%)
- **Challenges**: Cache TTL tuning, partition setup
- **Cost**: $1,520/month (20% under budget)

### Month 3: Production Stable (May 2024)
- **Traffic**: 9.8M requests/day (migrated 75% of users)
- **P95 Latency**: 75ms (cache hit rate: 68%)
- **Optimizations**:
  - Implemented monthly partitioning for error logs
  - Added materialized views for tenant dashboards
  - Tuned connection pool from 100 to 200 per pod
- **Cost**: $1,465/month (within budget)

### Month 6+: At Scale (August 2024 - Present)
- **Traffic**: 12.5M requests/day (100% of users)
- **P95 Latency**: 65ms (cache hit rate: 73%)
- **Lessons Learned**:
  - Materialized views are essential for complex aggregations
  - Monthly partitioning keeps error log queries fast
  - PostgreSQL-native approach scales well (no operational surprises)
- **Cost**: $1,475/month (stable, 46.5% savings vs old stack)

## Contact & Verification

- **Case Study Date**: October 2024
- **FraiseQL Version**: 0.11.0
- **Contact for Verification**: [Available upon request]
- **Public Reference**: [Company open to serving as reference for similar use cases]

---

## Real-World Production Tips

Based on 8 months in production:

1. **Connection Pool Sizing**: Start with `min_size=10, max_size=200` per pod. Monitor `db_pool_utilization` metric.

2. **Cache Cleanup**: Run cleanup every 5 minutes: `DELETE FROM cache_entries WHERE expires_at < NOW()`. Use pg_cron for scheduling.

3. **Error Notification Rate Limiting**: Implement exponential backoff: alert on occurrence [1, 10, 100, 1000] to avoid notification fatigue.

4. **Partition Maintenance**: Set up weekly cron job to ensure partitions exist 3 months ahead:
   ```sql
   SELECT ensure_error_occurrence_partitions(3);
   ```

5. **Monitoring Queries**: Create custom Grafana dashboard querying PostgreSQL directly. Example:
   ```sql
   -- Cache hit rate (last 5 minutes)
   SELECT
       ROUND(
           SUM(CASE WHEN status = 'hit' THEN 1 ELSE 0 END)::numeric /
           COUNT(*) * 100,
           2
       ) as hit_rate_pct
   FROM cache_access_log
   WHERE accessed_at > NOW() - INTERVAL '5 minutes';
   ```

---

**Note**: This is an example case study. For your production deployment to be featured, contact lionel.hamayon@evolution-digitale.fr with your metrics and architecture details.
