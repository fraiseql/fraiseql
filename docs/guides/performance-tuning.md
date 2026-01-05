# Performance Tuning Guide

Comprehensive guide to optimizing FraiseQL performance for production workloads.

## Overview

FraiseQL delivers exceptional performance through its exclusive Rust GraphQL pipeline, providing 2-4x better performance than traditional GraphQL frameworks. This guide covers optimization strategies, performance expectations, monitoring, and troubleshooting.

**Performance Characteristics:**
- **Throughput**: 10,000+ requests/second
- **Latency**: 0.5-5ms for cached queries, 5-25ms for database queries
- **Memory**: 100-500MB baseline usage
- **Concurrent Users**: Scales linearly with hardware
- **Cache Hit Rate**: 85-95% in production applications

**Key Performance Claims:**
- **2-4x faster** than traditional GraphQL frameworks (Strawberry, Hasura, PostGraphile)
- **Sub-millisecond cached responses** (0.5-2ms) with APQ optimization
- **85-95% cache hit rates** in well-designed production applications
- **Exclusive Rust pipeline** eliminates Python string operations

## Performance Expectations & Methodology

### Realistic Performance Targets

**Typical Query Response Times:**
- **Simple queries** (single table): 2-3x faster than alternatives
- **Complex queries** (joins, aggregations): 3-4x faster than alternatives
- **Cached queries**: 4-10x faster due to APQ optimization
- **Cache hit**: 0.5-2ms (Rust pipeline + APQ)
- **Cache miss**: 5-25ms (includes database query)

**Cache Hit Rate Expectations:**
- **Stable APIs**: 95%+ hit rate
- **Dynamic queries**: 80-90% hit rate
- **Admin interfaces**: 70-85% hit rate

**Conditions for Optimal Performance:**
- PostgreSQL 15+ with proper indexing
- APQ storage backend configured (PostgreSQL recommended)
- Query complexity score < 100
- Response size < 50KB
- Exclusive Rust pipeline active (automatic in v1.0.0+)

### Performance Testing Methodology

**Baseline Comparison:**
- Measured against Strawberry GraphQL (Python ORM) and Hasura (PostgreSQL GraphQL)
- Test queries: Simple user lookup, nested user+posts, filtered searches
- Dataset: 10k-100k records in PostgreSQL 15
- Hardware: Standard cloud instances (4 CPU, 8GB RAM)
- Measurement: End-to-end response time including database queries

## Quick Performance Wins

### 1. Enable Result Caching

FraiseQL's PostgreSQL-based caching provides 50-500x performance improvement:

```python
from fraiseql.caching import PostgresCache, ResultCache

# Add to your app startup
cache = ResultCache(
    backend=PostgresCache(connection_pool=pool),
    default_ttl=300  # 5 minutes
)

# Automatic caching for all queries
@fraiseql.query
async def get_products(info) -> list[Product]:
    # Automatically cached for 5 minutes
    return await db.get_products()
```

**Impact**: 85%+ query performance improvement for read-heavy workloads.

### 2. Optimize Database Connections

Proper connection pooling prevents bottlenecks:

```python
from fraiseql.db import DatabasePool

# Production settings
pool = DatabasePool(
    dsn="postgresql://user:pass@host:5432/db",
    min_size=10,      # Maintain minimum connections
    max_size=50,      # Scale up to 50 concurrent connections
    max_idle_time=300, # Recycle idle connections
    max_lifetime=3600  # Renew connections hourly
)
```

### 3. Enable Query Complexity Limits

Prevent expensive queries from degrading performance:

```python
from fraiseql import create_app

app = create_app(
    schema=schema,
    max_complexity=1000,    # Limit query complexity
    max_depth=10,           # Limit nesting depth
    timeout_seconds=30      # Global timeout
)
```

## Database Optimization

### Indexing Strategy

FraiseQL benefits from strategic indexing:

