# Performance Tuning: Batch Windows, Caching & Detection

This guide helps you optimize FraiseQL federation for your workload.

## Quick Start: Default Settings

FraiseQL comes configured for **balanced performance**:

```python
EntityDataLoader(
    resolver,
    db_pool,
    cache_size=1000,           # ← LRU cache (per-request)
    batch_window_ms=1.0        # ← 1ms batch window
)
```

**These defaults work for 80% of use cases.** Only tune if you measure a problem.

---

## Parameter 1: Batch Window (`batch_window_ms`)

The **batch window** is how long FraiseQL waits for more requests before flushing the batch.

### How It Works

```
Timeline:
t=0.0ms:  First load() call
t=0.5ms:  Second load() call
t=1.0ms:  [FLUSH] Batch window expires
          Execute query with all requests
```

### Tuning Guide

| Batch Window | Latency Impact | Batching Efficiency | Use Case |
|---|---|---|---|
| **0ms** | Lowest (~0ms) | Worst (~1-5 per query) | Real-time APIs, monitoring |
| **0.1ms** | Very low (~0.1ms) | Poor (~5-20 per query) | Interactive queries |
| **1.0ms** | Low (~1ms) | Good (~50-200 per query) | **Default, recommended** |
| **5.0ms** | Medium (~5ms) | Very good (~200-500 per query) | Background jobs |
| **50ms** | High (~50ms) | Excellent (~500-2000 per query) | Batch processing |
| **100ms** | Very high (~100ms) | Best (~2000+ per query) | Large bulk operations |

### Decision Tree

**Start here**:

```
Is this interactive (web/mobile)?
├─ YES → Use 0.1-1.0ms
└─ NO → Continue

Is user waiting for response?
├─ YES → Use 0.5-2.0ms (faster is better)
└─ NO → Continue

Is this background processing?
├─ YES → Use 5-100ms (more batching is better)
└─ NO → Use 1.0ms (safe default)
```

### Real-World Examples

#### Example 1: Web API (Interactive)

```python
executor = PerRequestBatchExecutor(batch_window_ms=1.0)  # Default

# Frontend makes query:
# → Request arrives
# → Resolves 20 entities
# → All batched in 1ms
# → Total latency: 2-3ms (including network, DB, etc.)
# → User gets response quickly
```

**Metrics**:
- Batch size: 20 entities
- DB queries: 2 (one per entity type)
- Request latency: 2-3ms
- User satisfaction: High (responsive)

#### Example 2: Mobile API (Low Latency)

```python
executor = PerRequestBatchExecutor(batch_window_ms=0.1)  # Aggressive

# Mobile request (slow network, battery sensitive)
# → Request arrives
# → Resolves 5 entities
# → Flushed quickly in 0.1ms
# → Total latency: 1-2ms (less waiting, fewer entities per batch)
# → Battery preserved

# Trade-off: More database queries, but lower latency
```

**Metrics**:
- Batch size: 5 entities
- DB queries: 2 (but smaller batches)
- Request latency: 1-2ms
- Battery: Better (less processing)

#### Example 3: Background Job (Throughput)

```python
executor = PerRequestBatchExecutor(batch_window_ms=50.0)  # Aggressive batching

# Processing 10,000 users in background
# → Bulk request arrives
# → Resolves 1000 entities per batch
# → Batched over 50ms window
# → Total latency: 50-100ms (doesn't matter, no user waiting)
# → DB throughput maximized

# Result: Process all 10,000 in 2-3 requests instead of 100+
```

**Metrics**:
- Batch size: 1000 entities
- DB queries: 2 (massive batching)
- Request latency: 50-100ms (acceptable for background job)
- Throughput: 10,000 entities processed efficiently

---

## Parameter 2: Cache Size (`cache_size`)

The **cache** stores entity results within a request. **Important**: Cache is **request-scoped**, not across requests.

### How Caching Works

```
Request 1:
  load("User", "id", "1")     → DB query
  load("User", "id", "1")     → Cache hit (same request)

Request 2:
  load("User", "id", "1")     → DB query again (new request, fresh cache)
  load("User", "id", "1")     → Cache hit (same request)
```

**Cache is NOT persistent across requests.**

### Cache Size Recommendations

| Cache Size | Max Entities Per Request | Use Case |
|---|---|---|
| **100** | 50-100 | Memory-constrained, simple queries |
| **1,000** | 200-1,000 | **Default, recommended** |
| **10,000** | 2,000-10,000 | Complex queries, many relations |
| **100,000** | 20,000+ | Large bulk operations |

### Decision Guide

```python
# Small API / Memory constraint
loader = EntityDataLoader(resolver, db_pool, cache_size=100)

# Typical API (recommended)
loader = EntityDataLoader(resolver, db_pool, cache_size=1000)

# Large API / Complex queries
loader = EntityDataLoader(resolver, db_pool, cache_size=10000)

# Very large API / Bulk processing
loader = EntityDataLoader(resolver, db_pool, cache_size=100000)
```

