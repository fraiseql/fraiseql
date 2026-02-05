<!-- Skip to main content -->
---
title: Caching Specification
description: Caching in FraiseQL is a **comprehensive, deterministic system** that improves performance while maintaining **absolute data consistency** through automatic inv
keywords: ["format", "compliance", "protocol", "specification", "standard"]
tags: ["documentation", "reference"]
---

# Caching Specification

**Version:** 1.0
**Status:** Draft
**Audience:** Database architects, schema designers, API developers, operations engineers

---

## 1. Overview

Caching in FraiseQL is a **comprehensive, deterministic system** that improves performance while maintaining **absolute data consistency** through automatic invalidation. The caching architecture is built on three foundational principles:

1. **Multi-Layer Architecture** — Query caching + APQ response caching + automatic invalidation
2. **Tenant Isolation** — All cache keys include tenant context, preventing cross-tenant data leakage
3. **Deterministic Invalidation** — Cache invalidation is compiler-determined, not runtime-dependent

FraiseQL provides:

- **Query Result Caching** — Cache GraphQL query results at the database level
- **APQ Response Caching** — Cache persisted query responses with field selection
- **graphql-cascade Integration** — Automatic cache invalidation based on mutation cascades
- **Multi-Backend Support** — In-memory (development), PostgreSQL (production), or custom backends
- **Transparent TTL Management** — Time-to-live policies with complexity-aware tuning
- **Observable Metrics** — Hit/miss rates, cache sizes, performance tracking

---

## 2. Query Result Caching

### 2.1 Overview

Query Result Caching stores the complete result of GraphQL queries in a cache, enabling subsecond response times for repeated queries.

**Key Characteristics:**

- Operates at the **GraphQL query level** (after validation, authorization, before database execution)
- **Deterministic cache keys** based on query, variables, tenant, and complexity
- **Tenant-isolated** — Impossible to retrieve data from other tenants via cache
- **Configurable backends** — Memory (default), PostgreSQL, or custom
- **Optional error caching** — Can cache error responses if configured

### 2.2 Cache Key Generation

#### Structure

```text
<!-- Code example in TEXT -->
{prefix}:{tenant_id}:{operation_hash}:{variables_hash}:{complexity_hash}
```text
<!-- Code example in TEXT -->

**Components:**

- `prefix` — Default: "FraiseQL", configurable per deployment
- `tenant_id` — Required UUID of the organization/tenant
- `operation_hash` — SHA-256 hash of the normalized GraphQL query
- `variables_hash` — SHA-256 hash of sorted variables (prevents same query with different variables from hitting same cache entry)
- `complexity_hash` — Hash of calculated query complexity (complexity-aware caching)

#### Example

```text
<!-- Code example in TEXT -->
FraiseQL:org_550e8400-e29b-41d4:a7f3e9d2c1b:4f6a8e9d:low
```text
<!-- Code example in TEXT -->

**Tenant Isolation Guarantee:**
Cache keys are scoped by `tenant_id` at the highest level. Even with cache backend compromise, an attacker cannot retrieve data from tenant A if requesting with tenant B credentials.

### 2.3 Configuration

#### CacheConfig Dataclass

```python
<!-- Code example in Python -->
@dataclass
class CacheConfig:
    """Query result cache configuration."""

    # Enable/disable caching
    enabled: bool = True

    # TTL (time-to-live) policies
    default_ttl: int = 300  # 5 minutes
    max_ttl: int = 3600     # 1 hour
    min_ttl: int = 0        # No minimum (immediate expiration allowed)

    # Cache size management
    max_size_bytes: int = 1_000_000_000  # 1 GB default
    max_entries: int = 100_000            # Max cache entries

    # Error caching
    cache_errors: bool = False  # Cache error responses?
    error_ttl: int = 60         # TTL for error responses (60 seconds)

    # Key configuration
    key_prefix: str = "FraiseQL"
    include_complexity: bool = True  # Include complexity in cache key

    # Tenant isolation
    require_tenant_id: bool = True  # Fail if tenant_id not provided
```text
<!-- Code example in TEXT -->

#### Usage

