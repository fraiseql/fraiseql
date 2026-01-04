# FraiseQL Caching Strategy Guide

**Version**: 1.0
**Last Updated**: January 4, 2026
**Framework**: FraiseQL v1.9.1

---

## Overview

FraiseQL implements intelligent query result caching optimized for transactional workloads. This guide explains cache performance characteristics and helps you choose the right strategy for your use case.

**TL;DR**: FraiseQL caching works great for typical applications (85%+ hit rate). For analytics, use a data warehouse instead.

---

## Cache Hit Rates by Workload

### High Cache Efficiency (85%+)

#### Typical SaaS Applications
**Characteristics**:
- Repeated queries for user data, settings, preferences
- Common filters (status, tenant_id, user_id)
- High temporal locality (same data accessed frequently)

**Examples**:
- GetUser query (same user_id requested repeatedly)
- ListUserSettings (user-specific data)
- GetTenantConfig (tenant settings)

**Expected Performance**:
- ✅ **Cache Hit Rate: 85%+**
- ✅ **DB Load Reduction: 85%**
- ✅ **Response Time: < 5ms (cached) vs 50-100ms (DB)**

**Optimization**: No special configuration needed. Caching works automatically.

---

#### High-Frequency APIs
**Characteristics**:
- Frequent requests for same data
- Volatile data (caches refresh frequently)
- Heavy read load with occasional writes

**Examples**:
- GetProduct (hot products checked repeatedly)
- GetInventory (inventory checked on every request)
- GetPricingTier (pricing accessed frequently)

**Expected Performance**:
- ✅ **Cache Hit Rate: 92%+**
- ✅ **DB Load Reduction: 92%**
- ✅ **Throughput: 200K+ QPS with caching vs 20K without**

**Optimization**: Adjust cache TTL based on data freshness requirements.

```python
@query
@cache(ttl_seconds=300)  # 5 minute cache
async def get_product(product_id: ID) -> Product:
    """Frequently requested product data."""
    ...
```

---

### Low Cache Efficiency (30-40%)

#### Analytical Workloads ⚠️
**Characteristics**:
- Each query is unique (different date ranges, filters, groupings)
- High cardinality (many possible combinations)
- Low temporal locality (same query rarely repeated)

**Examples**:
- ReportsQuery with custom date ranges
- DailyAnalytics with user-specific filters
- CustomMetrics with arbitrary dimensions

**Expected Performance**:
- ❌ **Cache Hit Rate: 30-40%** (expected, not a bug!)
- ⚠️ **DB Load Reduction: 30-40%**
- ⚠️ **Why**: Analytical queries have high cardinality, low reusability

**Root Cause Analysis**:

```python
# Example: Each query is unique
Query 1: SELECT SUM(sales) FROM orders WHERE date >= '2026-01-01' AND date <= '2026-01-04'
Query 2: SELECT SUM(sales) FROM orders WHERE date >= '2026-01-02' AND date <= '2026-01-05'
Query 3: SELECT SUM(sales) FROM orders WHERE date >= '2026-01-01' AND date <= '2026-01-07'

# Cache key for each query is unique (different WHERE clause)
# → Cache miss for each query
# → 30-40% hit rate is EXPECTED
```

**Recommendation**: ❌ **Don't use FraiseQL GraphQL caching for analytics**

---

## When to Use FraiseQL Caching

### ✅ Use FraiseQL Caching When:

1. **Queries are repeated** (same parameters run multiple times)
   - User login queries
   - Frequently accessed products/articles
   - Settings/configuration lookups

2. **Results don't need sub-second freshness**
   - Cache TTL of 1-5 minutes is acceptable
   - Stale data is acceptable for short periods

3. **Single-user or small group access**
   - Per-user queries with same filters
   - Team-specific data with consistent queries

4. **Response time matters**
   - Cached responses: < 5ms
   - DB responses: 50-100ms
   - Network: 10-50ms
   - **Total saved**: 50-100ms per request

