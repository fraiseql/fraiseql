# Request Lifecycle: What Happens Under the Hood

This document explains how FraiseQL processes a federated entity resolution request from start to finish.

## Complete Request Lifecycle

### Phase 1: Request Setup (t=0ms)

```
HTTP POST /graphql
{
  "query": "query { _entities(representations: [...]) { ... } }"
}
  ↓
GraphQL server receives request
  ↓
BatchExecutor.execute_request() called
  ↓
New EntityDataLoader created (request-scoped)
  ↓
DataLoader.__init__:
  - Empty dedup cache
  - Empty result cache
  - Empty pending batch
  - Batch window timer: NOT STARTED (lazy start)
  ↓
Ready for resolvers
```

**Key invariant**: Each HTTP request gets its own executor and loader. No cross-request pollution.

### Phase 2: Entity Resolution (t=0-1ms)

Resolvers start calling the loader:

```
Resolver for _entities[0] (User, id="1"):
  loader.load("User", "id", "1")
  ├─ Check result cache → MISS
  ├─ Check dedup cache → MISS
  ├─ Create Future A
  ├─ Add (User, id, 1) → [Future A] to pending
  ├─ Schedule batch flush in 1ms (BATCH WINDOW STARTS)
  └─ Return Future A (not yet resolved)

Resolver for _entities[1] (User, id="2"):
  loader.load("User", "id", "2")
  ├─ Check result cache → MISS
  ├─ Check dedup cache → MISS
  ├─ Create Future B
  ├─ Add (User, id, 2) → [Future B] to pending
  ├─ Batch window already scheduled, do nothing
  └─ Return Future B (not yet resolved)

Resolver for _entities[2] (User, id="1"):  ← DUPLICATE REQUEST
  loader.load("User", "id", "1")
  ├─ Check result cache → MISS
  ├─ Check dedup cache → HIT (Future A exists)
  ├─ DataLoader stats: dedup_hits += 1
  └─ Return Future A (dedup reuse)

[More resolvers...]

Resolver for _entities[50] (Order, id="42"):
  loader.load("Order", "id", "42")
  ├─ Check result cache → MISS
  ├─ Check dedup cache → MISS
  ├─ Create Future C
  ├─ Add (Order, id, 42) → [Future C] to pending
  ├─ Batch window already running
  └─ Return Future C (not yet resolved)

Pending batch state:
  {
    ("User", "id", 1) → [Future A],
    ("User", "id", 2) → [Future B],
    ("Order", "id", 42) → [Future C],
  }

Pending batch count: 3 (2 unique Users + 1 Order)
Dedup hits this phase: 48 (all the duplicate User 1 requests)
```

**Timeline**:
- t=0.0ms: First load() call
- t=0.5ms: Fifth load() call
- t=1.0ms: BATCH WINDOW EXPIRES (scheduled at t=0.0ms + 1ms)

### Phase 3: Batch Flush (t=1ms)

When the batch window expires, `_flush_after_delay()` executes:

```
[Batch window timer fires]
  ↓
DataLoader.flush() called
  ↓
Group by (typename, key_field):
  User group: key_values=[1, 2]
  Order group: key_values=[42]
  ↓
For User group:
  SQL: SELECT * FROM tv_user WHERE id IN (1, 2)
  Result: [User{id:1, name: "Alice"}, User{id:2, name: "Bob"}]
    ↓
    For each result:
      Cache in result_cache[(User, id, 1)] = User{1}
      Resolve Future A → User{1}
      Cache in result_cache[(User, id, 2)] = User{2}
      Resolve Future B → User{2}
    ↓
    For missing keys (none here):
      (Would cache None if User 3 was requested but not found)
  ↓
  Execution stats:
    - batch_count += 1
    - Results cached for both users

For Order group:
  SQL: SELECT * FROM tv_order WHERE id IN (42)
  Result: [Order{id:42, user_id: 1, total: 123.45}]
    ↓
    Cache result_cache[(Order, id, 42)] = Order{42}
    Resolve Future C → Order{42}
    ↓
    Execution stats:
      - batch_count += 1

Enforce cache size limit:
  Current cache size: 3 entries
  Max cache size: 1000 (default)
  → No eviction needed

All futures resolved ✓
Pending batch cleared
Batch window timer stopped
```

**Database queries executed**: 2 (one per entity type)
**Database roundtrips**: 1 (concurrent execution)
**vs. without batching**: 50+ queries (one per resolver)

### Phase 4: Resolver Completion (t=1-10ms)

```
GraphQL resolvers continue:
  user = await Future A  → User{1, name: "Alice"} (already resolved)
  order = await Future C → Order{42, ...} (already resolved)

Return to GraphQL executor:
  {
    "entities": [
      { "__typename": "User", "id": "1", "name": "Alice", ... },
      { "__typename": "User", "id": "2", "name": "Bob", ... },
      { "__typename": "Order", "id": "42", "total": 123.45, ... }
    ]
  }
```

### Phase 5: Request Teardown (t=10ms)