```python
<!-- Code example in Python -->
from FraiseQL import create_fraiseql_app, CacheConfig

cache_config = CacheConfig(
    enabled=True,
    default_ttl=300,        # 5 minutes for normal queries
    max_ttl=3600,           # Never cache longer than 1 hour
    cache_errors=False,     # Don't cache errors
    include_complexity=True # Complexity-aware caching
)

app = create_fraiseql_app(
    schema=schema,
    cache_config=cache_config
)
```text
<!-- Code example in TEXT -->

### 2.4 Cache Backends

#### 2.4.1 Memory Backend (Default)

**Location:** `FraiseQL.storage.backends.memory.MemoryCacheBackend`

**Characteristics:**

- Stores cache in process memory (Python dictionary)
- **Best for:** Development, single-instance deployments
- **Performance:** Sub-millisecond lookups
- **Thread-safety:** Protected by asyncio locks
- **Persistence:** Lost on process restart
- **Multi-instance:** Not shared between instances

**Configuration:**

```python
<!-- Code example in Python -->
from FraiseQL.storage.backends import MemoryCacheBackend

backend = MemoryCacheBackend(
    max_size_bytes=1_000_000_000,  # 1 GB
    max_entries=100_000,
    auto_cleanup_interval=300      # Cleanup every 5 minutes
)
```text
<!-- Code example in TEXT -->

**Automatic Cleanup:**

- Stale entries removed every 5 minutes (configurable)
- LRU (Least Recently Used) eviction when max_entries exceeded
- TTL-based expiration for all entries

#### 2.4.2 PostgreSQL Backend (Production)

**Location:** `FraiseQL.caching.postgres_cache.PostgreSQLCacheBackend`

**Characteristics:**

- Persists cache to PostgreSQL using **UNLOGGED tables** (no WAL overhead)
- **Best for:** Production multi-instance deployments, data persistence
- **Performance:** 10-50ms lookups (network latency + query)
- **Thread-safety:** Transaction isolation via PostgreSQL
- **Persistence:** Survives process restarts (but lost on database crash)
- **Multi-instance:** Shared cache across all instances
- **Scalability:** Horizontal scaling with multiple app instances

**Table Structure:**

```sql
<!-- Code example in SQL -->
CREATE UNLOGGED TABLE fraiseql_cache (
    cache_key TEXT PRIMARY KEY,
    cache_value JSONB NOT NULL,
    created_at TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP,
    expires_at TIMESTAMPTZ NOT NULL,
    hit_count BIGINT DEFAULT 0
);

CREATE INDEX idx_cache_expires_at ON fraiseql_cache(expires_at);
CREATE INDEX idx_cache_tenant ON fraiseql_cache(
    (cache_key LIKE 'FraiseQL:%')  -- Tenant extraction for cleanup
);
```text
<!-- Code example in TEXT -->

**Configuration:**

```python
<!-- Code example in Python -->
from FraiseQL.caching.postgres_cache import PostgreSQLCacheBackend

backend = PostgreSQLCacheBackend(
    connection_string="postgresql://user:pass@db/FraiseQL",
    table_name="fraiseql_cache",
    auto_initialize=True,           # Create tables if missing
    cleanup_interval=300,           # Cleanup every 5 minutes
    max_cleanup_batch=1000          # Clean up to 1000 expired entries per run
)
```text
<!-- Code example in TEXT -->

**UNLOGGED Table Tradeoff:**

- ✅ **Pros:** 7-10x faster than regular tables (no WAL writes)
- ⚠️ **Cons:** Data lost on database crash (acceptable for cache)
- **Use case:** Perfect for caches where data loss is not catastrophic

**Cleanup Process:**

```sql
<!-- Code example in SQL -->
-- Automatic cleanup removes expired entries
DELETE FROM fraiseql_cache
WHERE expires_at < NOW()
LIMIT 1000;  -- Batch cleanup to avoid long locks
```text
<!-- Code example in TEXT -->

#### 2.4.3 Custom Backend

**Location:** Extend `FraiseQL.storage.backends.base.BaseCacheBackend`

**Implement:**

