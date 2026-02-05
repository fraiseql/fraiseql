# Performance & Optimization Guide

**Status:** ✅ Production Ready
**Audience:** Backend engineers, DevOps, database administrators
**Reading Time:** 40-50 minutes
**Last Updated:** 2026-02-05

Comprehensive guide to optimizing FraiseQL performance for production systems.

---

## Table of Contents

1. [Query Optimization](#query-optimization)
2. [Database Optimization](#database-optimization)
3. [Caching Strategies](#caching-strategies)
4. [Connection Pooling](#connection-pooling)
5. [Monitoring & Profiling](#monitoring--profiling)
6. [Scaling Strategies](#scaling-strategies)
7. [Common Bottlenecks](#common-bottlenecks--solutions)

---

## Query Optimization

### 1. Avoid N+1 Query Problem

❌ **Bad: N+1 queries**

```graphql
query GetUsers {
  users {
    id
    name
    # This causes 1 query for users + N queries for posts (one per user)
    posts {
      id
      title
    }
  }
}
```text

Result: 101 queries (1 for users + 100 for individual user's posts)

✅ **Good: Single nested query**

```graphql
query GetUsers {
  users {
    id
    name
    posts {  # Joined in single query
      id
      title
    }
  }
}
```text

Result: 1-2 queries total

### 2. Pagination for Large Result Sets

❌ **Bad: Fetch all records**

```graphql
query AllPosts {
  posts {  # Returns 1,000,000 records!
    id
    title
    content
  }
}
```text

✅ **Good: Paginate with limit/offset or cursor**

```graphql
query PostsPaginated($first: Int!, $after: String) {
  posts(first: $first, after: $after) {
    edges {
      cursor
      node { id title }
    }
    pageInfo {
      hasNextPage
      endCursor
    }
  }
}
```text

### 3. Request Only Needed Fields

❌ **Bad: Over-fetching**

```graphql
query GetUser {
  user(id: "123") {
    id
    email
    full_name
    phone
    address
    payment_methods
    all_orders { id amount date }  # Fetch everything
    all_reviews { id rating text }
  }
}
```text

✅ **Good: Specific fields**

```graphql
query GetUser {
  user(id: "123") {
    id
    email
    full_name
    recent_orders(limit: 5) {
      id
      amount
    }
  }
}
```text

### 4. Use Database Indexes

```sql
-- ✅ Good: Indexes on common filters
CREATE INDEX idx_user_email ON users(email);
CREATE INDEX idx_order_date ON orders(created_at);
CREATE INDEX idx_user_status ON users(status);

-- For complex queries:
CREATE INDEX idx_orders_user_date ON orders(user_id, created_at);

-- For full-text search:
CREATE INDEX idx_content_search ON documents USING GIN(to_tsvector('english', content));
```text

**Index Selection:**

- Filter columns: Yes (WHERE clause)
- Join columns: Yes (ON clause)
- Order columns: Yes (ORDER BY)
- Covering index: Include other columns for "index-only" scans

### 5. Explain Query Plans

```sql
EXPLAIN ANALYZE
SELECT u.id, u.email, COUNT(o.id)
FROM users u
LEFT JOIN orders o ON u.id = o.user_id
WHERE u.status = 'active'
GROUP BY u.id, u.email
ORDER BY u.email;

-- Output shows:
-- - Sequential Scan vs Index Scan
-- - Rows filtered
-- - Actual runtime
-- - Inefficiencies (full table scans, etc.)
```text

---

## Database Optimization

### 1. Connection Pooling

```rust
// fraiseql configuration
const POOL_SIZE: u32 = 10;  // Connections per server instance
const QUEUE_TIMEOUT: Duration = Duration::from_secs(5);
const IDLE_TIMEOUT: Duration = Duration::from_secs(900);

// For 100 concurrent users:
// Pool size = 10-20 (not 100!)
// Each connection can handle multiple queries sequentially
```text

### 2. Query Result Caching

```python
# Cache SELECT query results
@types.query
def get_users(self, limit: int = 50) -> list[User]:
    """
    @cache(ttl=300)  # Cache for 5 minutes
    """
    pass

# Invalidate cache on mutations
@types.mutation
def create_user(self, email: str) -> User:
    """
    @invalidate_cache(paths=['getUsers'])  # Clear users list
    """
    pass
```text

### 3. Materialized Views for Aggregations

```sql
-- Pre-compute expensive aggregations
CREATE MATERIALIZED VIEW user_stats AS
SELECT
  user_id,
  COUNT(*) as total_orders,
  SUM(amount) as total_spent,
  AVG(amount) as avg_order_value,
  MAX(created_at) as last_order_date
FROM orders
GROUP BY user_id;

-- Refresh hourly
SELECT cron.schedule('refresh_user_stats', '0 * * * *',
  'REFRESH MATERIALIZED VIEW CONCURRENTLY user_stats');

-- Query materialized view (fast)
SELECT * FROM user_stats WHERE user_id = $1;
```text

### 4. Partitioning Large Tables

```sql
-- Time-based partitioning for time-series data
CREATE TABLE events (
  event_date DATE NOT NULL,
  event_id BIGSERIAL,
  user_id UUID,
  event_type VARCHAR(50),
  PRIMARY KEY (event_date, event_id)
) PARTITION BY RANGE (event_date);

-- Create partitions
CREATE TABLE events_2024_01 PARTITION OF events
  FOR VALUES FROM ('2024-01-01') TO ('2024-02-01');

CREATE TABLE events_2024_02 PARTITION OF events
  FOR VALUES FROM ('2024-02-01') TO ('2024-03-01');

-- Queries automatically scan only relevant partitions
SELECT * FROM events
WHERE event_date BETWEEN '2024-01-15' AND '2024-01-20';
-- Only queries events_2024_01 partition
```text

### 5. Denormalization When Needed

```sql
-- Denormalized user_stats table avoids expensive joins
CREATE TABLE user_stats (
  user_id UUID PRIMARY KEY,
  email VARCHAR(255),
  full_name VARCHAR(255),
  total_orders INT,
  total_spent DECIMAL(12, 2),
  last_order_date DATE,
  updated_at TIMESTAMP
);

-- Update on order changes
CREATE TRIGGER order_update_stats
AFTER INSERT OR UPDATE ON orders
FOR EACH ROW
EXECUTE FUNCTION update_user_stats(NEW.user_id);
```text

---

## Caching Strategies

### 1. Cache Layers

```text
┌─────────────────┐
│   Client Cache  │  (Browser local storage, Redux)
└────────┬────────┘
         ↓
┌─────────────────┐
│ Apollo Cache    │  (InMemoryCache)
└────────┬────────┘
         ↓
┌─────────────────┐
│ Redis Cache     │  (Server-side, shared)
└────────┬────────┘
         ↓
┌─────────────────┐
│   Database      │  (Slowest)
└─────────────────┘
```text

### 2. Cache-First vs Cache-And-Network

```typescript
// Cache-first: Good for static data
const { data } = useQuery(GET_CATEGORIES, {
  fetchPolicy: 'cache-first'  // 200ms response
});

// Cache-and-network: Good for mostly-static data
const { data } = useQuery(GET_POSTS, {
  fetchPolicy: 'cache-and-network'  // Returns cached data, then updates
});

// Network-only: Good for real-time data
const { data } = useQuery(GET_STOCK_PRICE, {
  fetchPolicy: 'network-only'  // Always fresh
});
```text

### 3. Redis for Shared Cache

```python
# Cache expensive query in Redis
import redis

cache = redis.Redis(host='localhost', port=6379)

async def get_user_stats(user_id: str):
    # Try cache first
    cached = cache.get(f'user_stats:{user_id}')
    if cached:
        return json.loads(cached)

    # Cache miss - query database
    stats = await db.fetch("""
        SELECT COUNT(*) as orders, SUM(amount) as total
        FROM orders WHERE user_id = $1
    """, user_id)

    # Store in cache (5 minute TTL)
    cache.setex(
        f'user_stats:{user_id}',
        300,
        json.dumps(stats)
    )

    return stats

# Invalidate cache on changes
async def create_order(user_id: str, ...):
    await db.execute('INSERT INTO orders ...')
    cache.delete(f'user_stats:{user_id}')  # Invalidate
```text

---

## Connection Pooling

### Configuration

```toml
# fraiseql.toml
[fraiseql.database]
pool_size = 20              # Connections
connection_timeout = 10000  # ms
idle_timeout = 900000       # ms (15 min)
max_lifetime = 1800000      # ms (30 min)
test_on_checkout = true     # Verify connection health
```text

### Tuning

```text
Pool Size Formula:
  = ((core_count × 2) + effective_spindle_count)
  = ((8 cores × 2) + 1) = 17 connections

Concurrency = Pool Size × Average Query Time
  = 20 connections × 50ms = 1000 concurrent requests
```text

### Monitoring

```sql
-- Check pool usage
SELECT count(*) FROM pg_stat_activity;
-- Should be <= pool_size (20)

-- Identify slow/idle connections
SELECT pid, usename, state, query, query_start
FROM pg_stat_activity
WHERE state = 'idle'
  AND query_start < NOW() - INTERVAL '15 minutes';
```text

---

## Monitoring & Profiling

### Query Performance Metrics

```python
# Instrument queries with timing
@types.query
def get_posts(self, limit: int = 50) -> list[Post]:
    start = time.time()

    # Query execution
    results = ...

    duration = time.time() - start
    log_metric('query.duration', duration, tags={'query': 'getPosts'})

    return results
```text

### Slow Query Log

```sql
-- Enable slow query logging
ALTER SYSTEM SET log_min_duration_statement = 100;  -- Log queries > 100ms
SELECT pg_reload_conf();

-- View slow queries
SELECT * FROM pg_stat_statements
ORDER BY mean_exec_time DESC
LIMIT 20;
```text

### APM Integration (DataDog/New Relic)

```python
from datadog import api
from datadog_api_client.v1.api.metrics_api import MetricsApi

# Report query metrics
metrics_api.submit_metrics(
    body=MetricsPayload(
        series=[
            Series(
                metric="fraiseql.query.duration",
                points=[[int(time.time()), query_duration_ms]],
                tags=["query:getPosts", "endpoint:graphql"]
            )
        ]
    )
)
```text

---

## Scaling Strategies

### Vertical Scaling (More Powerful Hardware)

✅ **When:**

- Single database is bottleneck
- Cost-effective up to ~200GB data
- Complex queries needing more CPU/RAM

### Horizontal Scaling (Multiple Servers)

```text
┌──────────────────────────────────────┐
│        Load Balancer (nginx)         │
└──────────────┬───────────────────────┘
               │
    ┌──────────┼──────────┐
    ↓          ↓          ↓
┌─────────┐┌─────────┐┌─────────┐
│FraiseQL │FraiseQL │FraiseQL │
│ Instance│ Instance│ Instance│
└────┬────┘└────┬────┘└────┬────┘
     │          │          │
     └──────────┼──────────┘
                ↓
        ┌──────────────┐
        │ PostgreSQL   │
        │ (Shared DB)  │
        └──────────────┘
```text

### Read Replicas

```sql
-- Primary for writes
PRIMARY (writes)
  ↓ (replication)
REPLICA 1 (reads)
REPLICA 2 (reads)
REPLICA 3 (reads)
```text

```python
# Route queries to replica
@database(
    write='postgres_primary',
    read='postgres_replica'
)
def get_users(self) -> list[User]:
    """Queries use replica, mutations use primary"""
    pass
```text

### Citus for Sharding

```sql
-- Distribute table across nodes
SELECT create_distributed_table('orders', 'user_id');

-- Queries automatically sharded
SELECT * FROM orders WHERE user_id = $1;  -- Single shard
SELECT * FROM orders;  -- All shards (parallel)
```text

---

## Common Bottlenecks & Solutions

| Symptom | Cause | Solution |
|---------|-------|----------|
| High CPU | Complex queries, missing indexes | Add indexes, optimize queries |
| High Memory | Large result sets | Paginate, limit results |
| Slow responses | N+1 queries | Use nested queries, batch requests |
| Connection errors | Pool exhausted | Increase pool size, optimize query time |
| Disk I/O | No indexes on filters | Create indexes |
| Network latency | Geographic distance | Use CDN, edge servers |
| Cache misses | Low TTL | Increase TTL for stable data |

---

## Performance Benchmarking

### Benchmark Suite

```typescript
import Benchmark from 'benchmark';

const suite = new Benchmark.Suite;

suite
  .add('Simple query (1KB result)', () => {
    return client.query(GET_USER);
  })
  .add('Complex query (100KB result)', () => {
    return client.query(GET_POSTS_WITH_COMMENTS);
  })
  .add('Aggregation query', () => {
    return client.query(GET_STATS);
  })
  .on('complete', function() {
    console.log('Fastest is ' + this.filter('fastest').map('name'));
  })
  .run({ async: true });
```text

### Load Testing

```bash
# Using Apache Bench
ab -n 10000 -c 100 http://localhost:5000/graphql

# Results:
# Requests per second: 500
# 95th percentile latency: 200ms
# Max latency: 1000ms
```text

---

## Best Practices Checklist

- [ ] Indexes on all filter/join/sort columns
- [ ] Query result pagination for large datasets
- [ ] Nested queries instead of N+1
- [ ] Connection pooling configured
- [ ] Slow query logging enabled
- [ ] Cache strategies implemented
- [ ] Read replicas for heavy read workloads
- [ ] Monitoring/alerting in place
- [ ] Load testing before production
- [ ] Database statistics up-to-date (`ANALYZE`)

---

## See Also

**Related Guides:**

- [Schema Design Best Practices](./schema-design-best-practices.md)
- [Production Deployment](./production-deployment.md)
- [Observability & Monitoring](./observability.md)

**Production Patterns:**

- [Analytics Platform](../patterns/analytics-olap-platform.md) - Optimize for aggregations
- [SaaS Multi-Tenant](../patterns/saas-multi-tenant.md) - Row-level security performance

---

**Last Updated:** 2026-02-05
**Version:** v2.0.0-alpha.1
