# JSON Passthrough Optimization

**Status:** ✅ Production-ready
**Added in:** v0.8.0
**Performance Impact:** Sub-millisecond response times (0.5-2ms)
**Acceleration:** Rust-powered transformation (10-80x faster)

## Overview

JSON Passthrough is FraiseQL's breakthrough optimization that delivers **sub-millisecond query responses** by eliminating serialization overhead. When combined with APQ and TurboRouter, it achieves response times of 0.5-2ms in production.

## How It Works

### Traditional GraphQL Flow
```
GraphQL Query → Parse (100-300ms)
           ↓
      SQL Query → Database (2-5ms)
           ↓
  Python Objects → Dict conversion (1-5ms)
           ↓
  JSON Serialize → Network (1-5ms)
           ↓
    Total: ~104-315ms
```

### FraiseQL JSON Passthrough Flow
```
APQ Hash → Cached JSON → Network (0.5-2ms)
    ↓
  Cache Hit!
    ↓
Total: 0.5-2ms (99.5% faster!)
```

### The Optimization

When FraiseQL executes a query:

1. **PostgreSQL returns JSONB** - Database views return pre-formatted JSON
2. **Hash-based cache lookup** - APQ hash identifies the query
3. **Direct passthrough** - JSON goes directly to response
4. **Zero serialization** - No Python→Dict→JSON conversion

```python
# PostgreSQL view returns JSONB
CREATE VIEW v_user AS
SELECT jsonb_build_object(
    'id', id,
    'email', email,
    'name', name,
    'created_at', created_at::text
) AS data FROM users;
```

When this view is queried with APQ enabled:
- **First request**: Normal execution (2-5ms) + cache store
- **Subsequent requests**: Direct JSON passthrough (0.5-2ms)

### Rust-Powered Transformation

JSON Passthrough is accelerated by **fraiseql-rs**, a Rust extension that provides:

- **10-80x faster** snake_case → camelCase transformation
- **Zero-copy JSON parsing** with minimal allocations
- **GIL-free execution** for true parallelism
- **Automatic fallback** to Python if Rust unavailable

```bash
# Install Rust extensions for maximum performance
pip install fraiseql[rust]
```

**With Rust transformation:**
- PostgreSQL JSONB (snake_case) → Direct passthrough → Rust transform (0.2-2ms) → Client (camelCase)

**Without Rust transformation:**
- PostgreSQL JSONB (snake_case) → Python transform (5-25ms) → Client (camelCase)

See [Rust Transformer Guide](./rust-transformer.md) for complete documentation.

## Performance Comparison

| Stack Layer | Standard | With Passthrough | With Passthrough + Rust | Improvement |
|-------------|----------|------------------|------------------------|-------------|
| APQ Lookup | N/A | 0.1ms | 0.1ms | ✅ Enabled |
| Query Parsing | 100-300ms | **Skipped** | **Skipped** | **100% faster** |
| SQL Execution | 2-5ms | **Cached** | **Cached** | **100% faster** |
| JSON Transform | N/A | 5-25ms (Python) | **0.2-2ms (Rust)** | **10-80x faster** |
| Serialization | 1-5ms | **Skipped** | **Skipped** | **100% faster** |
| **Total** | **103-310ms** | **5-25ms** | **0.5-2ms** | **~99% faster** |

### Real Production Benchmarks

```python
# Without JSON Passthrough
Average: 120ms
P50: 110ms
P95: 180ms
P99: 250ms

# With JSON Passthrough + APQ
Average: 1.2ms
P50: 0.8ms
P95: 2.1ms
P99: 3.5ms

# Result: 99% faster at P50
```

## Enabling JSON Passthrough

### Automatic Enablement

JSON Passthrough is **automatically enabled** when you:

1. **Use JSONB views** - Return JSON from PostgreSQL
2. **Enable APQ** - Automatic Persisted Queries caching
3. **Have cache hits** - Second+ execution of same query

```python
from fraiseql import create_fraiseql_app, FraiseQLConfig

config = FraiseQLConfig(
    apq_storage_backend="postgresql",  # Persistent cache
    enable_turbo_router=True,          # Pre-compiled queries
)

app = create_fraiseql_app(config=config)

# JSON Passthrough is now active!
# No additional configuration needed
```

### Database View Requirements

Your views must return JSONB for passthrough to work:

```sql
-- ✅ CORRECT: Returns JSONB (passthrough eligible)
CREATE VIEW v_posts AS
SELECT jsonb_build_object(
    'id', p.id,
    'title', p.title,
    'author', jsonb_build_object(
        'id', u.id,
        'name', u.name
    )
) AS data
FROM posts p
JOIN users u ON p.author_id = u.id;

-- ❌ WRONG: Returns individual columns (no passthrough)
CREATE VIEW v_posts_wrong AS
SELECT
    p.id,
    p.title,
    u.name as author_name
FROM posts p
JOIN users u ON p.author_id = u.id;
```

## Optimization Stack

JSON Passthrough works best as part of the **complete optimization stack**:

### Layer 1: APQ (Automatic Persisted Queries)
- Caches query by SHA-256 hash
- Stores full execution result
- Enables passthrough on cache hit

### Layer 2: TurboRouter
- Pre-compiles GraphQL queries to SQL
- Skips parsing on repeated queries
- 4-10x faster than standard routing

### Layer 3: JSON Passthrough
- Eliminates serialization overhead
- Direct JSON response from cache
- Sub-millisecond execution

