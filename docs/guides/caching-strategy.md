# Caching Strategy & Implementation Guide

Complete guide to FraiseQL's intelligent caching system for optimal performance and zero Redis dependency.

## Overview

FraiseQL provides sophisticated query result caching optimized for transactional workloads. The system stores cached results in PostgreSQL UNLOGGED tables, delivering sub-millisecond response times while maintaining data consistency.

**Key Benefits:**
- **Sub-millisecond cache hits** with automatic result caching
- **Zero Redis dependency** - uses existing PostgreSQL infrastructure
- **Multi-tenant security** - automatic tenant isolation in cache keys
- **Automatic invalidation** - TTL-based or domain-based (with extension)
- **Transparent integration** - minimal code changes required

**Performance Impact:**

| Scenario | Without Cache | With Cache | Speedup |
|----------|---------------|------------|---------|
| Simple query | 50-100ms | 0.5-2ms | **50-100x** |
| Complex aggregation | 200-500ms | 0.5-2ms | **200-500x** |
| Multi-tenant query | 100-300ms | 0.5-2ms | **100-300x** |

## Cache Hit Rates by Workload Type

### High Cache Efficiency (85%+ Hit Rate)

#### Typical SaaS Applications
**Characteristics:**
- Repeated queries for user data, settings, preferences
- Common filters (status, tenant_id, user_id)
- High temporal locality (same data accessed frequently)

**Examples:**
- GetUser query (same user_id requested repeatedly)
- ListUserSettings (user-specific data)
- GetTenantConfig (tenant settings)

**Expected Performance:**
- ✅ **Cache Hit Rate: 85%+**
- ✅ **DB Load Reduction: 85%**
- ✅ **Response Time: < 5ms (cached) vs 50-100ms (DB)**

**Optimization:** No special configuration needed. Caching works automatically.

#### High-Frequency APIs
**Characteristics:**
- Frequent requests for same data
- Volatile data (caches refresh frequently)
- Heavy read load with occasional writes

**Examples:**
- GetProduct (hot products checked repeatedly)
- GetInventory (inventory checked on every request)
- GetPricingTier (pricing accessed frequently)

### Medium Cache Efficiency (50-80% Hit Rate)

#### Content Management Systems
**Characteristics:**
- Mix of static and dynamic content
- Content accessed by category/section
- User-generated content with varying popularity

**Examples:**
- GetArticlesByCategory (category pages)
- GetContentBySlug (individual pages)
- GetCommentsByPost (comment threads)

**Expected Performance:**
- ✅ **Cache Hit Rate: 60-80%**
- ✅ **DB Load Reduction: 60-80%**
- ⚠️ **Consider cache warming** for popular content

#### E-commerce Product Catalogs
**Characteristics:**
- Product data changes infrequently
- Category browsing is common
- Search results can be cached

**Examples:**
- GetProductDetails (stable product data)
- GetCategoryProducts (category browsing)
- GetProductSearch (search results)

### Low Cache Efficiency (< 50% Hit Rate)

#### Analytics & Reporting
**Characteristics:**
- Ad-hoc queries with unique parameters
- Time-based data with frequent changes
- Complex aggregations and filtering

**Examples:**
- Custom date range reports
- User behavior analytics
- Real-time dashboards

**Recommendation:** Use a data warehouse instead of caching.

#### Write-Heavy Applications
**Characteristics:**
- Data changes frequently
- Cache invalidation overhead > cache benefit
- Real-time data requirements

**Examples:**
- Live chat systems
- Real-time collaboration tools
- High-frequency trading data

**Recommendation:** Disable caching or use very short TTL.

## Quick Start - Basic Setup

### For New Projects

```python
from fraiseql import create_fraiseql_app
from fraiseql.caching import PostgresCache, ResultCache, CachedRepository
from fraiseql.db import DatabasePool

# Initialize database pool
pool = DatabasePool("postgresql://user:pass@localhost/mydb")

# Create cache backend (PostgreSQL UNLOGGED table)
postgres_cache = PostgresCache(
    connection_pool=pool,
    table_name="fraiseql_cache",  # default
    auto_initialize=True
)

# Wrap with result cache (adds statistics tracking)
result_cache = ResultCache(backend=postgres_cache, default_ttl=300)

# Wrap repository with caching
from fraiseql.db import FraiseQLRepository

base_repo = FraiseQLRepository(
    pool=pool,
    context={"tenant_id": tenant_id}  # CRITICAL for multi-tenant!
)

cached_repo = CachedRepository(
    repository=base_repo,
    cache=result_cache,
    # Cache key includes tenant_id automatically
)

# Use cached repository in your GraphQL resolvers
@fraiseql.query
async def get_user(info, user_id: UUID) -> User:
    return await cached_repo.get_user(user_id)
```

