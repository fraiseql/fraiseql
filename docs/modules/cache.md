# Cache Module

**Source files**:
- `crates/fraiseql-core/src/cache/adapter.rs` (~1,836 lines) — `CachedDatabaseAdapter`, the main entry point
- `crates/fraiseql-core/src/cache/result.rs` (~1,177 lines) — `QueryResultCache` (64-shard LRU) and `CachedResult`
- `crates/fraiseql-core/src/cache/key.rs` (~632 lines) — cache key construction and security model
- `crates/fraiseql-core/src/cache/cascade_invalidator.rs` — view dependency graph and cascade invalidation
- Supporting: `invalidation_api.rs`, `fact_table_cache.rs`, `relay_cache.rs`, `config.rs`

**Tests**: Unit tests in `#[cfg(test)] mod tests` inside each source file. Run with:
```bash
cargo nextest run -p fraiseql-core --lib cache
```

---

## Overview

The cache module wraps `DatabaseAdapter` in a `CachedDatabaseAdapter` that stores query
results in an in-process LRU cache. The design has three critical properties:

1. **RLS isolation** — the cache key includes the user's Row-Level Security WHERE clause,
   ensuring that two users with different RLS policies never share cached data.
2. **View-based invalidation** — invalidation is coarse-grained (per-view, not per-row),
   which is fast but causes over-invalidation on write-heavy workloads.
3. **64-shard architecture** — a single `Arc<Mutex<LruCache>>` is a contention bottleneck
   under high concurrency; sharding eliminates the hot lock.

---

## Sharding Scheme

```
Cache key (string)
      ↓ SHA-256
[byte_0, byte_1, ..., byte_31]
      ↓ byte_0 % 64
Shard index (0..63)
      ↓
LruCache<String, CachedResult>  ← each shard has its own Mutex
```

**Why 64 shards?** Powers of 2 are standard for shard counts; 64 gives a good balance
between contention reduction and memory overhead. Under 64 concurrent writers, the expected
lock wait approaches zero.

**Why first byte of SHA-256?** SHA-256 output is uniform — the first byte distributes
evenly across 0–255, so `byte % 64` gives a near-uniform shard distribution regardless
of query content.

---

## Cache Key Construction

The cache key is `SHA-256(hash_query_with_variables + serde_json_where + schema_version)`.

### Why each component matters

| Component | Reason |
|-----------|--------|
| `hash_query_with_variables(query, vars)` | Different variables must produce different keys; APQ hasher normalizes variable ordering |
| `serde_json::to_string(WHERE clause)` | RLS produces per-user WHERE clauses; different users must hit different cache entries |
| Schema version hash | Invalidates all entries on deployment when schema changes |

### RLS security model (critical)

Without the RLS WHERE clause in the key, user A's cached result could be served to user B.
This would be a data breach. The cache key is the security boundary.

The APQ module normalizes variables before hashing, so:
- `{a: 1, b: 2}` and `{b: 2, a: 1}` produce the **same** key ✅
- `{a: 1}` and `{a: 2}` produce **different** keys ✅
- User A with `WHERE tenant_id = 1` and User B with `WHERE tenant_id = 2` produce **different** keys ✅

`serde_json::to_string` is used (not `{:?}` Debug format) to ensure key stability when
internal Rust types are refactored.

---

## Per-Entry TTL

```
QueryDefinition.cache_ttl_seconds: Option<u64>   (from compiled schema)
      ↓ (loaded at server startup)
CachedDatabaseAdapter.view_ttl_overrides: HashMap<ViewName, u64>
      ↓ (applied per put() call)
CachedResult { data, views_depended_on, cached_at, ttl_seconds }
```

**TTL = 0 means: never store this entry.** `put()` is a no-op when `ttl_override = Some(0)`.
This is how live-data queries (prices, stock levels) opt out of caching entirely.

**TTL expiry is checked on read** (`get()`), not lazily. Expired entries are evicted
immediately rather than accumulating:
```rust
if now - cached.cached_at > cached.ttl_seconds {
    cache.pop(key);  // evict
    return Ok(None); // miss
}
```

---

## Cascade Invalidation

```
mutation executes
      ↓ returns affected_view_names = ["v_user"]
InvalidationContext::for_mutation("createUser", affected_views)
      ↓ (if CascadeInvalidator is configured)
CascadeInvalidator::cascade_invalidate("v_user")
      ↓ BFS over dependency graph
      → expands to ["v_user", "v_user_stats", "v_dashboard"]
cache.invalidate_views(["v_user", "v_user_stats", "v_dashboard"])
      ↓ scans all entries
      → removes any entry where accessed_views ∩ expanded_views ≠ ∅
```

**Dependency graph declaration:**
```rust
let mut cascade = CascadeInvalidator::new();
cascade.add_dependency("v_user_stats", "v_user")?;   // v_user_stats reads v_user
cascade.add_dependency("v_dashboard", "v_user_stats")?;
```

The dependency graph is built at server startup and is read-only during request handling.
Only the `InvalidationStats` counter is mutable (protected by its own `Mutex`).

**Tradeoff**: View-level invalidation is coarser than row-level. Writing a single user
invalidates all cached queries that read `v_user`, even if those queries don't involve
the modified user. On write-heavy workloads, cache effectiveness drops. Design
your view dependency graph conservatively.

---

## Operational Notes

**Memory estimation**:
```
64 shards × LRU capacity × average entry size
Example: 64 × 1000 × 10KB = ~640MB
```

**Metrics** exposed on `/metrics`:
```
cache_hits_total
cache_misses_total
cache_size_current
cache_invalidations_total
cache_memory_bytes_estimated
```

**Manual invalidation** via HTTP:
```http
POST /cache/invalidate?view=v_user
```

**Disabling caching** for a specific query — set `cache_ttl_seconds = 0` in the schema:
```python
@fraiseql.query(sql_source="v_live_prices", cache_ttl_seconds=0)
def live_prices() -> list[Price]:
    ...
```
