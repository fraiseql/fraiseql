# Caching Strategy in Rust-First FraiseQL Architecture

**Date**: 2025-10-16
**Context**: How APQ and other caching fits into simplified Rust-first architecture

---

## 🎯 Core Insight: Separation of Concerns

In Rust-first architecture, caching operates at **three independent layers**:

```
┌─────────────────────────────────────────────────────────────┐
│ Layer 1: APQ Cache (Query String Cache)                     │
│ Purpose: Reduce network payload                             │
│ Caches: GraphQL query strings                               │
│ Benefit: 50-90% reduction in request size                   │
└─────────────────┬───────────────────────────────────────────┘
                  │
                  ▼
┌─────────────────────────────────────────────────────────────┐
│ Layer 2: SQL Execution (No Change)                          │
│ PostgreSQL query execution                                  │
└─────────────────┬───────────────────────────────────────────┘
                  │
                  ▼
┌─────────────────────────────────────────────────────────────┐
│ Layer 3: Rust Transformation (Always Fast)                  │
│ Purpose: Transform data                                     │
│ Speed: 0.5-1ms (so fast, caching often not needed)         │
└─────────────────┬───────────────────────────────────────────┘
                  │
                  ▼
┌─────────────────────────────────────────────────────────────┐
│ Layer 4: Result Cache (Optional)                            │
│ Purpose: Cache final GraphQL response                       │
│ Caches: Complete JSON responses                            │
│ Benefit: Skip DB query + transformation entirely           │
└─────────────────────────────────────────────────────────────┘
```

**Key Point**: APQ and Rust transformation are **orthogonal** - they solve different problems and both remain valuable.

---

## 📊 Detailed Analysis by Layer

### Layer 1: APQ Cache (KEEP - Still Valuable)

**What APQ Does**:
```
Without APQ:
Client → Server: 2KB GraphQL query string
Server → Client: 50KB response

With APQ:
Client → Server: 32 bytes (SHA256 hash)
Server → Client: 50KB response

Network savings: 2KB → 32 bytes (98% reduction on request)
```

**How It Works**:
```python
# First request (query not cached)
POST /graphql
{
  "query": "query GetUser($id: Int!) { user(id: $id) { id firstName ... } }",
  "extensions": {
    "persistedQuery": {
      "version": 1,
      "sha256Hash": "abc123..."
    }
  }
}

# Server stores: hash → query string in Redis/memory

# Subsequent requests
POST /graphql
{
  "extensions": {
    "persistedQuery": {
      "version": 1,
      "sha256Hash": "abc123..."
    }
  },
  "variables": {"id": 1}
}

# Server: lookup query by hash, execute, return result
```

**Where It Happens** (before Rust):
```
1. HTTP Request arrives
2. APQ: Check if hash exists → lookup query string
3. GraphQL: Parse query (or use cached AST)
4. Resolve: Execute resolvers
5. SQL: Query database
6. Rust: Transform results ← APQ DOESN'T TOUCH THIS
7. GraphQL: Serialize response
8. HTTP: Send response
```

**Impact in Rust-First Architecture**:

✅ **KEEP APQ** - It's valuable and orthogonal to Rust transformation:

| Metric | Without APQ | With APQ | Benefit |
|--------|-------------|----------|---------|
| Request size | 2KB | 32 bytes | 98% smaller |
| Parse time | ~0.5-2ms | ~0.05ms | 10-40x faster parsing |
| Network time | Variable | Lower | Faster on slow networks |
| CDN efficiency | Lower | Higher | Can cache by hash |

**Configuration in Rust-First** (simplified):

```python
@dataclass
class FraiseQLConfig:
    database_url: str

    # APQ settings (keep but simplify)
    apq_enabled: bool = True
    apq_storage: str = "memory"  # or "redis"
    apq_ttl: int = 3600  # 1 hour

    # Remove complex options:
    # - apq_cache_responses (handled by Layer 4 instead)
    # - apq_response_cache_ttl (handled by Layer 4)
    # - apq_require_hash (edge case, remove)
```

**Verdict**: ✅ **Keep APQ** - Reduces network payload and parsing time. Independent of Rust transformation.

---

### Layer 2: SQL Execution (No Caching at This Layer)

**What Happens**:
```python
async def find(self, view_name, **kwargs):
    # Build SQL query
    query = "SELECT data FROM users WHERE id = $1"

    # Execute (no caching here - let PostgreSQL handle it)
    results = await self.db.fetch(query, params)

    # Transform with Rust
    return fraiseql_rs.transform_many(results, view_name, graphql_info)
```

