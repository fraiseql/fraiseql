# Performance & Scaling Guide

**Duration**: 2-4 hours
**Outcome**: Optimize and scale FraiseQL for high throughput
**Prerequisites**: Understanding of deployment (see [deployment guide](../../deployment/guide.md))

---

## Part 1: Understanding Performance

### Key Metrics

Measure these metrics to understand your system's performance:

| Metric | Good | Acceptable | Poor |
|--------|------|-----------|------|
| P50 latency | <10ms | <50ms | >100ms |
| P95 latency | <50ms | <200ms | >500ms |
| P99 latency | <100ms | <500ms | >1000ms |
| Throughput | >1000 req/sec | >100 req/sec | <100 req/sec |
| Error rate | <0.1% | <1% | >1% |
| Database pool utilization | <50% | <80% | >80% |

### Baseline Performance

FraiseQL baseline performance (on single 2-core 4GB instance):

```
Simple query (single row fetch):

- Latency: 2-5ms
- Throughput: 200+ req/sec

Complex query (10 nested fields):

- Latency: 10-50ms
- Throughput: 100+ req/sec

Pagination query (100 rows):

- Latency: 20-100ms
- Throughput: 50+ req/sec
```

### Measuring Performance

#### 1. Enable Query Logging

```rust
// In your server setup
let schema = CompiledSchema::from_file("schema.compiled.json")?;
schema.enable_query_logging(); // Logs query execution time

// Check logs:
// [2026-01-26 10:30:15] Query executed in 12.3ms
// [2026-01-26 10:30:16] Query executed in 45.1ms
```

#### 2. Use Load Testing Tool

```bash
# Install Apache Bench
apt-get install apache2-utils

# Run load test (100 requests, 10 concurrent)
ab -n 100 -c 10 -p query.json \
  -H "Content-Type: application/json" \
  http://localhost:8080/graphql

# Output shows latency percentiles
```

#### 3. Monitor with Prometheus

```bash
# Scrape metrics endpoint
curl http://localhost:8080/metrics | grep fraiseql

# Example output:
# fraiseql_query_duration_seconds_bucket{le="0.01"} 50
# fraiseql_query_duration_seconds_bucket{le="0.05"} 95
# fraiseql_query_duration_seconds_bucket{le="0.1"} 99
```

---

## Part 2: Performance Tuning

### Database Optimization

#### 1. Add Indexes

Indexes speed up database queries significantly:

```sql
-- Frequently filtered fields
CREATE INDEX idx_users_email ON users(email);
CREATE INDEX idx_users_created_at ON users(created_at);
CREATE INDEX idx_posts_user_id ON posts(user_id);

-- Composite index for common filters
CREATE INDEX idx_posts_user_date ON posts(user_id, created_at DESC);

-- Full-text search indexes
CREATE INDEX idx_posts_search ON posts USING GIN(
  to_tsvector('english', title || ' ' || content)
);
```

Performance impact:

- Without indexes: 500-5000ms for filtered queries
- With indexes: 5-50ms for filtered queries
- Index overhead: ~1ms per INSERT

#### 2. Query Planning

Analyze query execution:

```sql
-- See how database executes query
EXPLAIN ANALYZE SELECT * FROM users WHERE email = 'test@example.com';

-- Look for:

-- - Sequential Scan (bad - use index)
-- - Index Scan (good)
-- - Join condition properly indexed (good)
```

#### 3. Connection Pooling

Configure connection pool for your workload:

```toml
[database]
# Min connections (keep warm)
pool_min = 5

# Max connections (protect database)
pool_max = 50

# Timeout (fail fast on unavailable connections)
connection_timeout_ms = 5000
```

Recommended settings:

- Light workload: min=5, max=20
- Medium workload: min=10, max=50
- Heavy workload: min=20, max=100

Check pool usage:

```bash
curl http://localhost:8080/health | jq '.database.connection_pool'
# {
#   "active": 3,
#   "idle": 7,
#   "total": 10
# }
```

If `active` is consistently > 80% of `total`, increase `pool_max`.

### Application Optimization

#### 1. Query Complexity Limits

Prevent expensive queries:

```json
{
  "limits": {
    "maxQueryComplexity": 5000,
    "maxDepth": 15,
    "maxAliases": 10,
    "defaultLimit": 100,
    "maxLimit": 1000
  }
}
```

Adjust based on your use case:

- Public API: Conservative limits (complexity=1000, depth=5)
- Internal API: Generous limits (complexity=10000, depth=20)

