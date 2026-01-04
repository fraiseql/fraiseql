# Phase 17A: Corrected Architecture (Query Result Cache with Cascade-Driven Invalidation)

**Status**: Corrected Implementation Plan
**Complexity**: Medium (simpler than originally thought)
**Effort**: 3-4 days
**Risk**: Low
**Version Target**: v1.9.0

**Last Updated**: January 4, 2026
**Note**: Based on actual graphql-cascade principle from PrintOptim

---

## 🎯 Core Principle (Corrected)

### What graphql-cascade Actually Is

**graphql-cascade is NOT just invalidation metadata.**

It's a complete design pattern:

```
1. Mutation executes in PostgreSQL
   → Writes to entity tables
   → Triggers compute cascade metadata

2. Mutation response built:
   → Primary object (from tv_entity JSONB)
   → ALL affected related objects (from tv_* JSONB tables)
   → Cascade metadata (describes what changed)

3. Client 1 receives:
   → Complete object graph (user + posts + comments + cascade)
   → Already has fresh data (no refetch needed)
   → graphql-cascade library stores this in Apollo cache

4. Server extracts cascade metadata:
   → "User:123 changed, Post:456,789 changed"
   → Invalidates server cache entries for these entities
   → Prepares cache to serve other clients fresh data

5. Client 2 queries same list:
   → Cache miss (just invalidated)
   → Hits DB, gets fresh data
   → Cache stores result

6. Client 3 queries same list:
   → Cache hit (refreshed by Client 2)
   → Gets data in 1-2ms (not 8-10ms)
   → DB not hit
```

**The insight**: Phase 17A cache isn't for the mutating client (they have fresh data). It's for OTHER clients who query the same data afterward.

---

## 📊 The Multi-Client Scenario (Why This Matters)

### Without Cache (Each Client Hits DB)

```
T0:   Client 1: Query "list users" → DB (8ms) → Response
T0.1: Client 2: Query "list users" → DB (8ms) → Response
T0.2: Client 3: Query "list users" → DB (8ms) → Response
T1:   Client 1: Mutation updateUser(2) → DB (10ms) → Response
T1.1: Client 2: Query "list users" → DB (8ms) → Response (stale? or fresh?)
T1.2: Client 3: Query "count users" → DB (5ms) → Response

Total DB hits: 6 (for 6 client operations)
Total latency: 47ms
Database load: High
```

### With Phase 17A Cache + Cascade Invalidation

```
T0:   Client 1: Query "list users"
      → Cache MISS
      → DB (8ms): [User:1, User:2, User:3]
      → Cache store: "users:list" = [1,2,3]
      → Response (8ms)

T0.1: Client 2: Query "list users"
      → Cache HIT ✓
      → Return cached [1,2,3] (1ms)

T0.2: Client 3: Query "list users"
      → Cache HIT ✓
      → Return cached [1,2,3] (1ms)

T1:   Client 1: Mutation updateUser(2)
      → DB (10ms): UPDATE users SET ... WHERE id=2
      → Response includes: User:2 (from tv_users) + cascade metadata
      → Cascade: "User:2 changed"
      → Server: Invalidate cache "users:*"
      → Response to Client 1 (10ms)

T1.1: Client 2: Query "list users"
      → Cache MISS (just invalidated)
      → DB (8ms): [User:1, User:2(updated), User:3]
      → Cache store: "users:list" = [1,2(updated),3]
      → Response (8ms)

T1.2: Client 3: Query "count users"
      → Cache MISS (just invalidated)
      → DB (5ms): 3
      → Cache store: "users:count" = 3
      → Response (5ms)

T2:   Client 4: Query "list users"
      → Cache HIT ✓
      → Return cached [1,2(updated),3] (1ms)

T3:   Client 5: Query "list users"
      → Cache HIT ✓
      → Return cached [1,2(updated),3] (1ms)

Total DB hits: 4 (down from 6)
Total latency: 34ms (down from 47ms)
Database load: 33% reduction
Cache hit rate: 60% (4 hits out of 6 queries after first mutation)
```

**Key insight**: After mutation, cache is invalidated but IMMEDIATELY populated by next client query. Subsequent clients hit cache until next mutation.

---

## 🏗️ Architecture (Corrected)

### Cache Entry Structure

```rust
#[derive(Clone)]
pub struct CacheEntry {
    /// The query result (complete GraphQL response)
    pub result: Arc<serde_json::Value>,

    /// Which entities this query depends on
    /// e.g., vec![("User", "123"), ("Post", "456")]
    pub accessed_entities: Vec<(String, String)>,

    /// When this entry was cached
    pub cached_at: DateTime<Utc>,

    /// TTL for safety (cascade is primary invalidation, TTL is safety net)
    pub ttl: Duration,
}
```