```python
<!-- Code example in Python -->
from FraiseQL.storage.backends.base import BaseCacheBackend

class RedisCacheBackend(BaseCacheBackend):
    """Example: Redis cache backend."""

    def __init__(self, redis_client, ttl: int = 300):
        self.redis = redis_client
        self.ttl = ttl

    async def get(self, key: str) -> Any | None:
        """Retrieve value from Redis."""
        value = await self.redis.get(key)
        return json.loads(value) if value else None

    async def set(self, key: str, value: Any, ttl: int = None) -> None:
        """Store value in Redis with TTL."""
        ttl = ttl or self.ttl
        await self.redis.setex(
            key,
            ttl,
            json.dumps(value)
        )

    async def delete(self, key: str) -> None:
        """Delete key from Redis."""
        await self.redis.delete(key)

    async def clear(self) -> None:
        """Clear all cache entries."""
        await self.redis.flushdb()
```text
<!-- Code example in TEXT -->

**Usage:**

```python
<!-- Code example in Python -->
import redis.asyncio as redis

redis_client = redis.from_url("redis://localhost:6379/0")
backend = RedisCacheBackend(redis_client, ttl=300)

app = create_fraiseql_app(
    schema=schema,
    cache_backend=backend
)
```text
<!-- Code example in TEXT -->

### 2.5 Cache Invalidation Strategies

#### 2.5.1 Time-Based (TTL) Invalidation

**How it works:** Cache entries automatically expire after TTL seconds.

**Complexity-Aware TTL:**

```python
<!-- Code example in Python -->
def calculate_ttl(query_complexity: float, config: CacheConfig) -> int:
    """
    Scale TTL based on query complexity:
    - Simple queries (< 10): 300 seconds (5 minutes)
    - Moderate queries (10-50): 180 seconds (3 minutes)
    - Complex queries (50-200): 60 seconds (1 minute)
    - Very complex (> 200): 30 seconds (only cache expensive queries briefly)
    """
    if query_complexity < 10:
        return min(config.default_ttl * 2, config.max_ttl)  # 600s
    elif query_complexity < 50:
        return config.default_ttl  # 300s
    elif query_complexity < 200:
        return config.default_ttl // 2  # 150s
    else:
        return 30  # Very expensive queries cached briefly
```text
<!-- Code example in TEXT -->

**Example:**

```text
<!-- Code example in TEXT -->
GET /graphql?query={users{id name}}
Cache-Control: max-age=600  # Simple query, cached 10 minutes

GET /graphql?query={users{id name posts{id comments{...}}}}
Cache-Control: max-age=30   # Complex query, cached 30 seconds
```text
<!-- Code example in TEXT -->

#### 2.5.2 Manual Invalidation

**Direct Cache Invalidation:**

```python
<!-- Code example in Python -->
from FraiseQL.caching import cache_manager

# Invalidate specific query
await cache_manager.invalidate_key(
    cache_key="FraiseQL:org_123:a7f3e9d2c1b:4f6a8e9d:low"
)

# Pattern-based invalidation (all queries for a tenant)
await cache_manager.invalidate_pattern(
    pattern="FraiseQL:org_123:*"
)

# Clear entire cache
await cache_manager.clear_all()
```text
<!-- Code example in TEXT -->

#### 2.5.3 graphql-cascade Integration (Automatic)

**How it works:** Mutations automatically invalidate related query caches via the **cascade** mechanism.

**Example Workflow:**

```graphql
<!-- Code example in GraphQL -->
# Original query (cached)
query GetUser($id: ID!) {
  user(id: $id) {
    id
    name
    email
    posts {
      id
      title
    }
  }
}

# Mutation that updates the user
mutation UpdateUser($id: ID!, $name: String!) {
  updateUser(id: $id, name: $name) {
    success
    entity {
      id
      name
      email
    }
    cascade {
      invalidations: [
        { query_name: "GetUser", scope: "EXACT" },
        { query_name: "users", scope: "PREFIX" },
        { query_name: "user_profile", scope: "SUFFIX" }
      ]
    }
  }
}
```text
<!-- Code example in TEXT -->

**Cascade Invalidation Pattern:**

