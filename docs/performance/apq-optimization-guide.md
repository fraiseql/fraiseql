# APQ Optimization Guide

**FraiseQL Automatic Persisted Queries - Performance Tuning & Best Practices**

---

## Table of Contents

1. [Overview](#overview)
2. [Understanding APQ](#understanding-apq)
3. [When to Enable APQ](#when-to-enable-apq)
4. [Configuration Guide](#configuration-guide)
5. [Monitoring & Metrics](#monitoring-metrics)
6. [Optimization Strategies](#optimization-strategies)
7. [Troubleshooting](#troubleshooting)
8. [Production Best Practices](#production-best-practices)

---

## Overview

APQ (Automatic Persisted Queries) is a GraphQL optimization technique that eliminates query parsing overhead by caching parsed queries by their SHA256 hash. FraiseQL's APQ implementation provides **two layers of caching**:

1. **Query Cache**: Stores query strings by hash (always active)
2. **Response Cache**: Stores complete query responses (optional)

### Performance Impact

**Query Cache Benefits:**
- Eliminates 20-80ms query parsing overhead per request
- Reduces network payload (hash instead of full query)
- Target: 90%+ hit rate in production

**Response Cache Benefits:**
- Can provide 260-460x speedup for identical queries
- Bypasses GraphQL execution entirely
- Best for read-heavy, cacheable data

---

## Understanding APQ

### Two-Layer Caching Strategy

FraiseQL uses a sophisticated caching approach:

```
┌──────────────────────────────────────────────────────────┐
│                    APQ Request Flow                       │
└──────────────────────────────────────────────────────────┘

1. Client sends: {"extensions": {"persistedQuery": {"sha256Hash": "abc123..."}}}

2. FraiseQL checks Response Cache (if enabled)
   ├─ HIT  → Return cached response immediately (fastest)
   └─ MISS → Continue to step 3

3. FraiseQL checks Query Cache
   ├─ HIT  → Use cached query string, execute GraphQL
   └─ MISS → Request full query from client, store it

4. Execute GraphQL query → Generate response

5. Store response in Response Cache (if enabled, for future requests)

6. Return response to client
```

### When to Use Each Layer

**Query Cache (Always Use):**
- ✅ All production environments
- ✅ Development (helpful for debugging)
- ✅ No downside, minimal overhead
- ✅ Automatic query string deduplication

**Response Cache (Selective Use):**
- ✅ Read-heavy APIs with cacheable data
- ✅ Public data that doesn't change frequently
- ✅ Queries without user-specific data
- ❌ User-specific queries (unless using tenant isolation)
- ❌ Real-time data requirements
- ❌ High mutation rate data

---

## When to Enable APQ

### Query Cache (Default: Enabled)

**Always enable query caching** - it provides pure performance benefits with no downsides.

Benefits:
- Eliminates query parsing overhead
- Reduces network payload size
- Improves response time consistency
- Automatic deduplication of queries

### Response Cache (Default: Disabled)

**Enable response caching** when you have:

1. **Cacheable Data Patterns:**
   - Public data (blogs, docs, product catalogs)
   - Reference data (countries, currencies, categories)
   - Aggregated statistics
   - Infrequently changing data

2. **Traffic Patterns:**
   - Repeated identical queries
   - High read-to-write ratio (>10:1)
   - Predictable query patterns

3. **Performance Requirements:**
   - Sub-10ms response time targets
   - High throughput requirements (>1000 req/s)
   - Cost optimization (reduce compute)

**Do NOT enable response caching** when:
- Data changes frequently (real-time updates)
- Queries are highly personalized
- Strong consistency requirements
- Complex authorization rules

---

## Configuration Guide

### Basic Configuration

```python
from fraiseql.fastapi.config import FraiseQLConfig

# Query cache only (recommended starting point)
config = FraiseQLConfig(
    db_url="postgresql://...",
    apq_storage_backend="memory",  # or "postgresql", "redis"
    apq_cache_responses=False,     # Response caching disabled
)

# Full APQ with response caching
config = FraiseQLConfig(
    db_url="postgresql://...",
    apq_storage_backend="memory",
    apq_cache_responses=True,      # Enable response caching
    apq_backend_config={
        "response_ttl": 300,        # 5 minutes
    }
)
```

### Storage Backend Options

#### 1. Memory Backend (Default)

**Best for:** Development, small deployments, single-instance apps

```python
config = FraiseQLConfig(
    apq_storage_backend="memory",
)
```

**Pros:**
- Fastest performance (<0.1ms lookup)
- Zero external dependencies
- Simple configuration

**Cons:**
- Lost on restart
- Not shared across instances
- Memory consumption grows with queries

**Recommended:** Development and single-server production

#### 2. PostgreSQL Backend

**Best for:** Production, multi-instance deployments, persistence

```python
config = FraiseQLConfig(
    apq_storage_backend="postgresql",
    apq_backend_config={
        "db_url": "postgresql://...",
        "table_name": "apq_cache",
        "response_ttl": 300,  # 5 minutes
    }
)
```

**Pros:**
- Shared across instances
- Survives restarts
- Leverages existing PostgreSQL infrastructure
- Automatic cleanup via TTL

**Cons:**
- Slightly slower than memory (~1-2ms)
- Requires database connection
- Additional database load

**Recommended:** Production with multiple app instances

#### 3. Redis Backend

**Best for:** High-traffic production, distributed systems

```python
config = FraiseQLConfig(
    apq_storage_backend="redis",
    apq_backend_config={
        "redis_url": "redis://localhost:6379/0",
        "key_prefix": "fraiseql:apq:",
        "response_ttl": 300,
    }
)
```

**Pros:**
- Fastest distributed cache (~0.5-1ms)
- Shared across instances
- Built-in TTL and eviction
- Scales independently

**Cons:**
- Additional infrastructure
- Network latency
- Serialization overhead

**Recommended:** High-traffic production (>1000 req/s)

---

## Monitoring & Metrics

### Dashboard Access

Access the interactive monitoring dashboard:

```
http://your-server:port/admin/apq/dashboard
```

Features:
- Real-time hit rate visualization
- Top queries analysis
- Health status monitoring
- Performance trends

### Key Metrics to Monitor

#### 1. Query Cache Hit Rate

**Target:** >70% (ideally >90%)

```bash
curl http://localhost:8000/admin/apq/health
```

**What it means:**
- **>90%**: Excellent - queries are being reused effectively
- **70-90%**: Good - normal for varied query patterns
- **50-70%**: Warning - high query diversity or cache warming needed
- **<50%**: Critical - investigate query patterns or cache configuration

#### 2. Response Cache Hit Rate

**Target:** >50% (when enabled)

**What it means:**
- **>80%**: Excellent - significant performance gains
- **50-80%**: Good - response caching is beneficial
- **30-50%**: Marginal - consider disabling if overhead isn't worth it
- **<30%**: Poor - disable response caching

#### 3. Top Queries

Monitor the top queries endpoint:

```bash
curl http://localhost:8000/admin/apq/top-queries?limit=10
```

**Look for:**
- High miss rate on frequent queries (cache warming opportunity)
- Queries with long parse times (optimization candidates)
- Unexpected query patterns (potential issues)

### Prometheus Integration

Add to your Prometheus configuration:

```yaml
# prometheus.yml
scrape_configs:
  - job_name: 'fraiseql-apq'
    metrics_path: '/admin/apq/metrics'
    scrape_interval: 15s
    static_configs:
      - targets: ['localhost:8000']
```

Available metrics:
- `apq_query_cache_hit_rate`: Query cache effectiveness
- `apq_response_cache_hit_rate`: Response cache effectiveness
- `apq_requests_total`: Total APQ requests
- `apq_storage_bytes_total`: Cache memory usage
- `apq_health_status`: System health status

---

## Optimization Strategies

### 1. Improve Query Cache Hit Rate

#### Strategy: Cache Warming

Pre-populate the cache with common queries:

```python
from fraiseql.storage.apq_store import store_persisted_query, compute_query_hash

# Get top queries from analytics
top_queries = [
    "query GetUsers { users { id name email } }",
    "query GetPosts { posts { id title content } }",
    # ... more queries
]

# Pre-warm the cache
for query in top_queries:
    hash_value = compute_query_hash(query)
    store_persisted_query(hash_value, query)
```

#### Strategy: Client-Side APQ

Configure your GraphQL client to use APQ:

**Apollo Client:**
```javascript
import { createPersistedQueryLink } from "@apollo/client/link/persisted-queries";
import { sha256 } from "crypto-hash";

const link = createPersistedQueryLink({ sha256 });
```

**urql:**
```javascript
import { Client, cacheExchange, fetchExchange } from "urql";
import { persistedExchange } from "@urql/exchange-persisted";

const client = new Client({
  exchanges: [persistedExchange({ generateHash: sha256 }), cacheExchange, fetchExchange],
});
```

### 2. Optimize Response Cache Hit Rate

#### Strategy: Tenant Isolation

For multi-tenant applications:

```python
from fraiseql.middleware.apq_caching import handle_apq_request_with_cache

# Add tenant context
context = {"tenant_id": request.headers.get("X-Tenant-ID")}

cached_response = handle_apq_request_with_cache(
    request=graphql_request,
    backend=backend,
    config=config,
    context=context,  # Tenant-specific caching
)
```

#### Strategy: TTL Tuning

Adjust response TTL based on data freshness requirements:

```python
# Aggressive caching (5-15 minutes)
apq_backend_config={"response_ttl": 900}  # 15 minutes

# Moderate caching (1-5 minutes)
apq_backend_config={"response_ttl": 300}  # 5 minutes

# Short-term caching (30-60 seconds)
apq_backend_config={"response_ttl": 60}  # 1 minute
```

#### Strategy: Selective Caching

Cache only specific query types:

```python
from fraiseql.middleware.apq_caching import is_cacheable_response

def custom_is_cacheable(response: dict, query_string: str) -> bool:
    """Custom caching logic."""
    # Only cache read-only queries
    if "mutation" in query_string.lower():
        return False

    # Don't cache queries with specific directives
    if "@nocache" in query_string:
        return False

    # Use default logic
    return is_cacheable_response(response)
```

### 3. Storage Optimization

#### Monitor Cache Size

```python
from fraiseql.storage.apq_store import get_storage_stats

stats = get_storage_stats()
print(f"Stored queries: {stats['stored_queries']}")
print(f"Total size: {stats['total_size_bytes'] / 1024:.1f} KB")
```

#### Implement Eviction (PostgreSQL/Redis)

PostgreSQL backend automatically cleans up expired entries. For memory backend, implement periodic cleanup:

```python
import asyncio
from fraiseql.storage.apq_store import clear_storage

async def periodic_cleanup():
    """Clear cache every 24 hours."""
    while True:
        await asyncio.sleep(86400)  # 24 hours
        clear_storage()
        print("APQ cache cleared")

# Run in background
asyncio.create_task(periodic_cleanup())
```

---

## Troubleshooting

### Problem: Low Query Cache Hit Rate (<70%)

**Diagnosis:**
```bash
curl http://localhost:8000/admin/apq/top-queries?limit=20
```

**Common Causes:**

1. **Client not configured for APQ**
   - Solution: Configure GraphQL client to send `persistedQuery` extension
   - Verify: Check network requests for `extensions.persistedQuery.sha256Hash`

2. **High query diversity**
   - Solution: This is expected for APIs with many unique queries
   - Target: Optimize the most frequent queries instead of all queries

3. **Cache cleared frequently**
   - Solution: Use PostgreSQL or Redis backend instead of memory
   - Verify: Check `apq_stored_queries_total` metric over time

4. **Development environment**
   - Solution: Low hit rates are normal during development
   - Action: Focus on production metrics

### Problem: Response Cache Not Working

**Diagnosis:**
```bash
curl http://localhost:8000/admin/apq/health
# Check response_cache_hit_rate
```

**Common Causes:**

1. **Response caching disabled**
   ```python
   # Check config
   config = FraiseQLConfig(apq_cache_responses=True)  # Must be True
   ```

2. **Queries with errors**
   - Responses with errors are never cached
   - Solution: Fix query errors or validation issues

3. **User-specific queries**
   - Different users get different responses
   - Solution: Implement tenant isolation with context

4. **Cache expired**
   - TTL too short for query patterns
   - Solution: Increase `response_ttl` in config

### Problem: High Memory Usage

**Diagnosis:**
```bash
curl http://localhost:8000/admin/apq/metrics | grep storage_bytes
```

**Solutions:**

1. **Switch to PostgreSQL backend:**
   ```python
   config = FraiseQLConfig(apq_storage_backend="postgresql")
   ```

2. **Reduce response TTL:**
   ```python
   apq_backend_config={"response_ttl": 60}  # Shorter expiration
   ```

3. **Implement cache size limits:**
   ```python
   from fraiseql.storage.apq_store import get_storage_stats, clear_storage

   stats = get_storage_stats()
   if stats["total_size_bytes"] > 100 * 1024 * 1024:  # 100MB
       clear_storage()
   ```

### Problem: Stale Data Being Served

**Diagnosis:**
Response cache serving outdated data after mutations

**Solutions:**

1. **Disable response caching:**
   ```python
   config = FraiseQLConfig(apq_cache_responses=False)
   ```

2. **Reduce TTL for volatile data:**
   ```python
   apq_backend_config={"response_ttl": 30}  # 30 seconds
   ```

3. **Implement cache invalidation:**
   ```python
   from fraiseql.storage import apq_store

   # After mutation
   apq_store.clear_storage()  # Clear all caches
   ```

4. **Use materialized views instead:**
   - FraiseQL already uses `tv_{entity}` materialized views
   - These provide data-level caching at PostgreSQL layer
   - More appropriate for frequently changing data

---

## Production Best Practices

### 1. Configuration Checklist

✅ **Always Enable:**
- [ ] Query caching (`apq_storage_backend` configured)
- [ ] Metrics tracking (automatic)
- [ ] Health monitoring endpoint
- [ ] Dashboard access for operations team

✅ **Consider Enabling:**
- [ ] Response caching (if read-heavy workload)
- [ ] PostgreSQL/Redis backend (if multi-instance)
- [ ] Prometheus integration (if using monitoring)

✅ **Never Do:**
- [ ] Enable response caching for user-specific data without tenant isolation
- [ ] Use memory backend in multi-instance deployments
- [ ] Ignore health warnings (hit rate <50%)

### 2. Monitoring Setup

**Set up alerts for:**

1. **Critical Alert: Hit Rate <50%**
   ```yaml
   # Prometheus alert
   - alert: APQHitRateCritical
     expr: apq_query_cache_hit_rate < 0.5
     for: 10m
     labels:
       severity: critical
   ```

2. **Warning Alert: Hit Rate <70%**
   ```yaml
   - alert: APQHitRateWarning
     expr: apq_query_cache_hit_rate < 0.7
     for: 30m
     labels:
       severity: warning
   ```

3. **Storage Alert: High Memory Usage**
   ```yaml
   - alert: APQHighStorage
     expr: apq_storage_bytes_total > 100 * 1024 * 1024
     for: 5m
     labels:
       severity: warning
   ```

### 3. Performance Testing

Before enabling in production:

1. **Baseline without APQ:**
   ```bash
   # Disable APQ
   config = FraiseQLConfig(apq_storage_backend=None)

   # Run load test
   ab -n 10000 -c 100 http://localhost:8000/graphql
   ```

2. **Test with query cache only:**
   ```bash
   config = FraiseQLConfig(
       apq_storage_backend="memory",
       apq_cache_responses=False,
   )
   ```

3. **Test with full APQ:**
   ```bash
   config = FraiseQLConfig(
       apq_storage_backend="memory",
       apq_cache_responses=True,
   )
   ```

4. **Compare metrics:**
   - Response time percentiles (p50, p95, p99)
   - Throughput (requests/second)
   - Memory usage
   - CPU usage

### 4. Rollout Strategy

**Phase 1: Query Cache Only**
1. Enable memory backend in production
2. Monitor for 1 week
3. Verify hit rate >70%
4. No rollback needed (pure performance gain)

**Phase 2: PostgreSQL Backend** (if multi-instance)
1. Deploy PostgreSQL backend to canary
2. Monitor for 48 hours
3. Verify no increased latency
4. Roll out to production

**Phase 3: Response Caching** (if applicable)
1. Enable for read-only, public queries only
2. Start with short TTL (60s)
3. Monitor for stale data issues
4. Gradually increase TTL if no issues
5. Rollback plan: Set `apq_cache_responses=False`

### 5. Maintenance

**Daily:**
- Check dashboard for warnings
- Monitor hit rates
- Review top queries

**Weekly:**
- Analyze hit rate trends
- Review storage usage
- Check for query pattern changes

**Monthly:**
- Review and optimize top queries
- Audit cache effectiveness
- Update TTL configuration if needed

**Quarterly:**
- Performance benchmark comparison
- Review backend choice (memory vs PostgreSQL vs Redis)
- Consider cache warming strategies

---

## Advanced Topics

### Custom Cache Backends

Implement custom storage backend:

```python
from fraiseql.storage.backends.base import APQStorageBackend

class CustomBackend(APQStorageBackend):
    def get_persisted_query(self, hash_value: str) -> str | None:
        # Your implementation
        pass

    def store_persisted_query(self, hash_value: str, query: str) -> None:
        # Your implementation
        pass

    def get_cached_response(self, hash_value: str, context=None) -> dict | None:
        # Your implementation
        pass

    def store_cached_response(self, hash_value: str, response: dict, context=None) -> None:
        # Your implementation
        pass
```

### Integration with CDN

For public APIs, combine with CDN caching:

```python
from fastapi import Response

@app.post("/graphql")
async def graphql_endpoint(request: GraphQLRequest, response: Response):
    # Add cache headers for CDN
    if is_public_query(request):
        response.headers["Cache-Control"] = "public, max-age=300"

    # APQ handles query and response caching
    return await execute_graphql(request)
```

### Multi-Tier Caching Strategy

Combine FraiseQL caching layers:

```
┌────────────────────────────────────────────────────┐
│ CDN Layer (Cloudflare, Fastly)                    │
│ • Full response caching                            │
│ • 5-15 minute TTL                                  │
│ • Public queries only                              │
└─────────────────┬──────────────────────────────────┘
                  │ CDN miss
                  ↓
┌────────────────────────────────────────────────────┐
│ APQ Response Cache                                 │
│ • FraiseQL in-process or Redis                     │
│ • 1-5 minute TTL                                   │
│ • All cacheable queries                            │
└─────────────────┬──────────────────────────────────┘
                  │ Response cache miss
                  ↓
┌────────────────────────────────────────────────────┐
│ APQ Query Cache                                    │
│ • Eliminates parsing overhead                      │
│ • Permanent (no TTL)                               │
│ • All queries                                      │
└─────────────────┬──────────────────────────────────┘
                  │ Query cache miss
                  ↓
┌────────────────────────────────────────────────────┐
│ PostgreSQL Materialized Views (tv_{entity})       │
│ • Data-level caching                               │
│ • Refresh strategy configured per entity           │
│ • All queries                                      │
└─────────────────┬──────────────────────────────────┘
                  │ Materialized view miss
                  ↓
┌────────────────────────────────────────────────────┐
│ PostgreSQL Base Tables                             │
│ • Source of truth                                  │
│ • Full query execution                             │
└────────────────────────────────────────────────────┘
```

---

## Summary

### Quick Decision Matrix

| Scenario | Query Cache | Response Cache | Backend |
|----------|-------------|----------------|---------|
| Development | ✅ Memory | ❌ Disabled | Memory |
| Single instance production | ✅ Memory | ⚠️ Selective | Memory |
| Multi-instance production | ✅ PostgreSQL | ⚠️ Selective | PostgreSQL |
| High-traffic (>1000 req/s) | ✅ Redis | ✅ Enabled | Redis |
| Read-heavy public API | ✅ Redis | ✅ Enabled | Redis |
| Real-time data | ✅ Memory | ❌ Disabled | Memory |
| User-specific queries | ✅ PostgreSQL | ⚠️ With isolation | PostgreSQL |

### Key Takeaways

1. **Always use query caching** - no downside, pure performance gain
2. **Response caching is powerful but selective** - only for appropriate workloads
3. **Monitor hit rates continuously** - <70% indicates optimization opportunity
4. **Choose backend based on deployment** - memory for single, PostgreSQL/Redis for distributed
5. **Combine with materialized views** - FraiseQL's two-layer caching strategy is ideal

---

## Further Reading

- [FraiseQL Performance Guide](./index.md)
- [Caching Guide](./caching.md)
- [GraphQL APQ Specification](https://www.apollographql.com/docs/react/api/link/persisted-queries/)

---

*Last Updated: 2025-10-23 | FraiseQL v1.0.0*