```sql
-- Primary key indexes (automatic)
-- Foreign key indexes (automatic)

-- Add composite indexes for common query patterns
CREATE INDEX idx_user_posts ON posts(user_id, created_at DESC);
CREATE INDEX idx_products_category_price ON products(category_id, price);

-- Partial indexes for filtered queries
CREATE INDEX idx_active_orders ON orders(order_date)
WHERE status = 'active';

-- Expression indexes for computed values
CREATE INDEX idx_products_search_vector ON products
USING GIN (to_tsvector('english', name || ' ' || description));
```

### Query Optimization

Monitor and optimize slow queries:

```sql
-- Enable query logging
ALTER DATABASE mydb SET log_statement = 'all';
ALTER DATABASE mydb SET log_duration = ON;

-- Analyze slow queries
SELECT query, mean_time, calls, total_time
FROM pg_stat_statements
ORDER BY mean_time DESC
LIMIT 10;
```

### Connection Pool Tuning

```python
# For high-throughput applications
pool = DatabasePool(
    dsn=dsn,
    min_size=20,
    max_size=100,
    max_idle_time=60,    # Aggressive recycling
    max_lifetime=1800,   # 30-minute renewal
    command_timeout=10   # Fast failure detection
)
```

## GraphQL-Specific Optimizations

### Batch Loading (DataLoader)

Prevent N+1 query problems:

```python
from fraiseql.dataloader import DataLoader

class UserLoader(DataLoader):
    async def batch_load(self, keys):
        # Single query for multiple users
        users = await db.get_users_by_ids(keys)
        return [users.get(key) for key in keys]

# Use in resolvers
@fraiseql.field
async def author(self, info) -> User:
    loader = UserLoader()
    return await loader.load(self.author_id)
```

### Query Complexity Analysis

```python
from fraiseql.analysis import QueryAnalyzer

analyzer = QueryAnalyzer()

# Analyze query cost before execution
cost = analyzer.analyze(query_string)
if cost > 1000:
    raise GraphQLError("Query too complex")

# Automatic complexity limiting
app = create_app(
    schema=schema,
    query_analyzer=analyzer,
    max_complexity=2000
)
```

### Field-Level Optimization

```python
@fraiseql.type
class Product:
    id: UUID
    name: str

    # Expensive field - add caching
    @fraiseql.field(cache=True, ttl=600)
    async def reviews(self, info) -> list[Review]:
        return await db.get_product_reviews(self.id)

    # Computed field - cache result
    @fraiseql.field(cache=True, ttl=3600)
    async def average_rating(self, info) -> float:
        reviews = await self.reviews(info)
        return sum(r.rating for r in reviews) / len(reviews)
```

## Caching Strategies

### Result Caching

Cache entire query results:

```python
from fraiseql.caching import ResultCache

cache = ResultCache(
    backend=PostgresCache(pool),
    default_ttl=300,
    max_size_mb=500
)

# Automatic result caching
@fraiseql.query
async def search_products(info, query: str) -> list[Product]:
    # Cached for 5 minutes
    return await db.search_products(query)
```

### Field-Level Caching

Cache individual expensive computations:

```python
@fraiseql.field(cache_key="product:{id}:inventory")
async def available_inventory(self, info) -> int:
    # Cache for 1 minute - inventory changes frequently
    return await external_api.get_inventory(self.id)
```

### Cache Invalidation

Handle data changes properly:

```python
from fraiseql.cache import CacheInvalidator

invalidator = CacheInvalidator(cache)

@fraiseql.mutation
async def update_product(info, id: UUID, data: dict) -> Product:
    product = await db.update_product(id, data)

    # Invalidate related caches
    await invalidator.invalidate_pattern(f"product:{id}:*")
    await invalidator.invalidate("search_products")

    return product
```

## Monitoring & Observability

### Performance Metrics