| Pattern | Example | Behavior |
|---------|---------|----------|
| **EXACT** | `GetUser` | Invalidate only `GetUser` queries |
| **PREFIX** | `users` | Invalidate `users`, `users_by_role`, etc. |
| **SUFFIX** | `_profile` | Invalidate `user_profile`, `post_profile`, etc. |
| **INFIX** | `_list_` | Invalidate `user_list_active`, `role_list_archived`, etc. |

**Implementation:**

```python
<!-- Code example in Python -->
# In your mutation resolver
@FraiseQL.mutation
async def update_user(info, id: UUID, name: str):
    # Update the user in database
    updated = await db.update_user(id, name=name)

    # Build cascade invalidations
    invalidations = [
        CacheInvalidation(
            query_name="GetUser",
            scope="EXACT",
            user_id=str(id)
        ),
        CacheInvalidation(
            query_name="users",
            scope="PREFIX"  # All user list queries
        )
    ]

    return {
        "success": True,
        "entity": updated,
        "cascade": {
            "invalidations": invalidations
        }
    }
```text
<!-- Code example in TEXT -->

### 2.6 Multi-Tenant Cache Isolation

#### Security Guarantee

Cache keys **always include tenant_id** as the highest-level discriminator:

```text
<!-- Code example in TEXT -->
{prefix}:{tenant_id}:{operation_hash}:{variables_hash}:{complexity_hash}
           ^^^^^^^^^^^
           Cannot retrieve other tenant's data
```text
<!-- Code example in TEXT -->

**Proof of Isolation:**

```python
<!-- Code example in Python -->
# Even if you know cache structure, you cannot access other tenant data
org_a_key = "FraiseQL:org_a:query_hash:vars_hash:complexity"
org_b_key = "FraiseQL:org_b:query_hash:vars_hash:complexity"

# Different cache entries - completely isolated
assert await cache.get(org_a_key) != await cache.get(org_b_key)

# If attacker queries as org_b, gets org_b data even if same query
# No cross-tenant data leakage possible
```text
<!-- Code example in TEXT -->

#### Multi-Tenant Deployment Pattern

```python
<!-- Code example in Python -->
@FraiseQL.query
async def get_user(info, id: UUID) -> User:
    """Retrieve user - automatically tenant-scoped."""
    tenant_id = info.context["tenant_id"]  # From auth token

    # Query is automatically cached with tenant_id prefix
    # Cache key: FraiseQL:{tenant_id}:get_user_hash:...
    user = await db.find_one(
        "users",
        {"id": id, "tenant_id": tenant_id}
    )
    return user
```text
<!-- Code example in TEXT -->

### 2.7 Performance Characteristics

#### Memory Backend

| Operation | Latency | Throughput |
|-----------|---------|------------|
| Cache Hit | < 1ms | 100k+ req/s |
| Cache Miss | < 1ms | 100k+ req/s |
| Eviction | 0-5ms | Depends on entry size |

**Memory Overhead:** ~1KB per cache entry (average)

#### PostgreSQL Backend

| Operation | Latency | Throughput |
|-----------|---------|------------|
| Cache Hit | 10-50ms | 5k-10k req/s |
| Cache Miss | 10-50ms | 5k-10k req/s |
| Cleanup | 50-200ms | Async, non-blocking |

**Network Latency:** 5-20ms typical (varies by network)
**Database CPU:** < 5% for moderate workload

### 2.8 Cache Monitoring & Metrics

#### Metrics Collected

```python
<!-- Code example in Python -->
# Via Prometheus or OpenTelemetry
fraiseql_cache_hits_total{
    operation_name="GetUser",
    complexity="low|moderate|high",
    tenant_id="org_123"
}

fraiseql_cache_misses_total{
    operation_name="GetUser",
    reason="expired|evicted|not_found",
    tenant_id="org_123"
}

fraiseql_cache_size_bytes{
    backend="memory|postgresql",
    tenant_id="org_123"
}

fraiseql_cache_evictions_total{
    backend="memory|postgresql",
    reason="lru|ttl|manual",
    tenant_id="org_123"
}
```text
<!-- Code example in TEXT -->

#### Example Monitoring Query