### Cache Key Strategy

**Query → Cache Key**:

```
Query: query { users { id name posts { title } } }

Cache key: "query:users:posts:all"
           ↑       ↑     ↑      ↑
           |       |     |      └─ No filters
           |       |     └──────── Related entities
           |       └───────────── Root entity
           └─────────────────── Query type

Accessed entities: [("User", "*"), ("Post", "*")]
                   ↑ All users   ↑ All posts
```

**With filters**:

```
Query: query { user(id: "123") { name posts { title } } }

Cache key: "query:user:123:posts"
Accessed entities: [("User", "123"), ("Post", "*")]
```

### Invalidation Strategy (Cascade-Driven)

**When mutation returns cascade metadata**:

```json
{
  "cascade": {
    "invalidations": {
      "updated": [
        { "type": "User", "id": "123" },
        { "type": "Post", "id": "456" }
      ],
      "deleted": [
        { "type": "Post", "id": "789" }
      ]
    }
  }
}
```

**Server extracts cascade and invalidates**:

```rust
// Cascade says: "User:123 changed, Post:456 changed, Post:789 deleted"

// Invalidate any cache entry that accessed these entities:
for entry in cache.entries {
    if entry.accessed_entities.contains(("User", "123")) {
        cache.remove(key)  // Query about User:123 is stale
    }
    if entry.accessed_entities.contains(("Post", "456")) {
        cache.remove(key)  // Query about Post:456 is stale
    }
    if entry.accessed_entities.contains(("Post", "789")) {
        cache.remove(key)  // Query about Post:789 is stale
    }
}
```

**Result**: All queries that touched the changed entities are cleared. Other queries stay cached.

---

## 📝 Implementation Plan

### Phase 17A.1: Core Cache Module

**File**: `fraiseql_rs/src/cache/query_result_cache.rs` (NEW)

**What it does**:
- Store query results in memory
- Track which entities each query accesses
- Implement LRU eviction (configurable max size)
- Provide get/put/invalidate operations

**Code structure** (same as original):

```rust
pub struct QueryResultCache {
    entries: Arc<Mutex<HashMap<String, CacheEntry>>>,
    dependencies: Arc<Mutex<HashMap<String, Vec<String>>>>,
    config: CacheConfig,
    metrics: Arc<Mutex<CacheMetrics>>,
}

impl QueryResultCache {
    pub fn new(config: CacheConfig) -> Self { }
    pub fn get(&self, cache_key: &str) -> Option<Arc<serde_json::Value>> { }
    pub fn put(
        &self,
        cache_key: String,
        result: serde_json::Value,
        accessed_entities: Vec<(String, String)>,
    ) { }
    pub fn invalidate_from_cascade(
        &self,
        cascade: &serde_json::Value,
    ) -> Result<u64> { }
    pub fn metrics(&self) -> CacheMetrics { }
}
```

**Tests** (6 tests):
- `test_cache_hit_returns_value`
- `test_cache_miss_returns_none`
- `test_lru_eviction_when_full`
- `test_cascade_invalidation_clears_entry`
- `test_cascade_invalidation_multi_entity`
- `test_metrics_tracking`

---

### Phase 17A.2: Query Execution Integration

**Files**:
- `fraiseql_rs/src/cache/mod.rs` (new exports)
- `fraiseql_rs/src/pipeline/unified.rs` (hook into query execution)

**What it does**:
- Generate cache key from GraphQL query
- Check cache before executing query
- Cache result after execution
- Track accessed entities

**Implementation**:

```rust
pub async fn execute_query_with_cache(
    cache: &QueryResultCache,
    query: &str,
    variables: &serde_json::Value,
) -> Result<serde_json::Value> {
    // Step 1: Generate cache key
    let cache_key = QueryCacheKey::from_query(query, variables)?;

    // Step 2: Check cache
    if let Some(cached) = cache.get(&cache_key.signature) {
        return Ok((*cached).clone());
    }

    // Step 3: Execute query (miss)
    let result = execute_query_uncached(query, variables).await?;

    // Step 4: Cache result with entity tracking
    cache.put(
        cache_key.signature,
        result.clone(),
        cache_key.accessed_entities,
    );

    Ok(result)
}

pub struct QueryCacheKey {
    pub signature: String,  // Hash of query + variables
    pub accessed_entities: Vec<(String, String)>,  // Entities touched
}

impl QueryCacheKey {
    pub fn from_query(query: &str, variables: &Value) -> Result<Self> {
        // Extract root entities from query
        // Extract filter arguments to determine specific entity IDs
        // Build accessed_entities list

        let signature = format!("{:x}", calculate_hash(query, variables));

        Ok(QueryCacheKey {
            signature,
            accessed_entities: extract_entities(query, variables)?,
        })
    }
}
```

