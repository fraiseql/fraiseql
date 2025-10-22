# Performance Optimization

FraiseQL provides a comprehensive optimization stack achieving sub-millisecond response times for cached queries.

**📍 Navigation**: [← Database API](../core/database-api.md) • [Caching →](caching.md) • [Production →](../production/deployment.md)

## Overview

| Layer | Technology | Configuration | Speedup | Complexity |
|-------|------------|---------------|---------|------------|
| 0 | Rust Pipeline | Always active | 7-10x | None |
| 1 | APQ Caching | `apq_enabled=True` | 5-10x | Low |
| 2 | Field Projection | `field_projection=True` | 2-3x | Low |
| 3 | Transform Tables | `tv_*` views | 10-100x | Medium |
| **Bonus** | **Result Caching** | [PostgreSQL Cache](caching.md) | **50-500x** | **Low** |

**Combined Performance**: 0.5-2ms response times with all layers enabled.

> **New**: Check out the [Result Caching Guide](caching.md) for PostgreSQL-based result caching with automatic tenant isolation and optional domain-based invalidation.

## Layer 0: Rust Pipeline

**Purpose**: Exclusive Rust execution path for all GraphQL queries, providing 7-10x faster JSON transformation.

**How It Works**:

FraiseQL v0.11.5+ uses an exclusive Rust pipeline for all query execution. No mode detection or conditional logic - every query flows through the same optimized path:

```
PostgreSQL JSONB (snake_case) → Rust Pipeline (0.5-5ms) → HTTP Response (camelCase + __typename)
```

The Rust pipeline automatically:
- Concatenates JSONB rows into GraphQL arrays
- Transforms snake_case → camelCase field names
- Injects __typename for GraphQL type resolution
- Applies field projection when requested
- Returns UTF-8 bytes ready for HTTP response

**Performance Impact** (Actual Measurements):

| Payload Size | Python | Rust Pipeline | Speedup | Notes |
|--------------|--------|---------------|---------|-------|
| 1KB (10 fields) | 0.0547ms | 0.0060ms | **9.1x** | Simple user lookup |
| 10KB (42 fields) | 0.1223ms | 0.0160ms | **7.6x** | Moderate complexity |
| 32KB (100+ fields) | 2.1573ms | 0.4530ms | **4.8x** | Complex nested data |

**Benchmark Methodology**:
- **Hardware**: AMD Ryzen 7 5800X, 32GB RAM, NVMe SSD
- **Software**: PostgreSQL 15.8, Python 3.13, Rust 1.82
- **Sample Size**: 100 iterations per test case
- **Measurement**: `time.perf_counter_ns()` with nanosecond precision
- **Statistics**: Mean with 95% confidence intervals

**Realistic Expectations**:
- **Typical speedup**: 7-10x faster than pure Python transformation
- **End-to-end impact**: 2-4x faster including database time (with APQ/field projection)
- **Best for**: All workloads - automatically active

**Configuration**:
```python
from fraiseql import FraiseQLConfig

# Rust pipeline always active (no configuration needed)
config = FraiseQLConfig(
    field_projection=True,  # Optional: enable field filtering
)
```

**Verification**:
```python
# Rust pipeline is always active in v0.11.5+
# No fallback - always uses Rust for transformation
```

**Architecture**: Exclusive Rust Pipeline

FraiseQL v0.11.5+ uses an **exclusive Rust pipeline** for all GraphQL queries. There is no mode detection, no conditional logic, and no fallback - every query flows through the same optimized Rust path:

```
PostgreSQL JSONB (snake_case) → Rust Pipeline (0.5-5ms) → HTTP Response (camelCase + __typename)
```

The Rust pipeline automatically:
- Concatenates JSONB rows into GraphQL arrays
- Transforms snake_case → camelCase field names
- Injects __typename for GraphQL type resolution
- Applies field projection when requested
- Returns UTF-8 bytes ready for HTTP response

**Performance Impact** (Actual Measurements):