```
HTTP response ready
  ↓
Request context exits
  ↓
DataLoader.close() called:
  - Cancel any pending batch timer (if still running)
  - Flush one final time (in case of early exit)
  - Collect final statistics
  ↓
BatchExecutor destroyed
  ↓
HTTP response sent
  ↓
All caches discarded (request-scoped)
```

**Final statistics**:
```
DataLoaderStats:
  - total_requests: 51
  - cache_hits: 0 (first request, no prior cache)
  - cache_misses: 3
  - dedup_hits: 48
  - batch_count: 2
  - cache_hit_rate: 0.0
  - dedup_rate: 0.941 (48 out of 51)
```

## Performance Implications

### Batching Effect

**Without batching** (naive):
```
for entity in entities:
    SELECT * FROM {entity.table} WHERE id = {entity.id}
# 51 sequential queries × ~1ms = 51ms
```

**With batching** (FraiseQL):
```
SELECT * FROM users WHERE id IN (1, 2)
SELECT * FROM orders WHERE id IN (42)
# 2 queries × ~1ms = 2ms (concurrent)
+ 1ms batch window wait
# Total: ~3ms (50x faster for this workload)
```

### Deduplication Effect

Without deduplication:
```
load("User", "id", "1")  # Future A
load("User", "id", "1")  # Future B (new, redundant)
load("User", "id", "1")  # Future C (new, redundant)
# 3 futures, all wait for same query result
```

With deduplication:
```
load("User", "id", "1")  # Future A created
load("User", "id", "1")  # Reuse Future A
load("User", "id", "1")  # Reuse Future A
# 1 future, 3 resolvers depend on it
# Avoids triple-resolution if result changed
```

### Caching Effect

Within a request:
```
load("User", "id", "1")  # Database query
load("User", "id", "1")  # Cache hit (after first batch flushes)
# Second load returns immediately, no query
```

Across requests:
```
Request 1: load("User", "id", "1")  # DB query
Request 2: load("User", "id", "1")  # DB query again (different cache)
# Caches are request-scoped, no cross-request hits
```

## Error Handling During Lifecycle

### Case 1: Database Query Fails

```
DataLoader.flush() called
  ↓
SQL query fails:
  SELECT * FROM users WHERE id IN (1, 2)
  ↗ PostgresException: connection refused
  ↓
Exception caught in try/except
  ↓
For all pending futures:
  Future A.set_exception(PostgresException)
  Future B.set_exception(PostgresException)
  ↓
Resolvers receive exception when awaiting futures
  ↓
GraphQL error: "PostgreSQL error: connection refused"
  ↓
Request fails (partial results not returned)
```

### Case 2: Entity Not Found

```
SQL query succeeds:
  SELECT * FROM users WHERE id IN (1, 2, 3)
  Result: [User{1}, User{2}]
  # User{3} was not found
  ↓
After processing results:
  Found keys: {1, 2}
  Requested keys: {1, 2, 3}
  Missing keys: {3}
  ↓
For each missing key:
  result_cache[(User, id, 3)] = None
  Future[3].set_result(None)
  ↓
Resolver receives None:
  user = await Future[3]  # None
  # Handle gracefully or error
```

### Case 3: Early Exit / Timeout

```
HTTP client disconnects before response sent
  ↓
Request context exits early
  ↓
DataLoader.close() called
  ↓
Pending batch timer might still be running
  ├─ Batch timer.cancel()
  └─ Attempt final flush
  ↓
Unresolved futures: ignored (client already gone)
  ↓
Graceful cleanup
```

## Customization Points

### Batch Window Size

```python
# Default: 1ms (balance latency vs batching)
loader = EntityDataLoader(
    resolver,
    db_pool,
    batch_window_ms=1.0
)

# Aggressive batching (higher latency, fewer queries)
loader = EntityDataLoader(..., batch_window_ms=100.0)

# Real-time (lower batching, more queries)
loader = EntityDataLoader(..., batch_window_ms=0.0)  # Flush immediately
```

### Cache Size

```python
# Default: 1000 entries (reasonable for most services)
loader = EntityDataLoader(
    resolver,
    db_pool,
    cache_size=1000
)

# Large API
loader = EntityDataLoader(..., cache_size=100000)

# Memory-constrained
loader = EntityDataLoader(..., cache_size=100)
```

### Manual Flush

```python
# Default: automatic flush on batch window expiry
# Manual: force flush immediately
await loader.flush()
# Useful for:
# - Testing
# - Eager resolution before request ends
# - Checkpoint in long-running operation
```

## Summary

| Phase | Timing | Operations | Futures |
|-------|--------|-----------|---------|
| Setup | 0ms | Create executor, loader | None |
| Resolution | 0-1ms | load() calls, schedule batch | Pending |
| Flush | 1ms | Group queries, execute batch | Resolve |
| Completion | 1-10ms | Use resolved results | All done |
| Teardown | 10ms | Cleanup, discard caches | None |

**Key principle**: Everything is request-scoped. Each request is independent, with no cross-request state pollution.