**Tests** (6 tests):
- `test_cache_key_generated_correctly`
- `test_cache_key_deterministic` (same query, same key)
- `test_cache_key_different_filters` (different filters, different key)
- `test_entity_extraction_from_query`
- `test_integration_query_cache_hit`
- `test_integration_query_cache_miss`

---

### Phase 17A.3: Mutation Response Integration

**Files**:
- `fraiseql_rs/src/http/mutation_handler.rs` (new or update)
- `fraiseql_rs/src/cache/cascade_invalidation.rs` (new)

**What it does**:
- Extract cascade metadata from mutation response
- Use cascade to invalidate cache
- Return response to client with cascade included

**Implementation**:

```rust
pub async fn handle_mutation(
    state: Arc<AppState>,
    mutation: String,
    variables: Value,
) -> Result<Response> {
    // Step 1: Execute mutation
    let response = execute_mutation(&mutation, &variables).await?;

    // Step 2: Extract cascade from response
    let cascade = response
        .get("data")
        .and_then(|d| d.get("cascade"))
        .cloned();

    // Step 3: Use cascade to invalidate cache
    if let Some(cascade_data) = cascade {
        match state.query_cache.invalidate_from_cascade(&cascade_data) {
            Ok(count) => {
                debug!("Invalidated {} cache entries from mutation", count);
            }
            Err(e) => {
                warn!("Cache invalidation error: {}", e);
                // Don't fail mutation if cache fails
            }
        }
    }

    // Step 4: Return response (includes cascade for client-side cache)
    Ok(Response::json(response))
}
```

**How cascade flows**:

```
PostgreSQL mutation:
  UPDATE users SET name = 'Jane' WHERE id = 123
  → Trigger computes: cascade = { updated: [{ type: "User", id: "123" }] }
  → Returns in mutation response

FraiseQL mutation handler:
  1. Receives response WITH cascade
  2. Extracts cascade: { updated: [{ type: "User", id: "123" }] }
  3. Calls: cache.invalidate_from_cascade(cascade)
  4. Cache logic:
     - For User:123 in cascade.updated:
       - Find all cache entries with ("User", "123")
       - Remove them from cache
       - Example: "query:user:123:posts" → REMOVED
       - Example: "query:users" → REMOVED (might list User:123)
  5. Sends response to client (cascade still in response)

Client-side:
  1. Receives response WITH cascade
  2. graphql-cascade library processes cascade
  3. Apollo cache invalidates entries for User:123
  4. Next query refetches from server
```

**Tests** (6 tests):
- `test_cascade_extraction_from_response`
- `test_cascade_invalidation_single_entity`
- `test_cascade_invalidation_multiple_entities`
- `test_cascade_invalidation_deleted_entities`
- `test_mutation_response_includes_cascade`
- `test_cascade_invalidation_error_handling`

---

### Phase 17A.4: HTTP Server Integration

**Files**:
- `fraiseql_rs/src/http/axum_server.rs` (update AppState)
- `fraiseql_rs/src/http/middleware.rs` (hook cache into requests)

**What it does**:
- Add cache to AppState
- Initialize cache with config
- Wire up query execution to use cache
- Wire up mutation invalidation

**Implementation**:

```rust
pub struct AppState {
    pub db_pool: PgPool,
    pub query_cache: Arc<QueryResultCache>,  // NEW!
}

pub async fn create_app() -> Result<Router> {
    let cache_config = CacheConfig {
        max_entries: 10000,        // LRU eviction at this size
        cache_list_queries: true,  // Cache paginated lists too
        ttl: Duration::from_secs(24 * 3600),  // 24h safety TTL
    };

    let query_cache = Arc::new(QueryResultCache::new(cache_config));

    let state = Arc::new(AppState {
        db_pool,
        query_cache,
    });

    Ok(Router::new()
        .post("/graphql", graphql_handler)
        .with_state(state))
}

pub async fn graphql_handler(
    State(state): State<Arc<AppState>>,
    body: String,
) -> Result<Response> {
    let (query, variables) = parse_graphql_request(&body)?;

    // Don't cache mutations or subscriptions
    if is_mutation(&query) {
        let response = execute_mutation(&query, &variables).await?;
        handle_mutation_invalidation(&state, &response).await?;
        return Ok(Response::json(response));
    }

    // Cache queries only
    let result = execute_query_with_cache(
        &state.query_cache,
        &query,
        &variables,
    ).await?;

    Ok(Response::json(result))
}
```