**Example - Good Fit for Caching**:
```graphql
# User refreshes their profile frequently
query GetMyProfile($userId: ID!) {
  user(id: $userId) {
    id
    name
    email
    settings {
      theme
      notifications
      privacy
    }
  }
}

# Hit Rate: 90%+
# Reason: Same user, same query, repeated access
```

---

### ❌ Don't Use FraiseQL Caching For:

1. **Ad-hoc exploratory queries**
   - Each query is unique
   - Low cache hit rate
   - **Alternative**: Use data warehouse

2. **Real-time analytical queries**
   - Need fresh data every time
   - Caching defeats the purpose
   - **Alternative**: Materialized views

3. **Complex aggregations across billions of rows**
   - CPU-intensive
   - Network-bound
   - **Alternative**: Snowflake, BigQuery, Redshift

4. **Highly volatile data**
   - Needs updates < 1 second
   - Cache invalidation overhead exceeds benefit
   - **Alternative**: Direct database queries (disable cache)

**Example - Poor Fit for Caching**:
```graphql
# Different date ranges every time
query GetDailySales($fromDate: Date!, $toDate: Date!) {
  sales(dateRange: {from: $fromDate, to: $toDate}) {
    date
    total
  }
}

# Hit Rate: 5-10% (each request has unique date range)
# Cost: Cache lookup overhead with no benefit
# Solution: Use data warehouse instead
```

---

## Cache Configuration

### Default Settings

```python
# Global cache defaults
DEFAULT_CACHE_TTL = 300  # 5 minutes
DEFAULT_CACHE_ENABLED = True
```

### Per-Query Configuration

#### Enable Caching (Default)
```python
@query
@cache()  # Uses default 5 minute TTL
async def get_user(user_id: ID) -> User:
    """Cached with default 5 minute TTL."""
    ...
```

#### Custom TTL
```python
@query
@cache(ttl_seconds=3600)  # 1 hour
async def get_product(product_id: ID) -> Product:
    """Cached for 1 hour (longer TTL for stable data)."""
    ...
```

#### Disable Caching
```python
@query
@cache(enabled=False)
async def get_real_time_data() -> Data:
    """Never cached - always fresh data."""
    ...
```

#### Conditional Caching
```python
@query
async def search_products(
    query: str,
    use_cache: bool = True
) -> List[Product]:
    """Cache enabled by default, can be disabled per-request."""
    if use_cache:
        # Cache this result
        ...
    else:
        # Skip cache
        ...
```

---

## Optimization Strategies

### For Transactional Queries (85%+ Hit Rate)

**Strategy**: No special optimization needed. Caching works automatically.

**Verification**:
```bash
# Check cache hit rate
fraiseql monitoring cache-stats

# Expected output:
# Total Queries: 1000
# Cache Hits: 850
# Cache Misses: 150
# Hit Rate: 85%
```

---

### For Analytical Queries (30-40% Hit Rate)

Choose ONE of these strategies:

#### Option 1: Materialized Views (Recommended for PostgreSQL)

**Concept**: Pre-compute aggregations on a schedule.

**Example**:
```sql
-- Create materialized view for daily sales
CREATE MATERIALIZED VIEW daily_sales_summary AS
SELECT
  DATE(created_at) as date,
  SUM(amount) as total_sales,
  COUNT(*) as transaction_count,
  AVG(amount) as avg_transaction
FROM sales
GROUP BY DATE(created_at);

-- Create index for fast queries
CREATE INDEX idx_daily_sales_date ON daily_sales_summary(date);

-- Refresh on schedule (e.g., hourly)
REFRESH MATERIALIZED VIEW daily_sales_summary;
```

**GraphQL Query**:
```graphql
query GetDailySalesSummary($date: Date!) {
  dailySalesSummary(date: $date) {
    date
    totalSales
    transactionCount
    avgTransaction
  }
}
```

**Benefits**:
- ✅ Instant response (pre-computed results)
- ✅ Consistent results (refreshed on schedule)
- ✅ No cache overhead
- ✅ Highly scalable