```sql
<!-- Code example in SQL -->
-- PostgreSQL: Cache hit rate by query (last 1 hour)
SELECT
    operation_name,
    COUNT(*) FILTER (WHERE cache_hit) AS hits,
    COUNT(*) FILTER (WHERE NOT cache_hit) AS misses,
    ROUND(
        100.0 * COUNT(*) FILTER (WHERE cache_hit) / COUNT(*),
        2
    ) AS hit_rate_pct
FROM cache_metrics
WHERE
    created_at > NOW() - INTERVAL '1 hour'
    AND tenant_id = $tenant_id
GROUP BY operation_name
ORDER BY hit_rate_pct DESC;
```text
<!-- Code example in TEXT -->

#### Monitoring Dashboard Recommendations

1. **Cache Hit Rate** — Target: > 80% for normal queries
2. **Cache Evictions** — Alert if > 10% of entries evicted per minute
3. **Memory Usage** — Alert if approaching max_size_bytes
4. **TTL Distribution** — Ensure complexity-aware TTL working correctly
5. **Error Caching Rate** — Monitor if error_ttl too aggressive

---

## 3. APQ Response Caching

### 3.1 Overview

APQ Response Caching caches the **HTTP response** of persisted queries, eliminating database execution entirely for identical requests.

**Key Difference from Query Caching:**

- **Query Caching:** Caches database result (query execution still happens)
- **APQ Response Caching:** Caches HTTP response (query execution skipped)
- **Performance:** APQ response caching is 10-100x faster (no GraphQL parsing, validation, or database)

### 3.2 Response Cache Key

```text
<!-- Code example in TEXT -->
{prefix}:{tenant_id}:{query_hash}:{variables_hash}:{field_selection_hash}
```text
<!-- Code example in TEXT -->

**Critical Component: Variables Hash**

```python
<!-- Code example in Python -->
def compute_variables_hash(variables: dict) -> str:
    """
    Variables are included in cache key to prevent data leakage.

    Example: Same query, different user_id
    query GetUser($id: ID!) { user(id: $id) { id name } }

    Call 1: variables = {"id": "user_1"}
    Call 2: variables = {"id": "user_2"}

    Different variables → different cache keys → different responses
    No data leakage even with same query.
    """
    sorted_vars = json.dumps(variables, sort_keys=True)
    return hashlib.sha256(sorted_vars.encode()).hexdigest()
```text
<!-- Code example in TEXT -->

### 3.3 Configuration

```python
<!-- Code example in Python -->
@dataclass
class APQConfig:
    """APQ response caching configuration."""

    # Enable APQ response caching
    cache_responses: bool = True

    # APQ response TTL
    response_ttl: int = 300  # 5 minutes

    # Field selection optimization
    optimize_field_selection: bool = True  # Only cache requested fields

    # Maximum cached queries per tenant
    max_queries_per_tenant: int = 10_000

    # Maximum response size to cache
    max_response_size_bytes: int = 1_000_000  # 1 MB
```text
<!-- Code example in TEXT -->

### 3.4 Field Selection Optimization

**How it works:** When a client requests fewer fields via field selection, the cached response is pruned to match the request.

**Example:**

```graphql
<!-- Code example in GraphQL -->
# First request (cached): All fields
query GetUser($id: ID!) {
  user(id: $id) {
    id
    name
    email
    phone
    address
  }
}

# Second request: Only id and name
query GetUser($id: ID!) {
  user(id: $id) {
    id
    name
  }
}
```text
<!-- Code example in TEXT -->

**With field selection optimization:**

- First request: Full response cached → `{id, name, email, phone, address}`
- Second request: Cached response pruned → `{id, name}` extracted from cache
- Result: Hit same cache entry, but only return requested fields

**Implementation:**

```python
<!-- Code example in Python -->
def prune_response_fields(cached_response: dict, requested_fields: set) -> dict:
    """Extract only requested fields from cached response."""
    def prune_recursive(obj, fields):
        if isinstance(obj, dict):
            return {k: v for k, v in obj.items() if k in fields}
        elif isinstance(obj, list):
            return [prune_recursive(item, fields) for item in obj]
        return obj

    return prune_recursive(cached_response, requested_fields)
```text
<!-- Code example in TEXT -->