### For Existing Projects

Add cache initialization to your application startup:

```python
from fastapi import FastAPI
from fraiseql.caching import PostgresCache, ResultCache

app = FastAPI()

@app.on_event("startup")
async def startup():
    # Reuse existing database pool
    pool = app.state.db_pool

    # Initialize cache backend (auto-creates UNLOGGED table)
    postgres_cache = PostgresCache(
        connection_pool=pool,
        auto_initialize=True
    )

    # Wrap with result cache
    result_cache = ResultCache(
        backend=postgres_cache,
        default_ttl=300,  # 5 minutes default
        max_size_mb=100   # Limit cache size
    )

    # Store in app state for use in resolvers
    app.state.cache = result_cache

# In your resolvers
@fraiseql.query
async def get_products(info, category: str) -> list[Product]:
    cache = info.context["request"].app.state.cache

    # Check cache first
    cache_key = f"products:{category}"
    cached_result = await cache.get(cache_key)

    if cached_result is not None:
        return cached_result

    # Cache miss - query database
    products = await db.get_products_by_category(category)

    # Cache the result
    await cache.set(cache_key, products, ttl=600)  # 10 minutes

    return products
```

## Cache Architecture

### Storage Backend

FraiseQL uses PostgreSQL UNLOGGED tables for cache storage:

```sql
CREATE UNLOGGED TABLE fraiseql_cache (
    cache_key TEXT PRIMARY KEY,
    value BYTEA NOT NULL,
    expires_at TIMESTAMP WITH TIME ZONE,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    metadata JSONB
);

-- Performance indexes
CREATE INDEX idx_fraiseql_cache_expires_at ON fraiseql_cache(expires_at);
CREATE INDEX idx_fraiseql_cache_metadata ON fraiseql_cache USING GIN(metadata);
```

**Benefits:**
- **ACID compliance** - Cache operations are transactional
- **Multi-tenant isolation** - Tenant ID included in cache keys
- **No additional infrastructure** - Uses existing PostgreSQL
- **Automatic cleanup** - Expired entries removed by background process

### Cache Key Generation

Cache keys automatically include:
- Query structure (GraphQL operation)
- Variable values
- User context (tenant_id, user_id)
- Request-specific parameters

```python
# Automatic key generation
query = """
    query GetUser($userId: UUID!) {
        user(id: $userId) {
            id
            name
            email
        }
    }
"""

variables = {"userId": "550e8400-e29b-41d4-a716-446655440000"}
context = {"tenant_id": "tenant123", "user_id": "user456"}

# Generated key: "graphql:GetUser:tenant123:user456:550e8400-e29b-41d4-a716-446655440000"
```

## Advanced Configuration

### TTL Strategies

```python
from fraiseql.caching import ResultCache

# Different TTL for different query types
cache = ResultCache(
    backend=postgres_cache,
    default_ttl=300,  # 5 minutes default
    ttl_overrides={
        "GetUser": 3600,      # 1 hour for user data
        "GetProducts": 600,   # 10 minutes for products
        "GetAnalytics": 60,   # 1 minute for analytics
    }
)
```

### Size Limits

```python
# Limit cache size to prevent unbounded growth
cache = ResultCache(
    backend=postgres_cache,
    max_size_mb=500,  # 500MB limit
    eviction_policy="lru"  # Least Recently Used
)
```

### Cache Warming

For critical data that must be fast:

```python
# Warm cache on startup
@app.on_event("startup")
async def warm_cache():
    cache = app.state.cache

    # Pre-populate frequently accessed data
    popular_products = await db.get_popular_products()
    await cache.set("popular_products", popular_products, ttl=3600)

    # Warm user-specific caches
    for user_id in active_users:
        user_data = await db.get_user(user_id)
        await cache.set(f"user:{user_id}", user_data, ttl=1800)
```

## Cache Invalidation

### Automatic Invalidation (GraphQL Cascade)

FraiseQL's GraphQL Cascade system provides automatic cache invalidation:

```python
# When data changes, related caches are automatically invalidated
@fraiseql.mutation
async def update_product(info, product_id: UUID, data: ProductUpdate) -> Product:
    # This mutation automatically invalidates:
    # - getProduct(id: $product_id)
    # - getProducts(category: $category) if category changed
    # - getProductsByPriceRange if price changed
    return await db.update_product(product_id, data)
```

### Manual Invalidation

For custom invalidation logic:

```python
from fraiseql.caching import CacheInvalidator

invalidator = CacheInvalidator(cache=result_cache)

# Invalidate specific keys
await invalidator.invalidate("user:123")

# Invalidate by pattern
await invalidator.invalidate_pattern("products:*")

# Invalidate by tags (requires pg_fraiseql_cache extension)
await invalidator.invalidate_tags(["user", "profile"])
```

### Domain-Based Invalidation

With the `pg_fraiseql_cache` extension:

```sql
-- Create domain-based invalidation rules
SELECT pg_fraiseql_cache.create_invalidation_rule(
    'user_updated',
    'user:*',  -- Cache key pattern
    'users'    -- Database table to monitor
);

-- Extension automatically invalidates cache when table changes
UPDATE users SET name = 'New Name' WHERE id = 123;
-- Automatically invalidates cache keys matching 'user:*'
```

## Monitoring & Observability

### Cache Statistics

```python
# Get cache performance metrics
stats = await cache.get_stats()

print(f"Cache hit rate: {stats.hit_rate:.1%}")
print(f"Total requests: {stats.total_requests}")
print(f"Cache hits: {stats.hits}")
print(f"Cache misses: {stats.misses}")
print(f"Cache size: {stats.size_mb:.1f} MB")
print(f"Eviction count: {stats.evictions}")
```

### Prometheus Metrics

```python
from fraiseql.caching.metrics import CacheMetrics

# Export metrics to Prometheus
metrics = CacheMetrics(cache=result_cache)
metrics.register_prometheus_metrics()

# Available metrics:
# - fraiseql_cache_hit_rate
# - fraiseql_cache_requests_total
# - fraiseql_cache_size_bytes
# - fraiseql_cache_evictions_total
```

### Health Checks

```python
# Add to your health check endpoint
@app.get("/health/cache")
async def cache_health():
    try:
        # Test cache connectivity
        test_key = f"health_check:{uuid4()}"
        await cache.set(test_key, "ok", ttl=10)
        result = await cache.get(test_key)

        return {
            "status": "healthy" if result == "ok" else "unhealthy",
            "stats": await cache.get_stats()
        }
    except Exception as e:
        return {"status": "unhealthy", "error": str(e)}
```

## Multi-Tenant Considerations

### Automatic Tenant Isolation

```python
# Cache keys automatically include tenant_id
@fraiseql.query
async def get_tenant_users(info) -> list[User]:
    # Cache key: "graphql:GetTenantUsers:tenant123"
    # Only returns cached results for same tenant
    return await db.get_users_for_tenant(info.context.tenant_id)
```

### Tenant-Specific Configuration

```python
# Different cache settings per tenant
tenant_configs = {
    "premium": {"ttl": 3600, "max_size_mb": 1000},
    "standard": {"ttl": 1800, "max_size_mb": 500},
    "basic": {"ttl": 600, "max_size_mb": 100}
}

def get_cache_for_tenant(tenant_id: str) -> ResultCache:
    config = tenant_configs.get(tenant_id, tenant_configs["basic"])
    return ResultCache(
        backend=postgres_cache,
        default_ttl=config["ttl"],
        max_size_mb=config["max_size_mb"]
    )
```

## Performance Tuning

### Memory Management

```python
# Optimize for memory usage
cache = ResultCache(
    backend=postgres_cache,
    max_size_mb=200,           # Limit memory usage
    compression="lz4",         # Compress cached data
    serialization="msgpack"    # Efficient serialization
)
```

### Connection Pooling

```python
# Separate connection pool for cache operations
cache_pool = DatabasePool(
    dsn="postgresql://user:pass@localhost/cache_db",
    min_size=2,
    max_size=10
)

postgres_cache = PostgresCache(
    connection_pool=cache_pool,
    table_name="cache_entries"
)
```

### Cache Warming Strategies

```python
class CacheWarmer:
    def __init__(self, cache: ResultCache, db_pool: DatabasePool):
        self.cache = cache
        self.db = db_pool

    async def warm_popular_data(self):
        """Warm cache with frequently accessed data."""
        # Popular products
        products = await self.db.fetch("SELECT * FROM products ORDER BY views DESC LIMIT 100")
        await self.cache.set("popular_products", products, ttl=3600)

        # Recent orders
        orders = await self.db.fetch("SELECT * FROM orders WHERE created_at > NOW() - INTERVAL '1 day'")
        await self.cache.set("recent_orders", orders, ttl=300)

    async def warm_user_data(self, user_ids: list[str]):
        """Warm user-specific data."""
        for user_id in user_ids:
            user_data = await self.db.fetch("SELECT * FROM users WHERE id = $1", user_id)
            await self.cache.set(f"user:{user_id}", user_data, ttl=1800)
```

## Troubleshooting

### Common Issues