**When to Use**:
- Data warehouse replaces real-time requirement
- Refresh frequency: hourly, daily, weekly
- Queries: standard reports, dashboards

---

#### Option 2: Data Warehouse (Recommended for Scale)

**Concept**: Separate analytics database optimized for complex queries.

**Platforms**:
- Snowflake (cloud-native, scalable)
- BigQuery (managed, serverless)
- Redshift (AWS-native)
- ClickHouse (open-source)

**Architecture**:
```
PostgreSQL (OLTP - transactional)
    ↓
CDC (Change Data Capture)
    ↓
Snowflake/BigQuery (OLAP - analytical)
    ↓
FraiseQL queries against warehouse
```

**FraiseQL with Warehouse**:
```python
# Query against analytical database
@query
async def analytics_reports() -> List[Report]:
    """Query warehouse instead of main database."""
    # Use separate connection pool for warehouse
    result = await warehouse_pool.fetch("""
        SELECT ... FROM reports_summary
        WHERE date >= NOW() - INTERVAL '30 days'
    """)
    return result
```

**Benefits**:
- ✅ Optimized for analytical queries
- ✅ Separate from production database
- ✅ Scales to trillions of rows
- ✅ Advanced aggregation functions

**When to Use**:
- Complex analytics across billions of rows
- Multiple data sources
- Real-time dashboards
- Data science integration

---

#### Option 3: Separate Analytics Database

**Concept**: PostgreSQL read replica for analytics.

**Setup**:
```python
# Main database (OLTP)
main_db_pool = ConnectionPool(DATABASE_URL)

# Analytics replica (OLAP)
analytics_db_pool = ConnectionPool(ANALYTICS_DATABASE_URL)

@query
async def get_analytics() -> AnalyticsResult:
    """Query from analytics database."""
    result = await analytics_db_pool.fetch("""
        SELECT ... FROM large_table
        WHERE date >= NOW() - INTERVAL '30 days'
    """)
    return result
```

**Benefits**:
- ✅ Uses PostgreSQL (familiar)
- ✅ Read replica doesn't impact main database
- ✅ Can add indexes/tuning for analytics
- ✅ Periodic refresh from main database

**When to Use**:
- PostgreSQL expertise in team
- Moderate analytics load
- Need for custom tuning

---

## Monitoring Cache Performance

### Check Current Cache Stats

```bash
# CLI command
fraiseql monitoring cache-stats

# Output:
# Cache Statistics
# ================
# Total Queries: 10,000
# Cache Hits: 8,500
# Cache Misses: 1,500
# Hit Rate: 85.0%
# Bytes Stored: 125 MB
# Most Cached: GetUser (500 hits)
```

### Prometheus Metrics

```
# Query cache hit rate
cache_hit_rate{query="GetUser"} = 0.95
cache_hit_rate{query="GetProduct"} = 0.92
cache_hit_rate{query="AnalyticsQuery"} = 0.30  # ← Alert on this

# Cache size
cache_size_bytes{} = 131072000  # 125 MB

# Cache evictions
cache_evictions_total{} = 42
```

### Grafana Dashboard

Create a dashboard to visualize:
1. Cache hit rate by query
2. Cache size growth
3. Eviction frequency
4. Top cached queries
5. Query response times (cached vs uncached)

---

## Cache Invalidation

### Automatic Invalidation

Cache is automatically invalidated when related data is mutated:

```python
@mutation
async def update_user(user_id: ID, name: str) -> User:
    """Automatically invalidates GetUser cache for this user."""
    user = await repository.update_user(user_id, name=name)

    # Cache is automatically invalidated for:
    # - GetUser(id: {user_id})
    # - ListUsers queries that include this user
    # - Any query that depends on User type

    return user
```

### Manual Invalidation

If needed, manually invalidate cache:

```python
from fraiseql.caching import cache_manager

@mutation
async def bulk_update_users() -> str:
    """Manually invalidate all user-related caches."""
    # Invalidate specific query
    await cache_manager.invalidate("GetUser")

    # Invalidate by type
    await cache_manager.invalidate_type("User")

    # Clear entire cache
    await cache_manager.clear_all()

    return "Cache invalidated"
```