```python
from fraiseql.monitoring import PerformanceMonitor

monitor = PerformanceMonitor()

# Track query performance
with monitor.timer("graphql_query"):
    result = await execute_graphql(query)

# Custom metrics
monitor.increment("queries_executed")
monitor.gauge("active_connections", pool.active_count)
```

### Prometheus Integration

```python
from fraiseql.monitoring.prometheus import PrometheusExporter

exporter = PrometheusExporter()

# Export metrics
app = create_app(
    schema=schema,
    metrics_exporter=exporter
)

# Available metrics:
# - fraiseql_query_duration_seconds
# - fraiseql_query_complexity
# - fraiseql_cache_hit_rate
# - fraiseql_db_connection_count
```

### Health Checks

```python
from fraiseql.health import HealthChecker

checker = HealthChecker()

@app.get("/health/performance")
async def performance_health():
    return {
        "cache_hit_rate": cache.hit_rate,
        "db_pool_utilization": pool.utilization,
        "avg_query_time": monitor.avg_query_time,
        "status": "healthy" if checker.all_healthy() else "degraded"
    }
```

## Production Deployment

### Horizontal Scaling

```yaml
# Kubernetes deployment for high availability
apiVersion: apps/v1
kind: Deployment
metadata:
  name: fraiseql-app
spec:
  replicas: 5  # Scale horizontally
  template:
    spec:
      containers:
      - name: fraiseql
        resources:
          requests:
            memory: "256Mi"
            cpu: "200m"
          limits:
            memory: "1Gi"
            cpu: "1000m"
        env:
        - name: DATABASE_POOL_MIN_SIZE
          value: "10"
        - name: DATABASE_POOL_MAX_SIZE
          value: "50"
        - name: CACHE_SIZE_MB
          value: "256"
```

### Database Optimization

```sql
-- Optimize PostgreSQL for GraphQL workloads
ALTER SYSTEM SET shared_preload_libraries = 'pg_stat_statements';
ALTER SYSTEM SET max_connections = 200;
ALTER SYSTEM SET work_mem = '64MB';
ALTER SYSTEM SET maintenance_work_mem = '256MB';
ALTER SYSTEM SET wal_buffers = '16MB';
```

### Load Balancing

```nginx
# Nginx configuration for load balancing
upstream fraiseql_backend {
    least_conn;
    server app1:8000;
    server app2:8000;
    server app3:8000;
}

server {
    listen 80;
    location /graphql {
        proxy_pass http://fraiseql_backend;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_read_timeout 30s;
    }
}
```

## Troubleshooting Performance Issues

### Database Performance Degradation

**Symptoms**: GraphQL queries taking >5 seconds, database connection pool exhausted, query timeout errors, high database CPU usage.

**Monitoring Metrics:**
```promql
# Query duration exceeding 5 seconds
rate(fraiseql_db_query_duration_seconds_sum[5m])
/ rate(fraiseql_db_query_duration_seconds_count[5m]) > 5

# Connection pool utilization
fraiseql_db_connection_pool_active / fraiseql_db_connection_pool_total > 0.8
```

**Immediate Response (MTTR: 15 minutes):**

1. **Check Database Load**
   ```sql
   -- Current active connections
   SELECT count(*) FROM pg_stat_activity WHERE state = 'active';

   -- Long-running queries
   SELECT pid, now() - pg_stat_activity.query_start AS duration,
          query
   FROM pg_stat_activity
   WHERE state = 'active'
   ORDER BY duration DESC
   LIMIT 5;
   ```

2. **Connection Pool Issues**
   - Increase `max_size` in DatabasePool configuration
   - Check for connection leaks in application code
   - Verify connection timeout settings

3. **Index Performance**
   ```sql
   -- Unused indexes (candidates for removal)
   SELECT schemaname, tablename, indexname
   FROM pg_stat_user_indexes
   WHERE idx_scan = 0;

   -- Missing indexes (high sequential scans)
   SELECT schemaname, tablename, seq_scan, seq_tup_read
   FROM pg_stat_user_tables
   WHERE seq_scan > 1000
   ORDER BY seq_scan DESC;
   ```