**Why No SQL-Level Caching**:
- PostgreSQL already has query plan caching
- PostgreSQL already has buffer pool (hot data in memory)
- SQL execution is fast (~0.05-0.5ms for indexed queries)
- Caching SQL results adds complexity without much benefit
- If data changes, cache invalidation is complex

**Verdict**: ❌ **No SQL caching** - Let PostgreSQL handle it

---

### Layer 3: Rust Transformation (No Caching Needed)

**Why Rust Transformation Doesn't Need Caching**:

```
Rust transformation: 0.5-1ms per query

Overhead of caching:
- Cache lookup: 0.1-0.3ms (Redis) or 0.01-0.05ms (memory)
- Serialization: 0.2-0.5ms
- Deserialization: 0.2-0.5ms
- Cache write: 0.1-0.3ms

Total cache overhead: 0.6-1.6ms

Result: Caching is SLOWER than just transforming!
```

**Example**:

```python
# Without caching (Rust-first)
async def user(info, id: int):
    result = await db.fetch("SELECT data FROM users WHERE id = $1", id)
    transformed = fraiseql_rs.transform_one(result, "User", graphql_info)
    return transformed
# Total: 0.05ms (DB) + 0.5ms (Rust) = 0.55ms

# With transformation caching (anti-pattern!)
async def user(info, id: int):
    cache_key = f"user:{id}:{hash(graphql_info.field_nodes)}"
    cached = await redis.get(cache_key)  # 0.2ms
    if cached:
        return json.loads(cached)  # 0.3ms (deserialization)

    result = await db.fetch("SELECT data FROM users WHERE id = $1", id)
    transformed = fraiseql_rs.transform_one(result, "User", graphql_info)

    await redis.set(cache_key, json.dumps(transformed), ttl=60)  # 0.3ms
    return transformed
# Cache hit: 0.5ms (slower than just transforming!)
# Cache miss: 1.15ms (2x slower!)
```

**Verdict**: ❌ **No Rust transformation caching** - It's already so fast that caching adds overhead

**Exception**: If Rust transformation was slow (>10ms), caching might help. But if Rust is that slow, fix Rust instead of adding caching!

---

### Layer 4: Result Cache (Optional - For Complete Responses)

**What Result Caching Does**:

Caches the **complete GraphQL response** to skip:
1. SQL query execution
2. Rust transformation
3. GraphQL serialization

**When It's Valuable**:

```
Scenario 1: Expensive Query
- Complex JOIN: 50ms
- Rust transform: 1ms
- Total: 51ms

With result caching:
- Cache lookup: 0.2ms
- Benefit: 250x faster

Verdict: ✅ Cache worth it
```

```
Scenario 2: Simple Query (Most Queries)
- Simple lookup: 0.5ms
- Rust transform: 0.5ms
- Total: 1ms

With result caching:
- Cache lookup: 0.2ms
- Benefit: 5x faster
- But: Cache invalidation complexity

Verdict: ⚠️ Marginal benefit, adds complexity
```

**Implementation in Rust-First**:

```python
# Optional result caching decorator
from fraiseql.caching import cache_result

@fraiseql.query
@cache_result(ttl=60, key_fn=lambda id: f"user:{id}")
async def user(info, id: int) -> User:
    """
    With @cache_result:
    1. Check cache by key: f"user:{id}"
    2. If hit: return cached response
    3. If miss: execute query, cache result, return
    """
    repo = Repository(info.context["db"], info.context)
    return await repo.find_one("users", id=id)

# Simple queries: Don't use caching (not worth complexity)
@fraiseql.query
async def users(info, limit: int = 10) -> list[User]:
    """No caching - already fast (1-2ms)"""
    repo = Repository(info.context["db"], info.context)
    return await repo.find("users", limit=limit)

# Complex queries: Use caching
@fraiseql.query
@cache_result(ttl=300, key_fn=lambda: "dashboard:stats")
async def dashboard_stats(info) -> DashboardStats:
    """
    Complex aggregation query (50ms)
    Cache for 5 minutes
    """
    # Complex query with multiple JOINs
    ...
```

**Simplified Result Cache Configuration**:

```python
@dataclass
class FraiseQLConfig:
    database_url: str

    # Result caching (optional, off by default)
    result_cache_enabled: bool = False
    result_cache_backend: str = "redis"  # or "memory"
    result_cache_default_ttl: int = 60

    # Remove from config (handled by decorator):
    # - Per-query TTL (use decorator)
    # - Cache key strategy (use key_fn)
    # - Cache invalidation rules (use decorator)
```