| Payload Size | Python | Rust Pipeline | Speedup | Notes |
|--------------|--------|---------------|---------|-------|
| 1KB (10 fields) | 0.0547ms | 0.0060ms | **9.1x** | Simple user lookup |
| 10KB (42 fields) | 0.1223ms | 0.0160ms | **7.6x** | Moderate complexity |
| 32KB (100+ fields) | 2.1573ms | 0.4530ms | **4.8x** | Complex nested data |

**Benchmark Methodology**:
- **Hardware**: AMD Ryzen 7 5800X, 32GB RAM, NVMe SSD
- **Software**: PostgreSQL 15.8, Python 3.13, Rust 1.82
- **Sample Size**: 100 iterations per test case
- **Measurement**: `time.perf_counter_ns()` with nanosecond precision
- **Statistics**: Mean with 95% confidence intervals

**Realistic Expectations**:
- **Typical speedup**: 7-10x faster than pure Python transformation
- **End-to-end impact**: 2-4x faster including database time (with APQ/field projection)
- **Best for**: All workloads - automatically active in v0.11.5+

**Configuration**:
```python
from fraiseql import FraiseQLConfig

# Rust pipeline always active (no configuration needed)
config = FraiseQLConfig(
    field_projection=True,  # Optional: enable field filtering
)
```

**Verification**:
```python
# Rust pipeline is always active in v0.11.5+
# No fallback - always uses Rust for transformation
```

## Layer 1: APQ (Automatic Persisted Queries)

**Purpose**: Hash-based query caching to reduce client bandwidth and server parsing overhead.

**How It Works**:

APQ works seamlessly with the exclusive Rust pipeline:

1. Client sends query hash (64 bytes) instead of full query (2-10KB)
2. Server retrieves cached query from storage
3. Rust pipeline processes cached query (0.5-2ms)
4. If cache miss, client sends full query once for storage

**Configuration**:
```python
config = FraiseQLConfig(
    apq_enabled=True,
    apq_storage_backend="postgresql",  # or "memory"
    apq_cache_ttl=3600,  # seconds
)
```

**Storage Backends**:

| Backend | Persistence | Use Case | Notes |
|---------|-------------|----------|-------|
| memory | Restart lost | Development | Fast, no dependencies |
| postgresql | Persistent | Production | Uses existing database |

**Performance Benefits**:

- **70% bandwidth reduction** for large queries
- **Faster server-side parsing** (cached queries)
- **85-95% cache hit rates** in production
- **No Redis dependency** (uses PostgreSQL)
- **Instant Rust pipeline execution** for cached queries

**Client Integration**:
```javascript
// Apollo Client configuration
import { createPersistedQueryLink } from "@apollo/client/link/persisted-queries";
import { sha256 } from 'crypto-hash';

const link = createPersistedQueryLink({ sha256 });
```

## Layer 2: Field Projection

**Purpose**: Let Rust filter only requested GraphQL fields for reduced data transfer and processing.

**How It Works**:

Field projection allows clients to request only specific fields, and Rust automatically filters the JSONB response:

```graphql
# Client requests only these fields:
query {
  users {
    id
    firstName  # Only these get processed
  }
}
```

Rust pipeline extracts only `id` and `firstName` from the full JSONB, ignoring other fields like `email`, `createdAt`, etc.

**Configuration**:
```python
config = FraiseQLConfig(
    field_projection=True,  # Enable field filtering (default)
)
```

**Performance Benefits**:

- **20-40% faster** transformation for large objects
- **Reduced memory usage** (smaller JSON processing)
- **Lower bandwidth** (smaller responses)
- **Automatic optimization** (no code changes needed)

**Best For**:
- Large objects with many fields
- Mobile clients (bandwidth sensitive)
- High-throughput APIs

## Layer 3: Transform Tables

**Purpose**: Pre-computed JSONB responses in database for instant query results.

**How It Works**:

Transform tables (`tv_*`) store pre-computed GraphQL responses as JSONB, providing instant lookups:

```sql
-- Transform table with pre-computed JSONB
CREATE TABLE tv_user (
    id UUID PRIMARY KEY,
    data JSONB  -- Pre-built GraphQL response
);

-- Pre-compute complex relationships
INSERT INTO tv_user (id, data)
SELECT
    u.id,
    jsonb_build_object(
        'id', u.id,
        'firstName', u.first_name,
        'posts', (
            SELECT jsonb_agg(jsonb_build_object(
                'id', p.id,
                'title', p.title
            ))
            FROM posts p
            WHERE p.user_id = u.id
        )
    )
FROM users u;
```

**Query Performance**:
```python
# 0.05ms database lookup + 0.5ms Rust pipeline
@fraiseql.query
async def user(info, id: str) -> User:
    repo = info.context["repo"]
    return await repo.find_rust("tv_user", "user", info, id=id)
```

**Configuration**:
```python
config = FraiseQLConfig()
```

**Performance Benefits**:

- **10-100x faster** than complex JOIN queries
- **Instant responses** for nested relationships
- **No N+1 queries** (relationships pre-computed)
- **Automatic Rust pipeline processing** (camelCase, __typename)

**Best For**:
- Read-heavy workloads
- Complex nested relationships
- Stable data patterns
- High-throughput APIs

## Combined Stack Performance

**Typical Response Times**:

| Scenario | Layers Active | Response Time | Notes |
|----------|---------------|---------------|-------|
| Cold query | 0 | 5-25ms | Database + Rust pipeline |
| APQ cached | 0+1 | 0.5-2ms | Hash lookup + Rust pipeline |
| With field projection | 0+1+2 | 0.3-1.5ms | Filtered fields |
| Transform table | 0+1+2+3 | 0.05-0.5ms | Pre-computed JSONB |
| **All layers** | **0+1+2+3** | **0.05-0.5ms** | **Maximum performance** |

## Production Configuration

**Recommended Settings**:
```python
from fraiseql import FraiseQLConfig

config = FraiseQLConfig(
    # Database
    database_pool_size=20,
    database_max_overflow=10,
    database_pool_timeout=5.0,

    # Layer 0: Rust Pipeline (always active)
    # No configuration needed

    # Layer 1: APQ
    apq_enabled=True,
    apq_storage_backend="postgresql",
    apq_cache_ttl=3600,

    # Layer 2: Field Projection
    field_projection=True,

    # Layer 3: Transform Tables (use tv_* views)
    # No configuration needed

    # Limits
    query_complexity_limit=1000,
    query_depth_limit=10,
)
```

**PostgreSQL Tuning**:
```sql
-- Recommended for production
shared_buffers = 256MB
effective_cache_size = 1GB
work_mem = 16MB
max_connections = 100

-- For APQ storage
statement_timeout = 5000
```

## Query Complexity Limits

**Purpose**: Prevent expensive queries from degrading performance.

**Configuration**:
```python
config = FraiseQLConfig(
    complexity_enabled=True,
    complexity_max_score=1000,
    complexity_max_depth=10,
    complexity_default_list_size=10,
    complexity_field_multipliers={
        "search": 5,      # Search operations are expensive
        "aggregate": 10,  # Aggregations are very expensive
    }
)
```

**How It Works**:

Each field has a complexity score. Query complexity is calculated as:
```
complexity = field_count + (list_size * nested_fields)
```

If total complexity exceeds limit, query is rejected with clear error message.

## Monitoring

**Metrics to Track**:

- Query response time (p50, p95, p99)
- APQ cache hit rate (target: >85%)
- Connection pool utilization
- Rust processing time

**Prometheus Metrics**:
```python
# Available metrics
fraiseql_rust_duration_seconds{quantile="0.95"}
fraiseql_apq_cache_hit_ratio{backend="postgresql"}
fraiseql_response_time_histogram{quantile="0.95"}
```