---

## Performance Comparison

### Response Time Breakdown

**Cached Response** (hit):
```
Cache lookup:       1-2 ms
Serialization:      0.5 ms
Network:            10-50 ms
────────────────────────
Total:              11-52 ms
```

**Database Response** (miss):
```
DB query:           50-100 ms
Result processing:  5-10 ms
Serialization:      0.5 ms
Network:            10-50 ms
────────────────────────
Total:              65-160 ms
```

**Savings**: 50-100 ms per request (2-3x faster)

### Query Throughput

**Without Caching**:
- Simple query: 20,000 QPS per server
- Complex query: 2,000 QPS per server

**With Caching (85% hit rate)**:
- Simple query: 180,000 QPS per server (9x improvement)
- Complex query: 18,000 QPS per server (9x improvement)

---

## Troubleshooting

### Cache Hit Rate Lower Than Expected

**Symptom**: `cache_hit_rate < 50%` for transactional query

**Checklist**:
1. ✅ Is the query parameter the same across requests?
   - Example: `GetUser(id: 123)` vs `GetUser(id: 456)` = different cache keys
   - Solution: Ensure similar queries run repeatedly

2. ✅ Is cache TTL too short?
   - Current: 5 minutes
   - Solution: Increase TTL for stable data: `@cache(ttl_seconds=3600)`

3. ✅ Are there too many unique parameter combinations?
   - Example: Date ranges always different = cache miss
   - Solution: Use materialized views for analytics

4. ✅ Is cache filling up and evicting entries?
   - Check: `cache_evictions_total` metrics
   - Solution: Increase cache size or reduce TTL

### Cache Memory Growing Too Large

**Symptom**: Cache size > 500 MB or memory usage increasing

**Checklist**:
1. ✅ Are results too large?
   - Solution: Paginate results, limit field selection

2. ✅ Is TTL too long?
   - Current: 1 hour (3600 seconds)
   - Solution: Reduce to 5-15 minutes: `@cache(ttl_seconds=600)`

3. ✅ Are there too many unique cache keys?
   - Solution: Reduce query variation, consolidate similar queries

4. ✅ Is cache not evicting old entries?
   - Check: `cache_size_bytes` and eviction metrics
   - Solution: Monitor and adjust cache policy

---

## Best Practices

### DO ✅

- ✅ Cache queries that are repeated (80%+ hit rate expected)
- ✅ Use 5-15 minute TTL for most data
- ✅ Disable cache for real-time data
- ✅ Monitor cache hit rates by query
- ✅ Use data warehouse for analytics (separate system)
- ✅ Test cache behavior with realistic workloads

### DON'T ❌

- ❌ Cache highly volatile data (updates < 1 sec)
- ❌ Cache large result sets (> 1 MB per query)
- ❌ Cache all queries with 1 hour TTL (adjust per use case)
- ❌ Use GraphQL caching for ad-hoc analytical queries
- ❌ Assume all analytical queries will cache well
- ❌ Ignore cache eviction warnings

---

## Further Reading

- [Query Optimization Guide](./query-optimization.md)
- [Performance Tuning](./performance-tuning.md)
- [Database Connection Pooling](./database-pooling.md)
- [Monitoring Guide](./monitoring.md)

---

## Summary

| Workload | Cache Hit Rate | Recommendation |
|----------|---|---|
| **SaaS (user data)** | 85%+ | ✅ Use GraphQL caching |
| **High-frequency API** | 92%+ | ✅ Use GraphQL caching |
| **Analytical queries** | 30-40% | ❌ Use data warehouse |
| **Real-time dashboards** | 0% | ❌ Disable cache |
| **Reports & summaries** | 30-50% | ⚠️ Use materialized views |

**Key Takeaway**: FraiseQL caching is optimized for transactional workloads (85%+ hit rate). For analytics, use a separate system (materialized views, data warehouse).