**Verdict**: ⚠️ **Optional result caching** - Use selectively for expensive queries, not by default

---

## 📋 Caching Strategy Summary

### What to Cache (and What Not to Cache)

| Layer | Cache? | Why | Impact |
|-------|--------|-----|--------|
| **APQ (Query String)** | ✅ Yes | Reduces network payload 98% | 0.5-2ms savings per request |
| **SQL Execution** | ❌ No | PostgreSQL already optimizes | Adds complexity for no benefit |
| **Rust Transformation** | ❌ No | Too fast (0.5ms), caching is slower | Negative impact |
| **Result (Complete Response)** | ⚠️ Selective | Only for expensive queries (>10ms) | 10-100x for complex queries |

### Recommended Configuration

**Minimal (Most Use Cases)**:
```python
config = FraiseQLConfig.preset_production(
    database_url="postgresql://...",
    apq_enabled=True,  # ✅ Keep APQ
    result_cache_enabled=False,  # ❌ No result caching by default
)
```

**High-Traffic (If Needed)**:
```python
config = FraiseQLConfig.preset_production(
    database_url="postgresql://...",
    apq_enabled=True,
    apq_storage="redis",  # Shared APQ cache
    result_cache_enabled=True,  # Enable for expensive queries
    result_cache_backend="redis",
)

# Then use @cache_result selectively
@cache_result(ttl=300)
async def expensive_dashboard_query(info):
    ...
```

---

## 🎯 Architectural Decision: Caching in Rust-First

### What Changes in Rust-First Architecture

**Removed** (from current FraiseQL):
```python
# OLD: Complex APQ response caching
class FraiseQLConfig:
    apq_cache_responses: bool = True  # ❌ Remove
    apq_response_cache_ttl: int = 3600  # ❌ Remove
    apq_response_cache_key_prefix: str = "..."  # ❌ Remove

    # These conflated APQ (query string cache) with
    # result caching (response cache)
```

**Why Remove**: APQ should only cache query strings, not responses. Result caching is a separate concern.

**Kept** (simplified):
```python
# NEW: Clear separation
class FraiseQLConfig:
    # APQ: Query string caching only
    apq_enabled: bool = True
    apq_storage: str = "memory"  # or "redis"
    apq_ttl: int = 3600

    # Result caching: Separate, optional
    result_cache_enabled: bool = False
    result_cache_backend: str = "redis"
    result_cache_default_ttl: int = 60
```

### Migration Path

**Before** (confused APQ with result caching):
```python
config = FraiseQLConfig(
    apq_storage_backend="memory",
    apq_cache_responses=True,  # Actually caching results, not queries!
    apq_response_cache_ttl=3600,
)
```

**After** (clear separation):
```python
config = FraiseQLConfig.preset_production(
    database_url="...",
    apq_enabled=True,  # Just query string caching
)

# If you want result caching, use decorator:
@cache_result(ttl=3600)
async def my_expensive_query(info):
    ...
```

---

## 💡 When to Use Each Caching Layer

### APQ (Always Recommended)

**Use When**:
- ✅ Always (default on)
- ✅ Public APIs with many clients
- ✅ Mobile apps (reduce network payload)
- ✅ Large queries (dashboard, reports)

**Don't Use When**:
- Internal APIs with trusted clients (optional)
- Very simple queries only (marginal benefit)

**Configuration**:
```python
# Production: Redis for shared cache across servers
config = FraiseQLConfig.preset_production(
    database_url="...",
    apq_enabled=True,
    apq_storage="redis",
)

# Development: Memory cache (simple)
config = FraiseQLConfig.preset_development(
    database_url="...",
    apq_enabled=True,
    apq_storage="memory",
)
```

### Result Caching (Selective)

**Use When**:
- ✅ Complex aggregations (>10ms query time)
- ✅ Dashboard queries (acceptable staleness)
- ✅ Reports (expensive, infrequent updates)
- ✅ Public data (same for all users)

**Don't Use When**:
- ❌ Simple lookups (Rust already fast)
- ❌ User-specific data (cache per user = low hit rate)
- ❌ Frequently updated data (invalidation complexity)
- ❌ Real-time requirements (staleness unacceptable)