### When Caching Helps (Within Request)

```python
async def handle_request(loader):
    results = []

    # User ID appears multiple times
    user = await loader.load("User", "id", "user-1")      # Cache miss
    results.append(user)

    # Later in same request
    user = await loader.load("User", "id", "user-1")      # Cache hit!
    results.append(user)

    # Load 5 more times
    for _ in range(5):
        user = await loader.load("User", "id", "user-1")  # All cache hits

    return results

# Stats: 1 DB query for user-1, 7 resolver calls satisfied
```

### Cache Eviction (LRU)

When cache exceeds size limit, oldest entries are evicted:

```python
loader = EntityDataLoader(resolver, db_pool, cache_size=1000)

# Load 1000 entities (cache full)
for i in range(1000):
    await loader.load("User", "id", f"user-{i}")

# Load one more
await loader.load("User", "id", "user-1001")
# → Evicts user-1 (oldest)
# → Adds user-1001

# If we try to load user-1 again
user = await loader.load("User", "id", "user-1")  # Cache miss (was evicted)
```

---

## Parameter 3: Detecting Over/Under-Batching

Use `DataLoaderStats` to measure what's actually happening:

### Statistics Available

```python
stats = loader.stats

# Total requests to loader
print(f"Total requests: {stats.total_requests}")

# Cache hits (same entity, same request)
print(f"Cache hits: {stats.cache_hits}")

# Dedup hits (duplicate request within batch)
print(f"Dedup hits: {stats.dedup_hits}")

# Actual DB queries
print(f"Batch count: {stats.batch_count}")

# Calculated rates
print(f"Cache hit rate: {stats.cache_hit_rate:.1%}")
print(f"Dedup rate: {stats.dedup_rate:.1%}")
```

### Interpreting Stats

#### Good Performance (What to Aim For)

```python
stats:
  total_requests=100
  cache_hits=0
  dedup_hits=75        # ← 75% dedup rate (good!)
  batch_count=2        # ← Only 2 DB queries!
  dedup_rate=0.75
```

**Interpretation**: Out of 100 entity requests:
- 25 unique entities (cache miss + dedup miss)
- 75 duplicate requests (dedup hits)
- Only 2 database queries (batching worked!)
- **Result**: 50x improvement over no batching

#### Under-Batching (Too Many DB Queries)

```python
stats:
  total_requests=100
  cache_hits=0
  dedup_hits=0         # ← No dedup (bad!)
  batch_count=100      # ← 100 DB queries (terrible!)
  dedup_rate=0.0
```

**What's wrong**: No deduplication, queries not batching.