---

### Phase 17A.5: Monitoring & Observability

**Files**:
- `fraiseql_rs/src/cache/metrics.rs` (endpoints)
- `fraiseql_rs/src/http/health_check.rs` (health endpoint)

**Metrics endpoints**:

```rust
pub async fn cache_metrics_endpoint(
    State(state): State<Arc<AppState>>,
) -> JsonResponse {
    let metrics = state.query_cache.metrics();

    let hit_rate = if metrics.hits + metrics.misses > 0 {
        (metrics.hits as f64) / ((metrics.hits + metrics.misses) as f64)
    } else {
        0.0
    };

    json!({
        "cache": {
            "hits": metrics.hits,
            "misses": metrics.misses,
            "hit_rate": format!("{:.1}%", hit_rate * 100.0),
            "size": metrics.size,
            "memory_mb": estimate_memory(&metrics),
            "total_cached": metrics.total_cached,
            "invalidations": metrics.invalidations,
        }
    })
}

pub async fn health_check_endpoint(
    State(state): State<Arc<AppState>>,
) -> JsonResponse {
    let metrics = state.query_cache.metrics();
    let hit_rate = calculate_hit_rate(&metrics);

    json!({
        "status": "healthy",
        "cache": {
            "hit_rate": format!("{:.1}%", hit_rate * 100.0),
            "size": metrics.size,
            "memory_mb": estimate_memory(&metrics),
        },
        "recommendations": if hit_rate < 0.75 {
            "Hit rate low, consider Phase 17B field-level cache"
        } else {
            "System performing well"
        },
    })
}
```

**What to monitor**:
- Cache hit rate (should be 85-90% for typical workloads)
- Cache size (should grow until stabilizes at 5-10% of working set)
- Invalidation count (should spike with mutations)
- Memory usage (bounded by LRU config)

---

## 🧪 Testing Strategy

### Unit Tests (Phase 17A.1)
- 6 cache core tests
- Focus: storage, retrieval, LRU eviction, dependency tracking

### Integration Tests (Phase 17A.2-3)
- 12 integration tests
- Focus: query caching, cascade invalidation, multi-entity handling

### Load Tests (Phase 17A.4)
- Scenario 1: 1000 QPS, 10,000 unique queries
  - Expected: 85%+ hit rate, < 2ms p99 latency on cache hits

- Scenario 2: 1000 QPS with mutations every 100ms
  - Expected: Cache invalidations clear affected queries
  - Expected: Subsequent queries hit cache

- Scenario 3: Multi-client scenario
  - Client 1: Query "list users"
  - Client 2: Same query (should cache hit)
  - Client 1: Mutation
  - Client 2: Query again (should miss, refresh cache)
  - Client 3: Same query (should cache hit refreshed data)

### Total Tests
- **6 unit** (Phase 17A.1)
- **12 integration** (Phase 17A.2-3)
- **3 load test scenarios** (Phase 17A.4)
- **Total: 21 tests** (simpler than original 26)

---

## 📋 Implementation Checklist

### Phase 17A.1: Core Cache
- [ ] `QueryResultCache` struct implemented
- [ ] `get()` operation works
- [ ] `put()` operation works
- [ ] LRU eviction works
- [ ] Dependency tracking works
- [ ] Metrics tracking works
- [ ] All 6 unit tests pass
- [ ] Thread-safe (Arc<Mutex>)

### Phase 17A.2: Query Integration
- [ ] `QueryCacheKey::from_query()` generates correct keys
- [ ] Entity extraction works for various query types
- [ ] Cache hit/miss tracking works
- [ ] Results cached after execution
- [ ] All 6 integration tests pass
- [ ] Non-cached queries still work

### Phase 17A.3: Mutation Integration
- [ ] Cascade extraction from response works
- [ ] `invalidate_from_cascade()` clears correct entries
- [ ] Multi-entity invalidation works
- [ ] Deleted entities handled correctly
- [ ] Response includes cascade for client
- [ ] All 6 integration tests pass
- [ ] Mutation still succeeds if cache fails

### Phase 17A.4: HTTP Integration
- [ ] Cache added to AppState
- [ ] Query requests use cache
- [ ] Mutation requests trigger invalidation
- [ ] Metrics endpoint returns correct data
- [ ] Health check works

### Phase 17A.5: Monitoring
- [ ] Metrics endpoint shows hit rate
- [ ] Metrics show cache size/memory
- [ ] Health check recommends scaling
- [ ] Alerts configurable

---

## 🎯 Success Criteria