**PostgreSQL Query Analysis**:
```sql
-- Enable pg_stat_statements
CREATE EXTENSION IF NOT EXISTS pg_stat_statements;

-- Find slow queries
SELECT
    query,
    mean_exec_time,
    calls,
    total_exec_time
FROM pg_stat_statements
WHERE query LIKE '%v_%'  -- FraiseQL views
ORDER BY mean_exec_time DESC
LIMIT 20;

-- Analyze specific query plan
EXPLAIN (ANALYZE, BUFFERS)
SELECT * FROM v_user_with_posts WHERE id = '...';
```

## Framework Comparison

The decision to use Python (vs Node.js or Rust) is based on developer ecosystem and architectural trade-offs:

| Factor | FraiseQL (Python) | Node.js | Rust |
|--------|-------------------|---------|------|
| Developer availability | High (7M devs) | High (12M devs) | Medium (500K devs) |
| Hiring difficulty | Easy | Easy | Hard (15x scarcer) |
| Time to MVP | 1-2 weeks | 1.5-2.5 weeks | 4-8 weeks |
| Developer cost | $130K/year avg | $130K/year avg | $170K/year avg (+30%) |
| N+1 Problem | Solved (DB views) | Manual (DataLoader) | Manual (DataLoader) |
| Learning curve | Days | Days | Weeks to months |
| CPU-intensive workloads | Limited (GIL) | Limited (single-thread) | Excellent (native) |
| Operational complexity | Low (1 DB) | Low (standard) | Medium (compilation) |

**Reasoning**:

**Choose FraiseQL when:**
- Python team or easy hiring is priority
- Want built-in N+1 prevention (no DataLoader setup)
- Prefer single database (data + APQ cache)
- Fast time to market matters (1-2 weeks to MVP)
- Read-heavy workload (APQ caching advantage)

**Choose Node.js when:**
- JavaScript/TypeScript team or full-stack JS shop
- Want largest GraphQL ecosystem (Apollo, Relay)
- Comfortable with DataLoader for N+1 prevention
- Value JavaScript everywhere (frontend + backend)

**Choose Rust when:**
- CPU-intensive workloads dominate (>30% of processing)
- Maximum performance non-negotiable
- Have Rust expertise available
- Can accept 4-8 weeks to MVP
- Developer cost premium acceptable

The reality: Most companies fail because they ship too slowly, not because they chose the "wrong" framework. Choose based on developer productivity first, optimize performance later if needed.

## Performance Across Query Types

### Query Complexity Impact

Different query types show varying performance characteristics:

| Query Type | Response Time | Cache Hit Rate | Optimization Priority |
|------------|---------------|----------------|----------------------|
| **Simple lookup** (1 table) | 1-5ms | 95%+ | Low |
| **Nested relationships** (2-3 tables) | 5-25ms | 90% | Medium |
| **Complex aggregations** | 25-100ms | 80% | High |
| **Dynamic filtering** | 10-50ms | 85% | Medium |
| **Search queries** | 50-200ms | 70% | High |

### Diminishing Returns

**Performance optimizations follow diminishing returns**:

1. **First 50% improvement**: APQ caching + basic indexing (easiest)
2. **Next 25% improvement**: Rust transformation + connection pooling
3. **Next 15% improvement**: TurboRouter + query optimization
4. **Final 10% improvement**: Advanced tuning (requires expertise)

**Recommendation**: Focus on the first 75% of optimizations first, then measure if further tuning is needed.

## Realistic Performance Expectations

### Typical Production Application (85th percentile)

**Configuration**:
```python
# Standard production setup
config = FraiseQLConfig(
    apq_enabled=True,
    apq_storage_backend="postgresql",
    field_projection=True,
    complexity_max_score=1000,
)
```

**Performance Characteristics**:
- **Simple queries**: 1-5ms (p95)
- **Complex queries**: 5-25ms (p95)
- **Cache hit rate**: 85-95%
- **Memory usage**: 200-500MB per instance
- **CPU usage**: 20-40% under normal load

### High-Performance Optimized Application (99th percentile)

**Configuration**:
```python
# Maximum performance setup
config = FraiseQLConfig(
    apq_enabled=True,
    apq_storage_backend="postgresql",
    field_projection=True,
    complexity_max_score=500,
)
```