**Configuration**:
```python
# Enable globally, use selectively via decorator
config = FraiseQLConfig.preset_production(
    database_url="...",
    result_cache_enabled=True,
    result_cache_backend="redis",
)

# Expensive query: Cache it
@cache_result(ttl=300)  # 5 minutes
async def dashboard_stats(info):
    # Complex query: 50ms
    return await expensive_aggregation()

# Simple query: Don't cache
async def user(info, id: int):
    # Simple query: 1ms (Rust makes it fast!)
    return await repo.find_one("users", id=id)
```

---

## 📈 Performance Impact Analysis

### Scenario 1: Simple User Lookup

**Query**: `{ user(id: 1) { id firstName email } }`

| Configuration | Total Time | Breakdown |
|--------------|------------|-----------|
| **No caching** | 1.2ms | DB: 0.5ms, Rust: 0.5ms, Framework: 0.2ms |
| **APQ only** | 0.9ms | APQ: -0.3ms (parse savings), DB: 0.5ms, Rust: 0.5ms |
| **APQ + Result cache (miss)** | 1.5ms | APQ: -0.3ms, DB: 0.5ms, Rust: 0.5ms, Cache write: 0.3ms |
| **APQ + Result cache (hit)** | 0.4ms | APQ: -0.3ms, Cache read: 0.2ms |

**Recommendation**: APQ only (no result cache for simple queries)

### Scenario 2: Complex Dashboard

**Query**: Dashboard with 5 aggregations, 3 JOINs

| Configuration | Total Time | Breakdown |
|--------------|------------|-----------|
| **No caching** | 65ms | DB: 60ms, Rust: 1ms, Framework: 4ms |
| **APQ only** | 62ms | APQ: -3ms, DB: 60ms, Rust: 1ms |
| **APQ + Result cache (miss)** | 62.5ms | Same + Cache write: 0.5ms |
| **APQ + Result cache (hit)** | 0.3ms | Cache read: 0.3ms |

**Recommendation**: APQ + Result cache (200x speedup on cache hit)

### Scenario 3: List Query (10 users)

**Query**: `{ users(limit: 10) { id firstName email } }`

| Configuration | Total Time | Breakdown |
|--------------|------------|-----------|
| **No caching** | 2.5ms | DB: 1ms, Rust: 1ms (10 transforms), Framework: 0.5ms |
| **APQ only** | 2.2ms | APQ: -0.3ms, DB: 1ms, Rust: 1ms |
| **APQ + Result cache (hit)** | 0.4ms | Cache read: 0.4ms |

**Recommendation**: APQ only for most cases, result cache if query is expensive

---

## 🏗️ Implementation Examples

### Example 1: Production API with APQ

```python
from fraiseql.fastapi import FraiseQLConfig, create_fraiseql_app

# Simple configuration: Just APQ
config = FraiseQLConfig(
    database_url="postgresql://...",
    apq_enabled=True,
    apq_storage="redis",  # Shared across servers
    apq_ttl=3600,
)

app = create_fraiseql_app(
    config=config,
    types=[User, Post],
    queries=[user, users, posts],
)

# All queries benefit from APQ automatically
# No result caching = simple, fast
```

### Example 2: API with Selective Result Caching

```python
from fraiseql.caching import cache_result

config = FraiseQLConfig(
    database_url="postgresql://...",
    apq_enabled=True,
    result_cache_enabled=True,  # Enable result caching
    result_cache_backend="redis",
)

# Simple query: No caching
@fraiseql.query
async def user(info, id: int) -> User:
    repo = Repository(info.context["db"], info.context)
    return await repo.find_one("users", id=id)
    # Fast enough (1ms) - no cache needed

# Expensive query: Cache it
@fraiseql.query
@cache_result(
    ttl=300,  # 5 minutes
    key_fn=lambda: "dashboard:main",
    invalidate_on=["user.updated", "post.created"]  # Optional
)
async def dashboard(info) -> Dashboard:
    """Complex dashboard with aggregations"""
    # This takes 50ms without cache
    # Cache hit takes 0.3ms
    return await complex_dashboard_query()

# List query: Parameterized caching
@fraiseql.query
@cache_result(
    ttl=60,
    key_fn=lambda limit, offset: f"users:list:{limit}:{offset}"
)
async def users(info, limit: int = 10, offset: int = 0) -> list[User]:
    """Cached list - useful for pagination"""
    repo = Repository(info.context["db"], info.context)
    return await repo.find("users", limit=limit, offset=offset)
```

### Example 3: High-Traffic Public API