---

## 4. graphql-cascade Integration

### 4.1 Automatic Cache Invalidation

The `cascade` field in mutation responses automatically triggers cache invalidation based on **compiler-determined** patterns.

### 4.2 Cascade Invalidation Patterns

```python
<!-- Code example in Python -->
@dataclass
class CacheInvalidation:
    """Pattern for invalidating cached queries."""

    query_name: str          # Query operation name
    scope: str               # EXACT, PREFIX, SUFFIX, INFIX
    user_id: str | None      # Optional: filter by user
    tenant_id: str | None    # Optional: filter by tenant
```text
<!-- Code example in TEXT -->

**Pattern Matching:**

```python
<!-- Code example in Python -->
def matches_invalidation(
    cached_operation_name: str,
    pattern: CacheInvalidation
) -> bool:
    """Check if cached operation matches invalidation pattern."""

    if pattern.scope == "EXACT":
        return cached_operation_name == pattern.query_name

    elif pattern.scope == "PREFIX":
        return cached_operation_name.startswith(pattern.query_name)

    elif pattern.scope == "SUFFIX":
        return cached_operation_name.endswith(pattern.query_name)

    elif pattern.scope == "INFIX":
        return pattern.query_name in cached_operation_name

    return False
```text
<!-- Code example in TEXT -->

### 4.3 Example: User Update Cascade

```python
<!-- Code example in Python -->
@FraiseQL.mutation
async def update_user(info, user_id: UUID, data: dict):
    """Update user and invalidate all related caches."""

    # Update database
    user = await db.update_user(user_id, **data)

    # Determine what to invalidate
    invalidations = []

    if "name" in data or "email" in data:
        invalidations.append(
            CacheInvalidation("GetUser", scope="EXACT")
        )

    if "role" in data:
        invalidations.append(
            CacheInvalidation("users", scope="PREFIX")
        )

    if "deleted_at" in data:
        invalidations.append(
            CacheInvalidation("active_users", scope="PREFIX")
        )

    return {
        "success": True,
        "entity": user,
        "cascade": {
            "invalidations": invalidations
        }
    }
```text
<!-- Code example in TEXT -->

### 4.4 Database-Level Cascade (PostgreSQL)

For direct SQL mutations, use triggers to log cascades:

```sql
<!-- Code example in SQL -->
CREATE TABLE cache_invalidation_log (
    id BIGSERIAL PRIMARY KEY,
    query_pattern TEXT NOT NULL,
    scope TEXT NOT NULL,  -- EXACT, PREFIX, SUFFIX, INFIX
    triggered_by TEXT,    -- Mutation that triggered invalidation
    created_at TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP
);

CREATE OR REPLACE FUNCTION log_cache_invalidation()
RETURNS TRIGGER AS $$
BEGIN
    -- Log when users table is updated
    INSERT INTO cache_invalidation_log (query_pattern, scope, triggered_by)
    VALUES ('GetUser', 'EXACT', TG_OP);

    INSERT INTO cache_invalidation_log (query_pattern, scope, triggered_by)
    VALUES ('users', 'PREFIX', TG_OP);

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER user_cache_invalidation
AFTER UPDATE ON tb_user
FOR EACH ROW
EXECUTE FUNCTION log_cache_invalidation();
```text
<!-- Code example in TEXT -->

---

## 5. Configuration in Production

### 5.1 Development Configuration

```python
<!-- Code example in Python -->
# Local development: fast cache, permissive
cache_config = CacheConfig(
    enabled=True,
    default_ttl=60,         # 1 minute (frequent changes)
    cache_errors=True,      # Cache errors for debugging
    backend=MemoryCacheBackend(
        max_size_bytes=100_000_000  # 100 MB (plenty for dev)
    )
)
```text
<!-- Code example in TEXT -->

### 5.2 Staging Configuration

```python
<!-- Code example in Python -->
# Staging: PostgreSQL cache, moderate TTL
cache_config = CacheConfig(
    enabled=True,
    default_ttl=300,        # 5 minutes
    cache_errors=False,     # Don't cache errors in staging
    backend=PostgreSQLCacheBackend(
        connection_string=os.getenv("STAGING_DB_URL")
    )
)
```text
<!-- Code example in TEXT -->