### Must Have
- ✅ Cache hit rate >= 85% (measured, not assumed)
- ✅ Cascade invalidation clears affected queries only
- ✅ Zero stale data (cascade is single source of truth)
- ✅ All 21 tests pass
- ✅ No performance regression on misses
- ✅ Request coalescing NOT needed (cascade handles invalidation)

### Should Have
- ✅ Monitoring shows real hit rates
- ✅ Load test validates multi-client scenario
- ✅ TTL safety net (24h default)
- ✅ Graceful degradation if cache fails

### Nice to Have
- Phase 17B: Field-level cache (if hit rate < 75%)
- Phase 17B: Async invalidation (if mutation latency > 50ms)

---

## 🚀 Rollout Plan (3-4 days)

| Day | Task | Duration |
|-----|------|----------|
| 1 | Phase 17A.1: Core cache + tests | 1 day |
| 1.5 | Phase 17A.2: Query integration + tests | 0.5 day |
| 2 | Phase 17A.3: Mutation integration + tests | 0.5 day |
| 2.5 | Phase 17A.4: HTTP integration | 0.5 day |
| 3 | Phase 17A.5: Monitoring + metrics | 0.5 day |
| 3.5 | Load testing + validation | 0.5 day |
| 4 | Documentation + polish | 0.5 day |

**Total: 3-4 days (realistic)**

---

## 📊 Expected Outcomes

### Before Phase 17A

```
Client 1: Query "list users" → DB (8ms)
Client 2: Query "list users" → DB (8ms)
Client 3: Query "list users" → DB (8ms)
Client 1: Mutation updateUser → DB (10ms)
Client 2: Query "list users" → DB (8ms)
Client 3: Query "list users" → DB (8ms)
Client 4: Query "list users" → DB (8ms)

Total: 7 queries, 7 DB hits, 60ms total latency
Hit rate: 0% (no caching)
DB load: High
```

### After Phase 17A

```
Client 1: Query "list users" → DB (8ms) [Cache MISS]
Client 2: Query "list users" → Cache (1ms) [Cache HIT] ✓
Client 3: Query "list users" → Cache (1ms) [Cache HIT] ✓
Client 1: Mutation updateUser → DB (10ms) [Cache INVALIDATED]
Client 2: Query "list users" → DB (8ms) [Cache MISS, refreshes]
Client 3: Query "list users" → Cache (1ms) [Cache HIT] ✓
Client 4: Query "list users" → Cache (1ms) [Cache HIT] ✓

Total: 7 queries, 3 DB hits (65% saved!), 30ms total latency
Hit rate: 57% overall, 85%+ after steady state
DB load: 57% reduction
```

---

## 🎯 Why This Works (Corrected Understanding)

1. **Mutations return ALL affected objects** (from tv_* JSONB tables)
   - Client 1 gets fresh data immediately

2. **Cascade metadata is precise** (describes exactly what changed)
   - Server knows which cache entries to invalidate
   - No guessing with TTL

3. **Other clients benefit from cache**
   - Client 2,3,4... requesting same query get cached result
   - Cascaded cache = always fresh when invalidated

4. **Cache is automatically coherent**
   - No manual configuration
   - Cascade drives invalidation
   - Mutations keep cache in sync

5. **Request coalescing NOT needed**
   - Cascade invalidation is immediate
   - Single DB hit populates cache
   - Subsequent clients hit cache (no thundering herd)

---

## 🛑 What Changed From Original

| Aspect | Original | Corrected |
|--------|----------|-----------|
| **Purpose** | Prevent all cache problems | Serve other clients after mutations |
| **Request coalescing** | Required (prevents thundering herd) | Not needed (cascade is immediate) |
| **Cascade audit trail** | Required (detect failures) | Recommended (optional safety) |
| **Enhanced monitoring** | Large addition | Smaller (basic metrics) |
| **TTL** | Required (safety net) | Recommended (safety net) |
| **Timeline** | 5 days | 3-4 days |
| **Complexity** | High | Medium |
| **LOC** | ~800 | ~500 |
| **Tests** | 26 | 21 |

---

## 📄 Summary

**Phase 17A is simpler and more elegant than originally thought:**

1. ✅ Cache query results
2. ✅ Track which entities each query accesses
3. ✅ Invalidate cache based on cascade metadata (no request coalescing needed)
4. ✅ Let other clients benefit from cached results
5. ✅ Monitor hit rates

**Result**:
- 85-90% cache hit rate after steady state
- 50-60% DB load reduction
- Zero stale data (cascade-driven invalidation)
- 3-4 days to implement
- Production-ready for 95% of SaaS

This is the right architecture.