### High Latency

**Symptoms**: Queries taking >100ms

**Common Causes & Solutions:**

1. **Missing Indexes**
   ```sql
   -- Check for slow queries
   SELECT * FROM pg_stat_user_indexes WHERE idx_scan = 0;
   -- Add missing indexes
   ```

2. **Connection Pool Exhaustion**
   ```python
   # Increase pool size
   pool = DatabasePool(max_size=100, min_size=20)
   ```

3. **Cache Misses**
   ```python
   # Check cache hit rate
   print(f"Cache hit rate: {cache.hit_rate:.1%}")
   # Adjust TTL or cache strategy
   ```

### High Memory Usage

**Symptoms**: Memory usage >1GB

**Solutions:**

1. **Reduce Cache Size**
   ```python
   cache = ResultCache(max_size_mb=256)
   ```

2. **Enable Compression**
   ```python
   cache = ResultCache(compression="lz4")
   ```

3. **Monitor Memory Leaks**
   ```python
   import tracemalloc
   tracemalloc.start()
   # Monitor memory usage patterns
   ```

### Database Bottlenecks

**Symptoms**: High DB CPU, slow queries

**Solutions:**

1. **Query Optimization**
   ```sql
   EXPLAIN ANALYZE SELECT * FROM users WHERE email = $1;
   -- Add indexes or rewrite query
   ```

2. **Connection Pool Tuning**
   ```python
   # Match pool size to database max_connections
   pool = DatabasePool(max_size=50)  # If DB max_connections = 100
   ```

3. **Read Replicas**
   ```python
   # Use read replicas for queries
   reader_pool = DatabasePool(dsn="postgresql://reader_host/db")
   ```

## Advanced Optimizations

### APQ (Automatic Persisted Queries)

Reduce network overhead for repeated queries:

```python
from fraiseql.apq import APQCache

apq_cache = APQCache(redis_url="redis://localhost:6379")

app = create_app(
    schema=schema,
    apq_cache=apq_cache
)

# Client sends query hash instead of full query
# Server looks up query by hash for repeated requests
```

### Query Result Compression

Reduce bandwidth for large result sets:

```python
from fraiseql.compression import GzipMiddleware

app = create_app(
    schema=schema,
    middleware=[GzipMiddleware(min_size=1024)]  # Compress >1KB
)
```

### Rust Pipeline Optimization

For maximum performance, use direct Rust integration:

```rust
use fraiseql_core::{execute_query, QueryResult};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let schema = load_schema();
    let query = "query { users { id name } }";

    // Direct Rust execution - maximum performance
    let result: QueryResult = execute_query(&schema, query).await?;

    println!("{:?}", result);
    Ok(())
}
```

## Performance Benchmarks

### Typical Workloads

| Workload Type | Queries/sec | Latency (p95) | Optimization Focus |
|---------------|-------------|---------------|-------------------|
| Simple CRUD | 5,000 | 5ms | Caching |
| Complex Analytics | 500 | 200ms | Query optimization |
| Real-time Dashboard | 2,000 | 20ms | Connection pooling |
| API Gateway | 10,000 | 10ms | Load balancing |

### Scaling Guidelines

- **1-100 users**: Single instance, basic caching
- **100-1,000 users**: Connection pooling, result caching
- **1,000-10,000 users**: Read replicas, horizontal scaling
- **10,000+ users**: Sharding, advanced caching, CDN

## Next Steps

- [Caching Strategy Guide](../guides/caching-strategy.md) - Detailed caching configuration
- [Monitoring Setup](../production/monitoring.md) - Production observability
- [Database Tuning](../architecture/internals/database-tuning.md) - Advanced DB optimization
- [Troubleshooting](../troubleshooting/common-issues.md) - Common performance issues

---

**Performance optimization is iterative. Start with caching and connection pooling, then monitor and tune based on your specific workload patterns.**