**Causes**:
1. Batch window too short (0ms)
2. Single entity per request (can't batch)
3. Different entity types (batch per type)

**Fix**:
```python
# Increase batch window
executor = PerRequestBatchExecutor(batch_window_ms=5.0)

# Or ensure resolvers load concurrently
async def resolve_entities(info, representations):
    db_pool = info.context.get("db_pool")

    async def handle_request(loader):
        # Load ALL entities before awaiting
        futures = [
            loader.load("User", "id", rep["id"])
            for rep in representations
        ]
        # Now batch window has time to collect all requests
        results = await asyncio.gather(*futures)
        return results

    return await executor.execute_request(handle_request, resolver, db_pool)
```

#### Over-Batching (High Latency)

```python
stats:
  total_requests=10
  cache_hits=0
  dedup_hits=8         # ← High dedup
  batch_count=2        # ← Good batching
  # But response takes 100ms (batch window)
```

**What's wrong**: Waiting too long for more requests.

**Cause**: Batch window set to 100ms, but requests are sparse (only 10 entities).

**Fix**:
```python
# Reduce batch window
executor = PerRequestBatchExecutor(batch_window_ms=1.0)
# Requests now resolve in ~1ms instead of 100ms
```

---

## Measuring & Monitoring

### Log Stats After Each Request

```python
async def resolve_entities(info, representations):
    db_pool = info.context.get("db_pool")

    async def handle_request(loader):
        results = []
        for rep in representations:
            entity = await loader.load(rep["__typename"], "id", rep["id"])
            results.append(entity)

        # Log performance stats
        stats = loader.stats
        logger.info(
            f"Federation stats - requests:{stats.total_requests} "
            f"dedup_rate:{stats.dedup_rate:.1%} "
            f"queries:{stats.batch_count} "
            f"cache_hit_rate:{stats.cache_hit_rate:.1%}"
        )

        return results

    return await executor.execute_request(handle_request, resolver, db_pool)
```

### Monitor in Production

```python
# Send stats to monitoring system
import time

async def monitored_resolve(info, representations):
    start_time = time.time()

    result = await resolve_entities(info, representations)

    elapsed_ms = (time.time() - start_time) * 1000

    # Send to monitoring
    metrics.timing("federation.request_latency_ms", elapsed_ms)
    metrics.gauge("federation.batch_count", loader.stats.batch_count)
    metrics.gauge("federation.dedup_rate", loader.stats.dedup_rate)

    return result
```

### Alerts to Set Up

```python
# Alert if dedup rate drops below 50% (under-batching)
if loader.stats.dedup_rate < 0.5:
    logger.warning("Low dedup rate - consider increasing batch window")

# Alert if batch count exceeds threshold
if loader.stats.batch_count > 10:
    logger.warning(f"High batch count ({loader.stats.batch_count}) - queries may be slow")

# Alert if latency exceeds threshold
if elapsed_ms > 100:
    logger.warning(f"Slow federation request ({elapsed_ms}ms)")
```

---

## Real-World Performance Profiles

### Profile 1: E-Commerce Product Catalog

```
Typical query:
  - 50 products
  - Each has: price, reviews, seller (from 3 services)

Without federation:
  - 1 query: products
  - 50 queries: reviews (one per product) ← N+1
  - 50 queries: sellers (one per product) ← N+1
  - Total: 101 queries, 50-100ms latency

With FraiseQL federation:
  - Service 1: SELECT * FROM products WHERE id IN (...)  ← 1 query
  - Service 2: SELECT * FROM reviews WHERE product_id IN (...)  ← 1 query
  - Service 3: SELECT * FROM sellers WHERE id IN (...)  ← 1 query
  - Total: 3 queries, 3-5ms latency

Result: 30x fewer queries, 15x faster
Stats:
  - dedup_rate: ~95% (products appear in multiple reviews)
  - batch_count: 3 (one per service)
  - cache_hit_rate: ~30% (products loaded multiple times)
```

### Profile 2: Social Media Feed

```
Typical query:
  - 20 posts
  - Each has: author, comments (5-20), likes (10-100)

With FraiseQL:
  - Posts: 1 query
  - Authors: 1 query (10-20 unique)
  - Comments: 1 query (100-400 comments)
  - Likes: 1 query (200-2000 user IDs)

Stats:
  - dedup_rate: ~98% (author appears in many posts)
  - batch_count: 4
  - cache_hit_rate: ~10% (comments rarely repeated)
```

### Profile 3: Admin Dashboard (Bulk)

```
Typical query:
  - 1000 users
  - Each has: roles, permissions, last_login

Batch window: 50ms (background, not real-time)

With FraiseQL:
  - Users: 1 query (1000 users, but batched in one go)
  - Roles: 1 query (batched assignment check)
  - Permissions: 1 query (batched permission check)

Stats:
  - dedup_rate: ~99% (same users, same roles)
  - batch_count: 3
  - cache_hit_rate: ~85% (1000 users, roles repeated many times)
```

---

## Optimization Checklist

- [ ] **Measure baseline**: Log stats for 24 hours to understand typical workload
- [ ] **Check dedup rate**: Should be >50%, aim for >70%
  - If low: increase batch window
- [ ] **Check batch count**: Should match number of entity types
  - If high: investigate why batches aren't grouping
- [ ] **Check cache hit rate**: Depends on workload
  - Low rate (0-20%): normal for most queries
  - High rate (>50%): indicates repeated entities within requests
- [ ] **Monitor latency**: P95 should be <50ms for interactive
  - If high: reduce batch window or check database performance
- [ ] **Set alerts**: For dedup_rate<50%, batch_count>5, latency>100ms
- [ ] **Document config**: Record final settings and reasoning

---

## Trade-Off Matrix

| Decision | Latency Impact | Throughput | Complexity | When to Choose |
|---|---|---|---|---|
| **Batch window: 1ms** | Low (~1ms) | Good (~100/sec) | Simple | Default, web APIs |
| **Batch window: 100ms** | High (~100ms) | Excellent (~1000/sec) | Simple | Background jobs |
| **Cache size: 1000** | None | Neutral | Simple | Default |
| **Cache size: 100000** | Minimal | Slight ↑ | Simple | Complex queries |
| **Custom executor** | Variable | Variable | Complex | Advanced only |

---

## Summary

**For 80% of users**:
- Default batch window (1.0ms) is optimal
- Default cache size (1000) is sufficient
- Monitor dedup rate (should be >50%)

**For 15% of users**:
- Tune batch window based on workload (0.1-50ms)
- Increase cache size for complex queries (10,000+)
- Monitor latency and throughput

**For 5% of users**:
- Custom BatchExecutor for tenant-aware or grouped batching
- Advanced metrics and observability

**Golden rule**: Measure first, tune second. Don't optimize what you haven't measured.