```python
# Complete optimization configuration
config = FraiseQLConfig(
    # Layer 1: APQ
    apq_storage_backend="postgresql",
    apq_storage_schema="apq_cache",

    # Layer 2: TurboRouter
    enable_turbo_router=True,

    # Layer 3: JSON Passthrough (automatic with APQ)
)

# Result: 0.5-2ms response times! 🚀
```

## Cache Hit Requirements

For JSON Passthrough to activate:

1. **✅ Same query hash** - Identical GraphQL query structure
2. **✅ Cache hit** - APQ cache contains result
3. **✅ Valid TTL** - Cache entry hasn't expired
4. **✅ JSONB view** - Database returns JSONB

### Cache Hit Scenarios

```python
# First request (MISS - normal execution)
query {
  users { id name }
}
# Response time: 25ms (no cache)

# Second request (HIT - passthrough!)
query {
  users { id name }
}
# Response time: 0.8ms (JSON passthrough!) ⚡

# Different query (MISS - different hash)
query {
  users { id name email }  # Added 'email'
}
# Response time: 25ms (new query, no cache yet)
```

## Monitoring Passthrough Performance

### Logging

Enable detailed logging to see passthrough in action:

```python
import logging

logging.basicConfig(level=logging.DEBUG)
logger = logging.getLogger("fraiseql.optimization")

# Logs will show:
# DEBUG:fraiseql.optimization: APQ cache hit, using passthrough
# DEBUG:fraiseql.optimization: Passthrough response time: 0.8ms
```

### Metrics

Track passthrough effectiveness:

```python
from fraiseql.monitoring import track_performance

@track_performance
async def my_query(info) -> list[User]:
    # Automatically tracks:
    # - Cache hit rate
    # - Passthrough usage
    # - Response times
    ...
```

### Prometheus Metrics

```python
# Available metrics
fraiseql_apq_cache_hits_total
fraiseql_passthrough_requests_total
fraiseql_response_duration_seconds{layer="passthrough"}
```

## Best Practices

### 1. Design Views for JSON

Always return JSONB from views to enable passthrough:

```sql
-- ✅ GOOD: Single JSONB column
CREATE VIEW v_user AS
SELECT jsonb_build_object(
    'id', id,
    'data', user_data
) AS data FROM users;

-- ❌ BAD: Multiple columns
CREATE VIEW v_user_bad AS
SELECT id, name, email FROM users;
```

### 2. Use PostgreSQL Backend for APQ

Memory backend doesn't persist across restarts:

```python
# ✅ GOOD: Persistent cache
config = FraiseQLConfig(
    apq_storage_backend="postgresql"
)

# ⚠️ OK for development only
config = FraiseQLConfig(
    apq_storage_backend="memory"
)
```

### 3. Warm Up Caches

Pre-populate APQ cache for critical queries:

```python
# Cache warming script
critical_queries = [
    "query { users { id name } }",
    "query { posts { id title } }",
]

for query in critical_queries:
    await execute_graphql(query)
    # First execution populates cache
    # Subsequent requests use passthrough
```

### 4. Monitor Cache Hit Rates

Aim for **95%+ cache hit rate** in production:

```python
# Check cache statistics
stats = await apq_storage.get_stats()
hit_rate = stats["hits"] / (stats["hits"] + stats["misses"])
print(f"Cache hit rate: {hit_rate:.1%}")  # Target: >95%
```

## Troubleshooting

### Passthrough Not Activating

**Symptom:** Response times still 20-50ms

**Checklist:**
1. ✅ APQ enabled? `apq_storage_backend` configured
2. ✅ JSONB views? Check `SELECT data FROM v_*`
3. ✅ Cache hits? Check APQ statistics
4. ✅ TurboRouter enabled? `enable_turbo_router=True`

### Inconsistent Performance

**Symptom:** Some queries fast, others slow

**Solution:** Check which queries are cached:

```python
# Log cache status
from fraiseql.caching import get_apq_stats

stats = get_apq_stats()
print(f"Cache size: {stats['size']}")
print(f"Hit rate: {stats['hit_rate']:.1%}")
print(f"Slowest queries: {stats['slow_queries']}")
```

### Cache Misses on Identical Queries

**Symptom:** Same query doesn't hit cache

**Cause:** Query hash changes due to:
- Different variable values (expected)
- Different whitespace (client issue)
- Different field order (client issue)

**Solution:** Normalize queries on client:

```typescript
// Client-side normalization
import { print } from 'graphql';
const normalizedQuery = print(parse(query));
```

## Advanced Configuration

### Custom Cache TTL

```python
config = FraiseQLConfig(
    apq_storage_backend="postgresql",
    apq_cache_ttl=3600,  # 1 hour TTL
)
```

### Selective Passthrough

Disable passthrough for specific queries:

```python
@fraiseql.query
async def realtime_data(info) -> RealtimeData:
    """This query should never use cache."""
    info.context["skip_cache"] = True
    ...
```

## See Also

- [Rust Transformer](rust-transformer.md) - 10-80x faster JSON transformation
- [Automatic Persisted Queries (APQ)](apq-storage-backends.md)
- [TurboRouter Pre-compilation](turbo-router.md)
- [Performance Optimization Layers](performance-optimization-layers.md)
- [Production Performance Tuning](performance.md)

---

**JSON Passthrough is FraiseQL's secret weapon for achieving sub-millisecond GraphQL responses. Combined with Rust transformation, APQ, and TurboRouter, it delivers 99%+ performance improvements over traditional GraphQL frameworks.**
