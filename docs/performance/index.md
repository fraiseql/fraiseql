# Performance Optimization

FraiseQL provides a four-layer optimization stack achieving sub-millisecond response times for cached queries.

## Overview

| Layer | Technology | Configuration | Speedup | Complexity |
|-------|------------|---------------|---------|------------|
| 0 | Rust Transformation | `pip install fraiseql[rust]` | 10-80x | Low |
| 1 | APQ Caching | `apq_enabled=True` | 5-10x | Low |
| 2 | TurboRouter | Query registration | 3-5x | Medium |
| 3 | JSON Passthrough | View design | 2-3x | Medium |

**Combined Performance**: 0.5-2ms response times with all layers enabled.

## Layer 0: Rust Transformation

**Purpose**: Accelerate JSON transformation from PostgreSQL to GraphQL format using native Rust code.

**Installation**:
```bash
pip install fraiseql[rust]
```

**How It Works**:

The Rust transformer is FraiseQL's foundational performance layer that uses **fraiseql-rs** (a Rust extension module built with PyO3) to provide:

- **Zero-copy JSON parsing** with serde_json
- **High-performance schema registry** for type-aware transformations
- **GIL-free execution** - Rust code runs without Python's Global Interpreter Lock
- **Automatic fallback** - Graceful degradation to Python when unavailable

All GraphQL types are automatically registered with the Rust transformer during schema building. When queries execute, JSON results from PostgreSQL are transformed via Rust:

```
PostgreSQL JSONB (snake_case) → Rust Transform (0.2-2ms) → GraphQL JSON (camelCase + __typename)
```

**Performance Impact**:

| Payload Size | Python | Rust | Speedup |
|--------------|--------|------|---------|
| 1KB | 15ms | 0.2ms | **75x** |
| 10KB | 50ms | 2ms | **25x** |
| 100KB | 450ms | 25ms | **18x** |

**Automatic Fallback**:

If Rust binary unavailable, automatically falls back to Python implementation with no code changes required.

**Configuration**:
```python
from fraiseql import FraiseQLConfig

# Rust enabled by default if installed
config = FraiseQLConfig(
    rust_enabled=True,  # Default: True
)
```

**Verification**:
```python
from fraiseql.core.rust_transformer import get_transformer

transformer = get_transformer()
if transformer.enabled:
    print("Rust transformer active")
else:
    print("Using Python fallback")
```

## Layer 1: APQ (Automatic Persisted Queries)

**Purpose**: Hash-based query caching to reduce client bandwidth and server parsing overhead.

**How It Works**:

APQ eliminates network overhead by replacing large GraphQL queries with small SHA-256 hashes:

1. Client sends query hash (64 bytes) instead of full query (2-10KB)
2. Server retrieves cached query from storage
3. If cache miss, client sends full query once
4. Subsequent requests use hash only

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
- **99.9% cache hit rates** in production
- **No Redis dependency** (uses PostgreSQL)

**Client Integration**:
```javascript
// Apollo Client configuration
import { createPersistedQueryLink } from "@apollo/client/link/persisted-queries";
import { sha256 } from 'crypto-hash';

const link = createPersistedQueryLink({ sha256 });
```

## Layer 2: TurboRouter

**Purpose**: Pre-compiled GraphQL-to-SQL routing for registered queries.

**How It Works**:

TurboRouter bypasses GraphQL parsing by pre-compiling frequently used queries to SQL templates:

```python
from fraiseql.fastapi import TurboRegistry, TurboQuery

registry = TurboRegistry(max_size=1000)

user_by_id = TurboQuery(
    graphql_query="""
        query GetUser($id: UUID!) {
            getUser(id: $id) { id name email }
        }
    """,
    sql_template="""
        SELECT id::text, name, email
        FROM v_user
        WHERE id = %(id)s
    """,
    param_mapping={"id": "id"}
)
registry.register(user_by_id)

app = create_fraiseql_app(
    config=config,
    turbo_registry=registry
)
```

**Configuration**:
```python
config = FraiseQLConfig(
    enable_turbo_router=True,
    turbo_router_cache_size=500,
    turbo_enable_adaptive_caching=True,
)
```

**Performance Benefits**:

- **4-10x faster** than standard GraphQL execution
- **Predictable latency** with pre-compiled queries
- **Lower CPU usage** (no parsing overhead)
- **Automatic fallback** to standard mode for unregistered queries

**Tenant-Aware Caching**:
```python
# TurboRouter supports multi-tenant caching patterns
# Cache keys automatically include tenant context
```

## Layer 3: JSON Passthrough

**Purpose**: Zero-copy JSON responses from database to client.

**How It Works**:

JSON Passthrough eliminates Python object instantiation and serialization overhead by returning PostgreSQL JSONB directly:

```python
# Standard Mode (with object instantiation)
# PostgreSQL JSONB � Python objects � GraphQL serialization � JSON
# Overhead: 5-25ms

# Passthrough Mode (direct JSON)
# PostgreSQL JSONB � Rust transform � JSON
# Overhead: 0.2-2ms (with Rust)
```

**Database View Pattern**:
```sql
CREATE VIEW v_orders_json AS
SELECT
  o.tenant_id,
  jsonb_build_object(
    'id', o.id,
    'total', o.total,
    'status', o.status,
    'items', (
      SELECT jsonb_agg(jsonb_build_object(
        'id', i.id,
        'name', i.name,
        'quantity', i.quantity
      ))
      FROM order_items i
      WHERE i.order_id = o.id
    )
  ) as data
FROM orders o;
```

**Configuration**:
```python
config = FraiseQLConfig(
    json_passthrough_enabled=True,  # Default: True
    passthrough_complexity_limit=50,
    passthrough_max_depth=3,
)
```

**Performance Benefits**:

- **5-20x faster** than object instantiation
- **Sub-millisecond cached responses**
- **Lower memory usage** (no object creation)
- **Composable with N+1 prevention** (database views)

**Requirements**:

- Views must return JSONB in `data` column
- APQ caching enabled for maximum benefit
- Compatible with all optimization layers

## Combined Stack Performance

**Typical Response Times**:

| Scenario | Layers Active | Response Time | Notes |
|----------|---------------|---------------|-------|
| Cold query (Python) | 0 | 100-300ms | First execution, no cache |
| Cold query (Rust) | 0 | 80-280ms | 1.2-1.5x faster |
| APQ cached | 0+1 | 50-150ms (Python) | Hash lookup + execution |
| APQ cached + Rust | 0+1 | 30-130ms | 2-3x faster |
| TurboRouter | 0+2 | 5-45ms | Pre-compiled query |
| Passthrough | 0+3 | 1-5ms (Rust) | Direct JSON |
| APQ + TurboRouter | 0+1+2 | 1-5ms | Query cache + pre-compilation |
| **All layers** | **0+1+2+3** | **0.5-2ms** | **Maximum performance** |

## Production Configuration

**Recommended Settings**:
```python
from fraiseql import FraiseQLConfig

config = FraiseQLConfig(
    # Database
    database_pool_size=20,
    database_max_overflow=10,
    database_pool_timeout=5.0,

    # Layer 0: Rust (automatic if installed)
    rust_enabled=True,

    # Layer 1: APQ
    apq_enabled=True,
    apq_storage_backend="postgresql",
    apq_cache_ttl=3600,

    # Layer 2: TurboRouter
    enable_turbo_router=True,
    turbo_router_cache_size=500,
    turbo_enable_adaptive_caching=True,

    # Layer 3: JSON Passthrough
    json_passthrough_enabled=True,
    passthrough_complexity_limit=50,

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
- APQ cache hit rate (target: >95%)
- Connection pool utilization
- Rust transformation time
- TurboRouter hit rate

**Prometheus Metrics**:
```python
# Available metrics
fraiseql_rust_transformer_enabled{environment="production"}
fraiseql_rust_transform_duration_seconds{quantile="0.95"}
fraiseql_apq_cache_hit_ratio{backend="postgresql"}
fraiseql_turbo_router_hit_ratio{environment="production"}
fraiseql_passthrough_usage_ratio{complexity_limit="50"}
fraiseql_response_time_histogram{mode="turbo", quantile="0.95"}
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

## Benchmarks

**Status**: Independent benchmarks pending.

Performance claims in this document are based on:
- Rust transformation: Measured (10-80x vs Python)
- APQ benefits: Architecture-based (hash vs full query)
- TurboRouter: Architecture-based (pre-compilation)
- Combined stack: Production experience (0.5-2ms observed)

Comprehensive independent benchmarks comparing FraiseQL to other frameworks will be published when available.

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

### TurboRouter Underutilization

**Symptom**: <50% turbo execution rate

**Solution**:
```sql
-- Identify hot queries for registration
SELECT query_hash, COUNT(*) as frequency
FROM query_logs
WHERE created_at > NOW() - INTERVAL '7 days'
GROUP BY query_hash
ORDER BY frequency DESC
LIMIT 20;
```

```python
# Increase cache size
config.turbo_router_cache_size = 2000

# Enable adaptive caching
config.turbo_enable_adaptive_caching = True
```

### Passthrough Not Activating

**Symptom**: Response times still 20-50ms

**Checklist**:
1. APQ enabled? `apq_storage_backend` configured
2. JSONB views? Check `SELECT data FROM v_*`
3. Cache hits? Check APQ statistics
4. TurboRouter enabled? `enable_turbo_router=True`

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

- [ ] Install Rust extensions (`pip install fraiseql[rust]`)
- [ ] Enable APQ caching
- [ ] Register hot queries in TurboRouter
- [ ] Enable JSON passthrough
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