**Low Cache Hit Rate:**
- Check if queries have varying parameters
- Consider cache warming for static data
- Review TTL settings (too short = low hit rate)

**High Memory Usage:**
- Set `max_size_mb` limit
- Enable compression
- Implement LRU eviction

**Stale Data:**
- Check TTL settings
- Implement proper invalidation
- Use domain-based invalidation extension

**Slow Cache Operations:**
- Check database connection pool size
- Monitor PostgreSQL performance
- Consider separate cache database

### Debug Cache Behavior

```python
# Enable debug logging
import logging
logging.getLogger("fraiseql.caching").setLevel(logging.DEBUG)

# Inspect cache keys
keys = await cache.list_keys(pattern="user:*")
print(f"User cache keys: {keys}")

# Check key metadata
for key in keys[:5]:
    metadata = await cache.get_metadata(key)
    print(f"Key: {key}, TTL: {metadata.ttl}, Size: {metadata.size_bytes}")
```

## Migration from Other Caches

### From Redis

```python
# Before (Redis)
import redis
redis_client = redis.Redis(host="localhost", port=6379)

@fraiseql.query
async def get_user(info, user_id: UUID) -> User:
    key = f"user:{user_id}"
    cached = redis_client.get(key)
    if cached:
        return json.loads(cached)

    user = await db.get_user(user_id)
    redis_client.setex(key, 300, json.dumps(user))
    return user

# After (FraiseQL)
@fraiseql.query
async def get_user(info, user_id: UUID) -> User:
    # Automatic caching with tenant isolation
    return await cached_repo.get_user(user_id)
```

### From Application-Level Caching

```python
# Before (manual caching)
@fraiseql.query
async def get_products(info, category: str) -> list[Product]:
    # Manual cache management
    cache_key = f"products:{category}:{info.context.tenant_id}"
    if cache_key in app.state.cache:
        return app.state.cache[cache_key]

    products = await db.get_products_by_category(category)
    app.state.cache[cache_key] = products
    return products

# After (automatic caching)
@fraiseql.query
async def get_products(info, category: str) -> list[Product]:
    # Automatic caching with invalidation
    return await cached_repo.get_products_by_category(category)
```

## Best Practices

### Cache Key Design

```python
# ✅ Good: Include all relevant parameters
@fraiseql.query
async def get_products(info, category: str, limit: int = 10) -> list[Product]:
    # Cache key automatically includes category and limit
    pass

# ❌ Bad: Don't cache personalized data without user context
@fraiseql.query
async def get_recommendations(info) -> list[Product]:
    # Recommendations are user-specific - don't cache globally
    pass
```

### TTL Guidelines

- **Static data**: 1-24 hours
- **User data**: 5-30 minutes
- **Analytics**: 1-5 minutes
- **Real-time data**: 30 seconds - 5 minutes
- **Volatile data**: 10-60 seconds

### Monitoring Alerts

```python
# Set up alerts for cache health
alerts = {
    "low_hit_rate": cache.hit_rate < 0.5,  # Alert if < 50%
    "high_memory": cache.size_mb > 400,    # Alert if > 400MB
    "high_evictions": cache.evictions > 1000,  # Alert if excessive evictions
}
```

## Integration Examples

### FastAPI Integration

```python
from fastapi import FastAPI, Request
from fraiseql.caching import PostgresCache, ResultCache

app = FastAPI()

@app.on_event("startup")
async def startup():
    # Initialize cache
    pool = app.state.db_pool
    postgres_cache = PostgresCache(connection_pool=pool)
    app.state.cache = ResultCache(backend=postgres_cache)

@app.middleware("http")
async def add_cache_to_request(request: Request, call_next):
    # Make cache available in request context
    request.state.cache = app.state.cache
    return await call_next(request)
```

### GraphQL Context

```python
# Add cache to GraphQL context
def get_context(request: Request) -> dict:
    return {
        "request": request,
        "cache": request.state.cache,
        "tenant_id": get_tenant_from_request(request)
    }

# Use in resolvers
@fraiseql.query
async def get_data(info) -> Data:
    cache = info.context["cache"]
    tenant_id = info.context["tenant_id"]

    key = f"data:{tenant_id}"
    cached = await cache.get(key)

    if cached is not None:
        return cached

    data = await fetch_data(tenant_id)
    await cache.set(key, data, ttl=300)
    return data
```

## Next Steps

- [Performance Guide](../guides/performance-guide.md) - Advanced optimization
- [Monitoring](../production/monitoring.md) - Production monitoring
- [Troubleshooting](../guides/troubleshooting.md) - Common cache issues

---

**Remember**: FraiseQL caching works great for typical applications. For analytics workloads, consider a dedicated data warehouse instead.