#### 2. Optimize Queries

❌ **Bad query** (requests unnecessary fields):
```graphql
query GetUsers {
  users {
    id
    name
    email
    password_hash  # Don't need this!
    created_at
    updated_at
    last_login
    ip_address
  }
}
```

✅ **Good query** (only needed fields):
```graphql
query GetUsers {
  users {
    id
    name
    email
  }
}
```

Difference:

- Bad query: 50ms
- Good query: 10ms
- Improvement: 80% faster

#### 3. Pagination for Large Result Sets

❌ **Bad** (requests all 100,000 users):
```graphql
query AllUsers {
  users {
    id
    name
  }
}
```

✅ **Good** (paginated request):
```graphql
query UsersPaginated {
  users(first: 100, after: "cursor123") {
    edges {
      node { id name }
      cursor
    }
    pageInfo { hasNextPage }
  }
}
```

Performance comparison:

- All at once: 500-1000ms + memory issues
- With pagination: 10-50ms per page + constant memory

### Infrastructure Optimization

#### 1. Vertical Scaling (Add Resources)

Increase CPU/Memory on single instance:

```bash
# Before: 2 CPU, 4 GB RAM, 100 req/sec
# After: 4 CPU, 16 GB RAM, 300+ req/sec

# Check resource usage
docker stats fraiseql

# Monitor:
# CPU usage should be <80%
# Memory usage should be <70%
```

When to scale vertically:

- Database waiting on CPU
- Connection pool utilization >80%
- Single bottleneck identified

#### 2. Horizontal Scaling (Add Instances)

Run multiple instances behind load balancer:

```bash
# Start 3 instances
docker run -d -p 8081:8080 fraiseql:latest
docker run -d -p 8082:8080 fraiseql:latest
docker run -d -p 8083:8080 fraiseql:latest

# Load balance with nginx
upstream fraiseql {
  server 127.0.0.1:8081;
  server 127.0.0.1:8082;
  server 127.0.0.1:8083;
}

server {
  location / {
    proxy_pass http://fraiseql;
  }
}
```

Scaling performance:

- 1 instance: 100 req/sec (CPU maxed)
- 3 instances: 280 req/sec (CPU at 80%)
- Improvement: 180% more throughput

#### 3. Load Balancing Strategy

```nginx
# Round-robin (simple, balanced)
upstream fraiseql {
  server instance1:8080;
  server instance2:8080;
  server instance3:8080;
}

# Least connections (good for variable load)
upstream fraiseql {
  least_conn;
  server instance1:8080;
  server instance2:8080;
  server instance3:8080;
}

# IP hash (sticky sessions)
upstream fraiseql {
  ip_hash;
  server instance1:8080;
  server instance2:8080;
}
```

---

## Part 3: Caching Strategies

### Query Result Caching

Cache frequently-requested queries:

```toml
[cache]
enabled = true
ttl_seconds = 300  # 5 minutes
max_entries = 10000
```

Which queries to cache:

- ✅ Static data (user profiles, configurations)
- ✅ Popular queries (top users, trending posts)
- ❌ Real-time data (current user auth, live notifications)
- ❌ Personalized data (user's own posts)

### Multi-Layer Caching

Implement caching at multiple levels:

```
Request
  ↓
Browser/CDN cache (1 day)
  ↓
Application cache (in-memory, 5 min)
  ↓
Redis cache (cross-server, 5 min)
  ↓
Database query
```

Performance impact:

- No cache: 50ms per query
- L1 cache (80% hit): 10ms average
- L1+L2 cache (95% hit): 5ms average
- Reduction: 90%

### Cache Invalidation

```rust
// When user updates profile
pub async fn update_user_profile(
    user_id: String,
    name: String,
    cache: &QueryCache,
    db: &Database,
) -> Result<User> {
    // Update in database
    let user = db.update_user(&user_id, &name).await?;

    // Invalidate related caches
    cache.invalidate_pattern(&format!("user:{}:*", user_id)).await;
    cache.invalidate_pattern("users:*").await;

    Ok(user)
}
```

---

## Part 4: Profiling & Diagnostics

### CPU Profiling

Find where CPU time is spent:

```bash
# Using flamegraph
cargo install flamegraph
sudo cargo flamegraph --bin fraiseql-server

# Generates flamegraph.svg showing call stacks
```

Common bottlenecks:

- JSON serialization: Optimize field selection
- Database queries: Add indexes
- GraphQL validation: Simplify complex schemas

### Memory Profiling

Find memory leaks or high usage:

```bash
# Monitor memory over time
watch -n 1 'ps aux | grep fraiseql | grep -v grep'

# If memory keeps growing:
# 1. Check connection pool isn't leaking
# 2. Check cache isn't unbounded
# 3. Profile with valgrind
```

### Query Analysis

Identify slow queries:

```bash
# Enable slow query logging
curl -X POST http://localhost:8080/admin/config \
  -H "Content-Type: application/json" \
  -d '{
    "slow_query_threshold_ms": 100
  }'

# Check logs for queries > 100ms
grep "SLOW QUERY" /var/log/fraiseql/error.log
```

---

## Part 5: Scaling to High Throughput

### 100 req/sec

Single instance, simple queries:

```toml
[database]
pool_min = 5
pool_max = 10

[query]
max_complexity = 1000
cache_ttl = 300
```

### 1,000 req/sec

3-5 instances, moderate queries:

```toml
# Each instance
[database]
pool_min = 10
pool_max = 30

[query]
max_complexity = 5000
cache_ttl = 60

# Plus:

- Read replicas for database
- Redis for cross-server caching
- CDN for static content
```

### 10,000 req/sec

10-20 instances, aggressive optimization:

```toml
# Each instance
[database]
pool_min = 20
pool_max = 100

[query]
max_complexity = 10000
cache_ttl = 30

# Plus:

- Database sharding by user_id
- Redis cluster for caching
- GraphQL query batching
- APQ (Automatic Persisted Queries)
```

### 100,000+ req/sec

Large-scale deployment:

```
┌─────────────┐
│  CDN/Cache  │ (Global caching)
├─────────────┤
│  Load Balancer (Geographic)
├─────────────┤
│ Region 1 (50k req/sec)
│ - 20 instances
│ - Dedicated database
│ - Redis cluster
├─────────────┤
│ Region 2 (50k req/sec)
│ - 20 instances
│ - Dedicated database
│ - Redis cluster
└─────────────┘
```

---

## Part 6: Monitoring Performance

### Key Dashboards

Create Grafana dashboards to monitor:

1. **Request Latency**
   - P50, P95, P99 percentiles
   - Histogram of request times
   - Trend over last 24h

2. **Throughput**
   - Requests per second
   - Error rate
   - Database query rate

3. **Resource Usage**
   - CPU utilization
   - Memory usage
   - Connection pool status
   - Disk I/O

4. **Database Health**
   - Query execution time
   - Number of active connections
   - Slow query count
   - Index usage

### Setting Alerts

```yaml
# Alert if latency is high

- alert: HighQueryLatency
  expr: fraiseql_query_duration_seconds_p99 > 1.0
  for: 5m
  annotations:
    summary: "Query latency above 1s"

# Alert if error rate is high

- alert: HighErrorRate
  expr: rate(fraiseql_query_errors_total[5m]) > 0.01
  for: 5m
  annotations:
    summary: "Error rate above 1%"

# Alert if connection pool is full

- alert: ConnectionPoolExhausted
  expr: fraiseql_connection_pool_active >= fraiseql_connection_pool_max
  for: 2m
  annotations:
    summary: "Connection pool exhausted"
```

---

## Summary

You now know how to:

✅ Measure performance with metrics and benchmarks
✅ Optimize database with indexes and tuning
✅ Configure application for your workload
✅ Scale vertically and horizontally
✅ Implement multi-layer caching
✅ Profile and diagnose bottlenecks
✅ Scale to high throughput
✅ Monitor performance continuously

## Performance Checklist

- [ ] Database indexes created for frequently filtered fields
- [ ] Connection pool sized for expected load
- [ ] Query complexity limits appropriate for use case
- [ ] Caching enabled for static/popular queries
- [ ] Load testing performed with realistic workload
- [ ] Monitoring dashboards created
- [ ] Performance alerts configured
- [ ] Scaling strategy documented
- [ ] Regular performance reviews scheduled
- [ ] Capacity planning done for growth

## Next Steps

- **Need help with production?** → See current [operations guide](../../operations/guide.md)
- **Specific problem?** → Check [troubleshooting guide](../../TROUBLESHOOTING.md)

---

**Questions?** See [TROUBLESHOOTING.md](../../TROUBLESHOOTING.md) for FAQ and solutions, or open an issue on [GitHub](https://github.com/fraiseql/fraiseql).