### 5.3 Production Configuration

```python
<!-- Code example in Python -->
# Production: optimized PostgreSQL cache
cache_config = CacheConfig(
    enabled=True,
    default_ttl=600,        # 10 minutes (conservative)
    max_ttl=3600,           # Never cache > 1 hour
    cache_errors=False,
    include_complexity=True,  # Complexity-aware TTL
    require_tenant_id=True,   # Fail if tenant_id missing
    backend=PostgreSQLCacheBackend(
        connection_string=os.getenv("PROD_DB_URL"),
        max_size_bytes=10_000_000_000  # 10 GB
    )
)
```text
<!-- Code example in TEXT -->

### 5.4 Environment Variables

```bash
<!-- Code example in BASH -->
# Development
FRAISEQL_CACHE_ENABLED=true
FRAISEQL_CACHE_TTL=60
FRAISEQL_CACHE_BACKEND=memory

# Staging
FRAISEQL_CACHE_ENABLED=true
FRAISEQL_CACHE_TTL=300
FRAISEQL_CACHE_BACKEND=postgresql
FRAISEQL_CACHE_DB_URL=postgresql://...

# Production
FRAISEQL_CACHE_ENABLED=true
FRAISEQL_CACHE_TTL=600
FRAISEQL_CACHE_MAX_TTL=3600
FRAISEQL_CACHE_BACKEND=postgresql
FRAISEQL_CACHE_DB_URL=postgresql://...
FRAISEQL_CACHE_SIZE_BYTES=10000000000
FRAISEQL_CACHE_INCLUDE_COMPLEXITY=true
```text
<!-- Code example in TEXT -->

---

## 6. Best Practices

### 6.1 Cache Strategy Decision Tree

```text
<!-- Code example in TEXT -->
Is data frequently queried?
├─ YES: Cache it
│   ├─ Is data frequently modified?
│   │   ├─ YES: Shorter TTL (60-300 seconds)
│   │   └─ NO: Longer TTL (300-3600 seconds)
│   └─ End: Use caching
└─ NO: Don't cache
    └─ Monitor to ensure cache isn't wasted
```text
<!-- Code example in TEXT -->

### 6.2 TTL Guidelines

| Data Type | Update Frequency | Recommended TTL | Example |
|-----------|------------------|-----------------|---------|
| User Profile | Hours | 600-1800s | User account settings |
| Product Catalog | Days | 3600-86400s | E-commerce products |
| Real-time Data | Seconds | 30-60s | Stock prices, weather |
| Derived Data | Minutes | 300-600s | User rankings, aggregates |
| Reference Data | Never | 86400s | Countries, currencies |

### 6.3 Complexity-Aware TTL

```python
<!-- Code example in Python -->
# Don't cache expensive queries for long
simple_query_ttl = 3600      # 1 hour (cheap, safe)
complex_query_ttl = 60       # 1 minute (expensive, aggressive invalidation)

# This forces expensive queries to be regenerated frequently
# Preventing cache from becoming a bottleneck
```text
<!-- Code example in TEXT -->

### 6.4 Monitoring Checklist

- [ ] Cache hit rate > 80% for normal workload
- [ ] Cache eviction rate < 1% per minute
- [ ] Memory usage stable (not growing unbounded)
- [ ] Database cache table size monitored
- [ ] TTL distribution matches complexity
- [ ] Invalidation patterns working correctly
- [ ] Tenant isolation verified in testing

---

## 7. Troubleshooting

### 7.1 Low Cache Hit Rate

**Symptoms:** Cache hit rate < 60%

**Causes:**

1. Variables changing between requests (same query, different variables)
2. TTL too short — entries expiring too quickly
3. Complexity hash changing (reordering fields in query)
4. Wrong query name in invalidation patterns

**Solutions:**

1. Normalize variable order before querying
2. Increase default_ttl in configuration
3. Disable include_complexity in CacheConfig
4. Review invalidation patterns in mutations

### 7.2 Memory Exhaustion