```python
# Maximum optimization for public API
config = FraiseQLConfig(
    database_url="postgresql://...",

    # APQ with Redis (shared cache)
    apq_enabled=True,
    apq_storage="redis",
    apq_ttl=86400,  # 24 hours for public API

    # Result caching with Redis
    result_cache_enabled=True,
    result_cache_backend="redis",
    result_cache_default_ttl=300,
)

# Cache most queries (public data, acceptable staleness)
@fraiseql.query
@cache_result(ttl=60, key_fn=lambda id: f"user:public:{id}")
async def user(info, id: int) -> User:
    """Public user profile - cache for 1 minute"""
    ...

@fraiseql.query
@cache_result(ttl=300, key_fn=lambda: "users:trending")
async def trending_users(info) -> list[User]:
    """Trending users - expensive query, cache 5 minutes"""
    ...

# Still no caching for personalized queries
@fraiseql.query
async def me(info) -> User:
    """Current user - don't cache (user-specific)"""
    user_id = info.context["user"]["id"]
    ...
```

---

## 🎯 Key Takeaways

### 1. APQ is Orthogonal to Rust Transformation

**APQ** (Layer 1): Caches query strings
- Reduces network payload
- Speeds up parsing
- Works before any data processing

**Rust** (Layer 3): Transforms data
- So fast (0.5ms) that caching would be slower
- No caching needed at this layer

**They're independent and both valuable.**

### 2. Simplify APQ in Rust-First

**Remove**:
- `apq_cache_responses` (confused with result caching)
- `apq_response_cache_ttl` (separate concern)
- Complex APQ response caching logic

**Keep**:
- `apq_enabled` (query string caching)
- `apq_storage` (memory or redis)
- `apq_ttl` (query string TTL)

### 3. Result Caching is Optional

**Use selectively**:
- Expensive queries (>10ms)
- Acceptable staleness
- High hit rate

**Don't use by default**:
- Adds complexity
- Cache invalidation is hard
- Rust makes most queries fast enough

### 4. Recommended Configuration

**For 90% of use cases**:
```python
config = FraiseQLConfig.preset_production(
    database_url="...",
    apq_enabled=True,  # ✅ Always on
    result_cache_enabled=False,  # ❌ Off by default
)
```

**For high-traffic APIs**:
```python
config = FraiseQLConfig.preset_production(
    database_url="...",
    apq_enabled=True,
    apq_storage="redis",
    result_cache_enabled=True,
    result_cache_backend="redis",
)

# Use @cache_result decorator selectively
```

---

## 📊 Updated Simplified Configuration

```python
@dataclass
class FraiseQLConfig:
    """Rust-First FraiseQL Configuration"""

    # Database (required)
    database_url: str
    database_pool_size: int = 20

    # APQ: Query string caching (recommended)
    apq_enabled: bool = True
    apq_storage: str = "memory"  # or "redis"
    apq_ttl: int = 3600

    # Result caching: Optional, off by default
    result_cache_enabled: bool = False
    result_cache_backend: str = "redis"
    result_cache_default_ttl: int = 60

    # GraphQL
    enable_introspection: bool = True
    enable_playground: bool = False

    # Debug
    debug: bool = False

    # Total: 10 options (was 50+)

    @classmethod
    def preset_production(cls, database_url: str) -> "FraiseQLConfig":
        """Production: APQ on, result cache off"""
        return cls(
            database_url=database_url,
            database_pool_size=50,
            apq_enabled=True,
            apq_storage="redis",
            result_cache_enabled=False,  # Use @cache_result selectively
            enable_introspection=False,
            enable_playground=False,
        )

    @classmethod
    def preset_high_traffic(cls, database_url: str) -> "FraiseQLConfig":
        """High-traffic: APQ + result cache"""
        return cls(
            database_url=database_url,
            database_pool_size=100,
            apq_enabled=True,
            apq_storage="redis",
            result_cache_enabled=True,
            result_cache_backend="redis",
            enable_introspection=False,
            enable_playground=False,
        )
```

---

## 🚀 Summary

**APQ in Rust-First Architecture**:
- ✅ **Keep APQ** - Still valuable for reducing network payload
- ✅ **Simplify APQ** - Only cache query strings, not responses
- ✅ **Default on** - Recommended for all production deployments

**Rust Transformation**:
- ❌ **No caching at Rust layer** - It's already too fast (0.5ms)
- ✅ **Caching would be slower** than just transforming

**Result Caching**:
- ⚠️ **Optional, off by default** - Use selectively for expensive queries
- ✅ **Decorator-based** - Opt-in per query with `@cache_result`
- ✅ **Clear benefit** - 100x speedup for complex aggregations

**Final Configuration**: 10 options (down from 50+), clearer separation of concerns, same or better performance.