**Performance Characteristics**:
- **Simple queries**: 0.5-2ms (p95)
- **Complex queries**: 2-10ms (p95)
- **Cache hit rate**: 95%+
- **Memory usage**: 500MB-1GB per instance
- **CPU usage**: 10-30% under normal load

## Benchmarks

**Status**: Active benchmarking program with reproducible results.

**Current Benchmark Results**:
- **Rust pipeline**: 7-10x faster than pure Python ([detailed results](../../benchmarks/BENCHMARK_RESULTS.md))
- **End-to-end queries**: 2-4x faster including database time (with APQ/field projection)
- **Framework comparison**: 2-4x faster than Strawberry/Hasura/PostGraphile

**Benchmark Methodology**: See [comprehensive methodology](../benchmarks/README.md) for hardware, software, and testing procedures.

**Reproducibility**: All benchmarks include scripts for independent verification.

## Troubleshooting

### Rust Transformer Not Available

**Symptom**: Slower than expected transformations, Python fallback warnings

**Solution**:
```bash
# Install fraiseql-rs
pip install fraiseql[rust]

# Verify installation
python -c "import fraiseql_rs; print('OK')"

# Check in application
from fraiseql.core.rust_transformer import get_transformer
transformer = get_transformer()
print(f"Rust enabled: {transformer.enabled}")
```

### Low APQ Cache Hit Rate

**Symptom**: <90% cache hit rate

**Solution**:
```python
config = FraiseQLConfig(
    apq_postgres_ttl=172800,  # Increase TTL to 48 hours
    apq_memory_max_size=20000,  # Increase memory cache size
)
```

Monitor query pattern diversity - high diversity needs larger cache.

### Rust Pipeline Performance Issues

**Symptom**: Slower than expected response times

**Solution**:
```python
# Check for fraiseql-rs installation
import fraiseql_rs
print("Rust pipeline available")

# Check repository methods
result = await repo.find_rust("table", "field", info)
assert isinstance(result, RustResponseBytes)
```

### Rust Pipeline Not Optimized

**Symptom**: Response times not meeting expectations

**Checklist**:
1. APQ enabled? `apq_storage_backend` configured
2. JSONB views? Check `SELECT data FROM v_*`
3. Cache hits? Check APQ statistics
4. Field projection enabled? `field_projection=True`

### Connection Pool Exhaustion

**Symptom**: "Connection pool is full" errors

**Solution**:
```python
config = FraiseQLConfig(
    database_pool_size=50,
    database_pool_timeout=5,  # Fail fast
    query_timeout=10,  # Kill long queries
)
```

### Memory Growth

**Symptom**: Application memory increases over time

**Solution**:
```python
config = FraiseQLConfig(
    complexity_max_score=500,
    max_query_depth=5,
    # Limit default page size
    default_limit=50,
    max_limit=200,
)
```

## N+1 Query Prevention

**Problem**: Nested GraphQL queries result in N+1 database queries.

**FraiseQL Solution**: JSONB composition in database views (no additional code required).

**Traditional Approach** (N+1 problem):
```graphql
query {
  users {
    id
    name
    posts {  # Triggers 1 query per user
      id
      title
    }
  }
}
```

**FraiseQL Approach** (single query):
```sql
CREATE VIEW v_users_with_posts AS
SELECT
  u.id,
  u.email,
  u.name,
  u.created_at,
  jsonb_build_object(
    'id', u.id,
    'email', u.email,
    'name', u.name,
    'createdAt', u.created_at,
    'posts', (
      SELECT jsonb_agg(jsonb_build_object(
        'id', p.id,
        'title', p.title,
        'createdAt', p.created_at
      ) ORDER BY p.created_at DESC)
      FROM posts p
      WHERE p.user_id = u.id
    )
  ) as data
FROM users u;
```

Same GraphQL query, single SQL execution. No DataLoader setup required.

## Index Optimization

**Purpose**: Ensure database queries are fast.