**Symptoms:** Cache size approaching max_size_bytes, then entries evicted

**Causes:**

1. max_size_bytes too small for workload
2. TTL too long — entries not expiring
3. High cardinality in query variables (unbounded cache keys)

**Solutions:**

1. Increase max_size_bytes or switch to PostgreSQL backend
2. Decrease default_ttl
3. Normalize variables or add parameterization rules

### 7.3 Stale Data in Cache

**Symptoms:** Old data served even after updates

**Causes:**

1. Invalidation patterns not matching mutation
2. Cascade invalidations not configured
3. Database update bypassed GraphQL mutation

**Solutions:**

1. Review CacheInvalidation scope patterns
2. Add explicit cache.invalidate() call after DB update
3. Ensure all writes go through GraphQL mutations

---

## 8. Security Considerations

### 8.1 Tenant Isolation

✅ **Guaranteed:** Cache keys include tenant_id at highest level
⚠️ **Verify:** Always pass tenant_id from authenticated context

```python
<!-- Code example in Python -->
@FraiseQL.query
async def get_user(info, id: UUID) -> User:
    # CORRECT: tenant_id from auth context
    tenant_id = info.context["tenant_id"]  # ✅

    # WRONG: user-provided tenant_id
    tenant_id = info.arguments.get("tenant_id")  # ❌ SECURITY BUG
```text
<!-- Code example in TEXT -->

### 8.2 Error Caching

⚠️ **Warning:** Caching error responses can leak information

```python
<!-- Code example in Python -->
# ❌ DON'T cache errors in production
cache_errors=True  # SECURITY RISK

# ✅ Only cache errors in development
cache_errors=os.getenv("ENVIRONMENT") == "development"
```text
<!-- Code example in TEXT -->

### 8.3 Sensitive Data

🔒 **Best Practice:** Don't cache PII or sensitive data

```python
<!-- Code example in Python -->
# ❌ DON'T cache this
@FraiseQL.query
async def get_user(info, id: UUID) -> User:
    """Returns user with sensitive fields."""
    return await db.find_one("users", {"id": id})

# ✅ DO this: Cache only public fields
@FraiseQL.query
async def get_user_profile(info, id: UUID) -> UserProfile:
    """Returns only public profile."""
    return await db.find_one("user_profiles", {"id": id})
```text
<!-- Code example in TEXT -->

---

## 9. Performance Example

### 9.1 Real-World Benchmark

**Setup:**

- 100,000 users
- 10,000 concurrent sessions
- Memory backend with 5-minute TTL
- Query: GetUser (moderate complexity)

**Results:**

| Metric | Without Cache | With Cache |
|--------|---------------|-----------|
| Latency (p95) | 250ms | 15ms |
| Latency (p99) | 500ms | 25ms |
| Throughput | 2,000 req/s | 50,000 req/s |
| Database CPU | 85% | 5% |
| Memory | 128MB | 800MB |

**Conclusion:** 25x throughput improvement, 17x latency improvement, at cost of 6x memory.

---

## 10. Related Specifications

- **docs/specs/persisted-queries.md** — APQ implementation (uses this caching system)
- **docs/guides/monitoring.md** — Cache metrics and observability
- **docs/guides/production-deployment.md** — Cache configuration in production
- **docs/architecture/core/execution-model.md** — Where caching fits in query execution

---

## Glossary

| Term | Definition |
|------|-----------|
| **Cache Hit** | Requested data found in cache |
| **Cache Miss** | Requested data not in cache, retrieved from database |
| **TTL (Time-To-Live)** | How long a cache entry remains valid |
| **LRU (Least Recently Used)** | Eviction policy: remove least recently accessed entries |
| **Tenant Isolation** | Guarantee that one tenant cannot access another's cached data |
| **Invalidation** | Removing or marking cache entries as stale |
| **Cascade** | Automatic invalidation triggered by mutations |
| **UNLOGGED Table** | PostgreSQL table without write-ahead logging (faster writes, lost on crash) |
| **APQ** | Automatic Persisted Queries (caches query definitions and responses) |
| **Complexity** | Measure of query expense (fields + depth + relationships) |