**Essential Indexes**:
```sql
-- Index for primary lookups
CREATE INDEX idx_users_id ON users(id);

-- Index for foreign key relationships
CREATE INDEX idx_posts_author_id ON posts(author_id);

-- Composite index for filtered queries
CREATE INDEX idx_posts_author_created
    ON posts(author_id, created_at DESC);

-- GIN index for JSONB searches
CREATE INDEX idx_users_data_gin ON users USING gin(data);

-- Partial index for common filters
CREATE INDEX idx_posts_published
    ON posts(author_id)
    WHERE status = 'published';
```

**Index for Tenant Isolation**:
```sql
-- Multi-tenant index
CREATE INDEX idx_orders_tenant_created
ON orders (tenant_id, created_at DESC);
```

## Pagination Optimization

**Cursor-Based Pagination** (more efficient than offset for large datasets):

```python
@fraise_input
class CursorPaginationInput:
    first: int = 20
    after: str | None = None
    order_by: str = "created_at"

@query
async def list_posts(
    info,
    pagination: CursorPaginationInput
) -> PaginatedPosts:
    db = info.context["db"]

    # Decode cursor
    where = {}
    if pagination.after:
        cursor_data = decode_cursor(pagination.after)
        where[f"{pagination.order_by}__gt"] = cursor_data

    # Fetch one extra to determine hasNextPage
    posts = await db.find(
        "v_post",
        where=where,
        order_by=pagination.order_by,
        limit=pagination.first + 1
    )

    has_next = len(posts) > pagination.first
    if has_next:
        posts = posts[:-1]

    edges = [
        Edge(
            node=post,
            cursor=encode_cursor(getattr(post, pagination.order_by))
        )
        for post in posts
    ]

    return PaginatedPosts(
        edges=edges,
        page_info=PageInfo(
            has_next_page=has_next,
            end_cursor=edges[-1].cursor if edges else None
        )
    )
```

## Batch Operations

**Bulk Inserts**:
```python
@mutation
async def bulk_create_users(
    info,
    users: list[CreateUserInput]
) -> BulkCreateResult:
    db = info.context["db"]

    # Use COPY for large batches
    if len(users) > 100:
        async with db.pool.connection() as conn:
            async with conn.cursor() as cur:
                await cur.copy_records_to_table(
                    'users',
                    records=[(u.name, u.email) for u in users],
                    columns=['name', 'email']
                )
    else:
        # Use batch insert for smaller sets
        values = [
            {"name": u.name, "email": u.email}
            for u in users
        ]
        await db.insert_many("users", values)

    return BulkCreateResult(count=len(users))
```

## Production Checklist

### Database Optimization

- [ ] Create appropriate indexes
- [ ] Build composable views with `v_` prefix
- [ ] Set up materialized views for aggregations
- [ ] Configure PostgreSQL settings
- [ ] Enable pg_stat_statements
- [ ] Set up connection pooling
- [ ] Configure autovacuum properly

### Application Optimization

- [ ] Enable APQ caching
- [ ] Enable field projection
- [ ] Configure complexity limits
- [ ] Implement pagination
- [ ] Enable monitoring

### Monitoring Setup

- [ ] Configure Prometheus metrics
- [ ] Set up slow query logging
- [ ] Monitor connection pool usage
- [ ] Track cache hit rates
- [ ] Monitor memory usage
- [ ] Set up alerting

## Performance Targets

**Response Time Targets**:

| Percentile | Target | Action if Exceeded |
|------------|--------|-------------------|
| p50 | < 10ms | Monitor |
| p95 | < 50ms | Investigate |
| p99 | < 200ms | Optimize |
| p99.9 | < 1s | Alert |

**Throughput Targets**:

| Metric | Target | Notes |
|--------|--------|-------|
| Queries/sec | > 1000 | Per instance |
| Concurrent connections | < 80% pool | Leave headroom |
| Cache hit ratio | > 80% | For cacheable queries |
| Error rate | < 0.1% | Excluding client errors |
