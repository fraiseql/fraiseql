# Phase 17A: Cascade-Driven Server-Side Query Cache

**Status**: Implementation Plan
**Complexity**: Medium
**Effort**: 3-4 days
**Risk**: Low (composable with client caching)
**Version Target**: v1.9.0

---

## 🎯 Overview

Server-side query result caching that automatically invalidates based on **graphql-cascade mutation metadata**.

Instead of manually tracking which queries depend on which fields (Phase 17B complexity), we leverage the cascade data your mutations already return to drive intelligent, automatic cache invalidation.

**Key insight**: You already compute cascade metadata (which entities/fields changed). Use it to invalidate the server cache.

---

## 📊 Expected Impact

- **Cache hit rate**: 90-95% for typical workloads (NO TTL = cache lives until cascade invalidates)
- **DB load reduction**: 60-80% (more with client-side caching layered on top)
- **Response latency**: 8ms → 1-2ms (cached requests, no expiration check)
- **Implementation time**: 2-3 days (vs 2+ weeks for Phase 17B)
- **Test complexity**: 6 tests (vs 54 for Phase 17B)

---

## 🏗️ Architecture

### High-Level Flow (WITH Cascade Integration)

```
REQUEST: query { user(id: "123") { name cascade { ... } } }
    ↓
LOOKUP: cache["User:123:name:WITH_CASCADE"]?
    ├─ HIT  → return cached response (includes cascade!)
    │         → client-side graphql-cascade still processes cascade
    └─ MISS → execute query
    ↓
EXECUTE: PostgreSQL returns complete response with cascade
    {
      data: {
        user: { name: "John", cascade: { invalidations: { ... } } }
      }
    }
    ↓
CACHE: store ENTIRE response with entity tracking
    cache["User:123:name:WITH_CASCADE"] = full_response
    deps["User:123"] += ["User:123:name:WITH_CASCADE"]
    ↓
RESPONSE: { data: { user: { name: "John", cascade: { ... } } } }
    → Client receives cascade data
    → graphql-cascade library processes it

---

MUTATION: mutation { updateUser(id: "123", name: "Jane") { id name cascade { ... } } }
    ↓
EXECUTE: PostgreSQL returns mutation response WITH cascade
    cascade.invalidations.updated = [{ type: "User", id: "123" }]
    ↓
SERVER CACHE INVALIDATION: Use cascade to clear server cache
    for each updated in cascade:
        deps_to_clear = deps["User:123"]
        foreach dep in deps_to_clear:
            cache.remove(dep)  // Removes both WITH_CASCADE and NO_CASCADE variants
    ↓
CLIENT CACHE INVALIDATION: Response includes cascade
    Response sent to client WITH cascade metadata
    → graphql-cascade library on client processes cascade
    → Automatically invalidates Apollo/React Query cache
    ↓
RESPONSE: Include cascade metadata for client-side invalidation
    Both server and client caches are now empty!
```

### Cache Key Strategy

```rust
// Queries cached as:
// "User:123:*"              ← Any query about user 123
// "Post:456:author"         ← Specific post.author relationship
// "User:123:posts:2:title"  ← Nested: user 123's posts, page 2, title field
// "User:*:count"            ← All users count (wildcard for "select all")

// Cascade tells us what to invalidate:
cascade.invalidations.updated = [
    { type: "User", id: "123" }  ← Invalidate: "User:123:*"
]

cascade.invalidations.deleted = [
    { type: "Post", id: "456" }  ← Invalidate: "Post:456:*"
]
```

### Data Structures

```rust
/// A single cached query result
#[derive(Clone)]
struct CacheEntry {
    /// The actual query result (JSON)
    result: Arc<serde_json::Value>,

    /// Which entities this query accessed
    /// e.g., vec![("User", "123"), ("Post", "456")]
    accessed_entities: Vec<(String, String)>,

    // NOTE: NO created_at field needed!
    // Cache lives until cascade invalidates it.
}

/// Main query result cache
pub struct QueryResultCache {
    /// All cached entries: "cache_key" → CacheEntry
    entries: Arc<Mutex<HashMap<String, CacheEntry>>>,

    /// Dependency tracking: "User:123" → ["User:123:*", "User:123:posts"]
    /// Lets us quickly find affected queries when cascade says "User:123 changed"
    dependencies: Arc<Mutex<HashMap<String, Vec<String>>>>,

    /// Configuration
    config: CacheConfig,

    /// Metrics
    metrics: Arc<Mutex<CacheMetrics>>,
}

/// Cache configuration
pub struct CacheConfig {
    /// Maximum entries in cache (LRU eviction)
    pub max_entries: usize,

    /// Whether to cache list queries
    pub cache_list_queries: bool,

    // NOTE: NO TTL! Cache lives until cascade invalidates.
    // This ensures 90-95% hit rates and eliminates staleness from expiration.
}

/// Cache statistics
#[derive(Clone, Default)]
pub struct CacheMetrics {
    /// Number of cache hits
    pub hits: u64,

    /// Number of cache misses
    pub misses: u64,

    /// Total queries cached
    pub total_cached: u64,

    /// Total invalidations triggered
    pub invalidations: u64,

    /// Size of cache (number of entries)
    pub size: usize,
}
```

---

## 📝 Implementation Plan

### Phase 17A.1: Create Cache Module

**File**: `fraiseql_rs/src/cache/result_cache.rs` (new)

**Objectives**:
1. Implement `QueryResultCache` struct
2. Implement cache entry storage with LRU eviction
3. Implement dependency tracking
4. Implement metrics collection

**Code Structure**:
```rust
mod result_cache {
    /// Core cache operations
    pub struct QueryResultCache { ... }

    impl QueryResultCache {
        /// Create new cache
        pub fn new(config: CacheConfig) -> Self { }

        /// Get cached result (or None)
        pub fn get(&self, cache_key: &str) -> Option<Arc<serde_json::Value>> { }

        /// Cache query result
        pub fn put(
            &self,
            cache_key: String,
            result: serde_json::Value,
            accessed_entities: Vec<(String, String)>,
        ) { }

        /// Invalidate queries based on cascade data
        pub fn invalidate_from_cascade(
            &self,
            cascade: &serde_json::Value,
        ) -> Result<u64, String> { }

        /// Get current metrics
        pub fn metrics(&self) -> CacheMetrics { }

        /// Clear entire cache
        pub fn clear(&self) { }
    }


    /// LRU eviction
    impl QueryResultCache {
        fn evict_lru_if_full(&self) { }
    }
}
```

**Tests** (in module):
```rust
#[test]
fn test_cache_hit_returns_stored_value() { }

#[test]
fn test_cache_miss_returns_none() { }

#[test]
fn test_lru_eviction_when_full() { }

#[test]
fn test_cascade_invalidation_clears_entry() { }
```

**Acceptance Criteria**:
- ✅ Cache stores and retrieves results
- ✅ LRU eviction works correctly
- ✅ Metrics tracked accurately
- ✅ Thread-safe (Arc<Mutex>)
- ✅ No creation_at field needed (no TTL logic)

---

### Phase 17A.2: Integrate with Query Execution

**Files**:
- `fraiseql_rs/src/cache/mod.rs` (update exports)
- `fraiseql_rs/src/pipeline/unified.rs` (hook into execution)

**Objectives**:
1. Create query cache key from GraphQL query
2. **Include cascade selection in cache key** (if client requested cascade)
3. Check cache before executing
4. **Cache entire response** (including cascade if present)
5. Store with accessed entities

**Implementation Details**:

```rust
// In pipeline/unified.rs or new cache_middleware.rs

pub struct QueryCacheKey {
    /// Query signature (hash)
    pub signature: String,

    /// Whether this query includes cascade selection
    /// Important: "user { name }" != "user { name cascade { ... } }"
    pub has_cascade: bool,

    /// Which entities this query accesses
    pub accessed_entities: Vec<(String, String)>,
}

impl QueryCacheKey {
    /// Generate cache key from GraphQL query and variables
    pub fn from_query(
        query: &str,
        variables: &serde_json::Value,
    ) -> Result<Self, String> {
        // Parse query to extract:
        // 1. Root fields (e.g., "user", "posts")
        // 2. Filter arguments (e.g., id:"123", status:"active")
        // 3. Pagination args
        // 4. WHETHER cascade WAS REQUESTED (critical!)

        let has_cascade = query.contains("cascade");

        // Hash to create signature (includes cascade selection)
        let mut signature = format!(
            "{:x}",
            calculate_hash(query, variables)
        );

        // Append cascade marker to signature
        if has_cascade {
            signature.push_str(":WITH_CASCADE");
        } else {
            signature.push_str(":NO_CASCADE");
        }

        Ok(QueryCacheKey {
            signature,
            has_cascade,
            accessed_entities: extract_entities(query, variables)?,
        })
    }
}

// Hook into query execution:
pub async fn execute_query_with_cache(
    cache: &QueryResultCache,
    query: &str,
    variables: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    let cache_key = QueryCacheKey::from_query(query, variables)?;

    // Check cache
    if let Some(cached) = cache.get(&cache_key.signature) {
        return Ok((*cached).clone());
    }

    // Execute query (miss)
    // This returns ENTIRE response including cascade if requested
    let result = execute_query_uncached(query, variables).await?;

    // Store in cache
    // IMPORTANT: We cache the entire response!
    // If client requested cascade, it's in the response
    // If not, response doesn't have cascade field
    cache.put(
        cache_key.signature,
        result.clone(),  // Full response (WITH cascade if present!)
        cache_key.accessed_entities,
    );

    Ok(result)  // Returns to client with cascade metadata if requested
}
```

**Acceptance Criteria**:
- ✅ Cache key generation works for all query types
- ✅ Cache hit/miss tracking works
- ✅ Cached results returned correctly
- ✅ Non-cacheable queries skipped (mutations, subscriptions)
- ✅ No change to query execution logic (transparent)

---

### Phase 17A.3: Integrate with Mutation Response

**Files**:
- `fraiseql_rs/src/mutation/response_builder.rs` (update)
- `fraiseql_rs/src/http/middleware.rs` or new `cache_invalidation_middleware.rs`

**Objectives**:
1. Extract cascade metadata from mutation response
2. Use cascade to invalidate server cache
3. Return invalidation stats to client (optional)

**Implementation Details**:

```rust
// New file: fraiseql_rs/src/http/cache_invalidation_middleware.rs

use crate::cache::QueryResultCache;
use crate::mutation::FullResponse;
use serde_json::Value as JsonValue;

/// Handle cache invalidation after mutation
pub fn invalidate_cache_from_mutation(
    cache: &QueryResultCache,
    mutation_response: &serde_json::Value,
) -> Result<CacheInvalidationStats, String> {
    // Extract cascade metadata
    let cascade = mutation_response
        .get("data")
        .and_then(|d| d.get("cascade"))
        .ok_or("No cascade metadata found")?;

    // Use cascade to invalidate
    let count = cache.invalidate_from_cascade(cascade)?;

    Ok(CacheInvalidationStats {
        invalidated_entries: count,
        timestamp: chrono::Utc::now(),
    })
}

#[derive(Serialize)]
pub struct CacheInvalidationStats {
    pub invalidated_entries: u64,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

// In Axum middleware or mutation handler:
pub async fn handle_mutation_endpoint(
    state: Arc<AppState>, // Contains cache
    body: String,
) -> Result<Response> {
    // Execute mutation
    let (mutation_response, cascade) = execute_mutation(&body).await?;

    // Invalidate cache based on cascade
    if let Some(cascade_data) = &cascade {
        match state.query_cache.invalidate_from_cascade(cascade_data) {
            Ok(count) => {
                debug!("Invalidated {} cache entries", count);
            }
            Err(e) => {
                warn!("Cache invalidation error: {}", e);
                // Don't fail the mutation, just log it
            }
        }
    }

    Ok(Response::json(mutation_response))
}
```

**Cascade Data Structure** (from PostgreSQL):
```json
{
  "cascade": {
    "invalidations": {
      "updated": [
        { "type": "User", "id": "123", "operation": "update" },
        { "type": "User", "id": "123", "operation": "update" }
      ],
      "deleted": [
        { "type": "Post", "id": "456" }
      ]
    },
    "metadata": {
      "timestamp": "2025-01-04T12:00:00Z"
    }
  }
}
```

**Acceptance Criteria**:
- ✅ Cascade extraction works
- ✅ Invalid cascade data handled gracefully
- ✅ Cache invalidation called correctly
- ✅ Mutation completes even if cache invalidation fails
- ✅ Metrics updated after invalidation

---

### Phase 17A.4: Create Test Suite

**File**: `fraiseql_rs/src/cache/tests/result_cache_tests.rs` (new)

**6 Core Tests** (TTL removed = simpler tests):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    /// Test 1: Basic cache hit
    #[test]
    fn test_cache_hit_returns_stored_value() {
        let cache = QueryResultCache::new(CacheConfig::default());
        let result = json!({"name": "John"});

        cache.put(
            "User:123".to_string(),
            result.clone(),
            vec![("User".to_string(), "123".to_string())],
        );

        let cached = cache.get("User:123");
        assert_eq!(cached.unwrap(), result);
    }

    /// Test 2: Cache miss
    #[test]
    fn test_cache_miss_returns_none() {
        let cache = QueryResultCache::new(CacheConfig::default());
        assert!(cache.get("NonExistent").is_none());
    }

    /// Test 3: LRU eviction when cache is full
    #[test]
    fn test_lru_eviction_when_full() {
        let mut config = CacheConfig::default();
        config.max_entries = 3;

        let cache = QueryResultCache::new(config);

        // Add 5 entries (max is 3)
        for i in 0..5 {
            cache.put(
                format!("Key:{i}"),
                json!({"value": i}),
                vec![("Type".to_string(), format!("{i}"))],
            );
        }

        // Should only have 3 entries (LRU evicted 2 oldest)
        assert_eq!(cache.metrics().size, 3);
    }

    /// Test 4: Cascade invalidation - single entity
    #[test]
    fn test_cascade_invalidates_affected_queries() {
        let cache = QueryResultCache::new(CacheConfig::default());

        // Cache queries about different users
        cache.put(
            "User:123:name".to_string(),
            json!({"name": "John"}),
            vec![("User".to_string(), "123".to_string())],
        );

        cache.put(
            "User:456:name".to_string(),
            json!({"name": "Jane"}),
            vec![("User".to_string(), "456".to_string())],
        );

        assert_eq!(cache.metrics().size, 2);

        // Cascade: User 123 was updated
        let cascade = json!({
            "invalidations": {
                "updated": [
                    {"type": "User", "id": "123"}
                ]
            }
        });

        let invalidated = cache.invalidate_from_cascade(&cascade).unwrap();

        // Should invalidate User 123 queries, not 456
        assert_eq!(invalidated, 1);
        assert!(cache.get("User:123:name").is_none());
        assert!(cache.get("User:456:name").is_some());
    }

    /// Test 5: Multiple cascade invalidations
    #[test]
    fn test_cascade_multiple_invalidations() {
        let cache = QueryResultCache::new(CacheConfig::default());

        cache.put("User:100".to_string(), json!({"name": "Alice"}), vec![("User".to_string(), "100".to_string())]);
        cache.put("User:200".to_string(), json!({"name": "Bob"}), vec![("User".to_string(), "200".to_string())]);
        cache.put("Post:1".to_string(), json!({"title": "Post1"}), vec![("Post".to_string(), "1".to_string())]);

        assert_eq!(cache.metrics().size, 3);

        // Cascade: Both users updated, one post deleted
        let cascade = json!({
            "invalidations": {
                "updated": [
                    {"type": "User", "id": "100"},
                    {"type": "User", "id": "200"}
                ],
                "deleted": [
                    {"type": "Post", "id": "1"}
                ]
            }
        });

        let invalidated = cache.invalidate_from_cascade(&cascade).unwrap();
        assert_eq!(invalidated, 3);
        assert_eq!(cache.metrics().size, 0);
    }

    /// Test 6: Hit/miss metrics tracking
    #[test]
    fn test_metrics_tracking() {
        let cache = QueryResultCache::new(CacheConfig::default());

        // Miss
        cache.get("NotThere");

        // Put
        cache.put(
            "Key:1".to_string(),
            json!({"value": 1}),
            vec![("Type".to_string(), "1".to_string())],
        );

        // Hit
        cache.get("Key:1");

        let metrics = cache.metrics();
        assert_eq!(metrics.hits, 1);
        assert_eq!(metrics.misses, 1);
        assert_eq!(metrics.size, 1);
    }
}
```

**Acceptance Criteria**:
- ✅ All 6 tests pass
- ✅ No flakiness (run 10x in CI)
- ✅ Tests cover happy path and edge cases
- ✅ Tests verify metrics accuracy

---

### Phase 17A.5: HTTP Server Integration

**Files**:
- `fraiseql_rs/src/http/axum_server.rs` or `middleware.rs`
- `fraiseql_rs/src/http/mod.rs`

**Objectives**:
1. Add `QueryResultCache` to AppState
2. Initialize cache with config
3. Wire up query execution to use cache
4. Wire up mutation invalidation

**Code Structure**:

```rust
// In fraiseql_rs/src/http/axum_server.rs

pub struct AppState {
    // ... existing fields ...

    /// Server-side query result cache
    pub query_cache: Arc<QueryResultCache>,
}

pub async fn create_app_state() -> Result<AppState> {
    let cache_config = CacheConfig {
        max_entries: 10000,    // LRU eviction if we exceed this
        cache_list_queries: true,
        // NO TTL! Cache lives until cascade invalidates
    };

    let query_cache = Arc::new(QueryResultCache::new(cache_config));

    Ok(AppState {
        // ... other fields ...
        query_cache,
    })
}

// Middleware to cache queries
pub async fn cache_query_middleware(
    state: Arc<AppState>,
    query: String,
    variables: serde_json::Value,
) -> Result<serde_json::Value> {
    // Check if this is a mutation or subscription
    // (don't cache mutations or subscriptions)
    if is_mutation(&query) || is_subscription(&query) {
        return execute_query_uncached(&query, &variables).await;
    }

    // Generate cache key
    let cache_key = QueryCacheKey::from_query(&query, &variables)?;

    // Check cache
    if let Some(cached) = state.query_cache.get(&cache_key.signature) {
        info!("Cache hit: {}", cache_key.signature);
        return Ok((*cached).clone());
    }

    // Execute and cache
    let result = execute_query_uncached(&query, &variables).await?;
    state.query_cache.put(
        cache_key.signature,
        result.clone(),
        cache_key.accessed_entities,
    );

    Ok(result)
}

// Mutation handler with invalidation
pub async fn handle_mutation(
    state: Arc<AppState>,
    mutation: String,
    variables: serde_json::Value,
) -> Result<serde_json::Value> {
    // Execute mutation (always)
    let result = execute_mutation_uncached(&mutation, &variables).await?;

    // Extract cascade and invalidate
    if let Some(cascade) = result.get("data").and_then(|d| d.get("cascade")) {
        match state.query_cache.invalidate_from_cascade(cascade) {
            Ok(count) => {
                info!("Invalidated {} cache entries from mutation", count);
            }
            Err(e) => {
                warn!("Cache invalidation error: {}", e);
                // Continue anyway - mutation succeeded
            }
        }
    }

    Ok(result)
}
```

**Acceptance Criteria**:
- ✅ Cache initialized on server startup
- ✅ Query execution uses cache
- ✅ Mutations trigger invalidation
- ✅ No impact on non-cached queries
- ✅ Graceful fallback if cache fails

---

### Phase 17A.6: Metrics & Monitoring

**File**: `fraiseql_rs/src/cache/metrics.rs` (update/create)

**Objectives**:
1. Expose cache metrics via HTTP endpoint
2. Track hit rate, size, invalidations
3. Optional: Prometheus metrics

**Implementation**:

```rust
// Cache metrics endpoint
pub async fn cache_metrics_endpoint(
    state: Arc<AppState>,
) -> impl Response {
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
            "hit_rate": format!("{:.2}%", hit_rate * 100.0),
            "size": metrics.size,
            "total_cached": metrics.total_cached,
            "invalidations": metrics.invalidations,
        }
    })
}

// Query for cache metrics
#[allow(dead_code)]
type Query {
    _metrics: Metrics!
}

type Metrics {
    cache: CacheMetrics!
}

type CacheMetrics {
    hits: Int!
    misses: Int!
    hitRate: Float!  # 0.0-1.0
    size: Int!
    totalCached: Int!
    invalidations: Int!
}
```

**Acceptance Criteria**:
- ✅ Metrics endpoint returns current stats
- ✅ Metrics are accurate
- ✅ Hit rate calculation correct
- ✅ Optional GraphQL query works

---

## 🧪 Testing Strategy

### Unit Tests (Phase 17A.1)
- Cache storage/retrieval
- TTL expiration
- LRU eviction
- Dependency tracking

### Integration Tests (Phase 17A.2-3)
- Query cache key generation
- Mutation invalidation
- Cascade parsing
- Multi-entity invalidation

### End-to-End Tests (Phase 17A.4)
- Full query → cache → result flow
- Full mutation → invalidation → new query flow
- Metrics accuracy

### Performance Tests
- Cache hit/miss latency
- Invalidation speed (1000+ entries)
- Memory usage (10k entries)

---

## 📋 Acceptance Criteria

### Must Have (v1.0)
- ✅ Query caching works
- ✅ Cascade-driven invalidation works
- ✅ All 8 tests pass
- ✅ Hit rate >= 60% for typical workloads
- ✅ No stale data (tests verify)
- ✅ Graceful degradation if cache fails

### Should Have (v1.1)
- 📊 Cache metrics exposed (hit rate, size)
- 📊 Performance benchmarks
- 📊 Cache configuration options

### Nice to Have (v2.0)
- Distributed cache (Redis)
- Warm-up cache on startup
- Cache warming strategies
- Client-side cache hints

---

## 🔄 Rollout Plan

### Step 1: Implement Core Cache (0.5 day)
- Create `result_cache.rs`
- Implement storage/retrieval (NO TTL logic = simple!)
- LRU eviction only
- All cache tests pass

### Step 2: Hook Into Query Execution (0.5 day)
- Create cache key generation
- Hook into pipeline
- Query tests pass

### Step 3: Hook Into Mutation Invalidation (0.5 day)
- Extract cascade metadata
- Call invalidation
- Mutation tests pass

### Step 4: HTTP Integration (0.5 day)
- Add to AppState
- Middleware setup
- E2E tests pass

### Step 5: Metrics & Monitoring (0.25 days)
- Metrics endpoint
- Hit rate tracking

### Step 6: Performance Testing (0.25 days)
- Benchmark cache hits/misses
- Memory usage
- Verify 90%+ hit rate

---

## 🎯 Success Metrics

**Before Phase 17A**:
```
Query: { users { id name } }
  → 8-10ms (DB + serialization)

Mutation: updateUser → cascade clears cache → refetch
  → 15ms (mutation) + 8ms (refetch DB) = 23ms
```

**After Phase 17A (NO TTL)**:
```
Query: { users { id name } }  (cache hit - lives until mutation)
  → 1-2ms (memory only)
  → 90-95% hit rate (no expiration!)

Mutation: updateUser → cascade clears cache → refetch
  → 15ms (mutation) + 1ms (refetch from DB) = 16ms
  → Next identical query hits new cache: 1-2ms
```

**Expected Database Load**:
- Read queries: -60-80% (dramatically fewer DB hits due to no TTL)
- Mutations: No change (always go to DB)
- Overall: -50-60% database load

---

## 📝 Documentation

### For Users
1. **Cache Configuration**: How to configure TTLs, max size
2. **Metrics**: How to monitor hit rates
3. **Cascade Integration**: How cascade drives invalidation

### For Developers
1. **Architecture**: How cache integrates with pipeline
2. **Adding Cacheable Endpoints**: Checklist for new queries
3. **Troubleshooting**: Common issues and fixes

---

## 🚀 Future Enhancements (Phase 17B+)

After Phase 17A proves successful:

1. **Field-level invalidation** (Phase 17B):
   - Cache only what queries asked for
   - Selective invalidation on field changes
   - Higher hit rates (80-90%)

2. **Redis integration**:
   - Distributed caching across processes
   - Cluster-aware invalidation
   - Persistent cache between deployments

3. **Cache warming**:
   - Warm high-frequency queries on startup
   - Predictive invalidation
   - Smart TTL adjustment

4. **Client hints**:
   - Clients specify cache TTL preferences
   - Server respects client cache policies
   - Cache-Control headers

---

## ⚠️ Risk Mitigation

| Risk | Mitigation |
|------|-----------|
| **Stale data** | TTL prevents indefinite staleness; cascade invalidation handles known changes |
| **Cache misses break app** | Fallback to DB on cache errors; mutations never cached |
| **Memory bloat** | LRU eviction; configurable max entries |
| **Invalidation lag** | Sub-millisecond invalidation; mutation waits for cache |
| **Complex cascade** | Simple entity-level matching (not field-level) |

---

## 📞 Questions & Decisions

### Decision 1: Cache List Queries?
- **YES** (but conservatively)
- Include pagination params in cache key
- Invalidate on any entity change of that type

### Decision 2: Multi-Entity Queries?
- **YES**
- Track all entities accessed
- Invalidate if ANY entity changes

### Decision 3: When to Enable?
- **Start disabled**, enable per-type via config
- Gradual rollout: User → Post → Comment
- Monitor metrics before full enable

### Decision 4: Invalidation Failure Handling?
- **Warn, continue**
- Log cache invalidation errors
- Don't fail mutation if cache fails

---

## 🗓️ Timeline

| Phase | Tasks | Duration | Owner |
|-------|-------|----------|-------|
| **17A.1** | Core cache module (no TTL!) + tests | 0.5 day | Engineer |
| **17A.2** | Query execution integration | 0.5 day | Engineer |
| **17A.3** | Mutation invalidation | 0.5 day | Engineer |
| **17A.4** | HTTP server integration | 0.5 day | Engineer |
| **17A.5** | Metrics & monitoring | 0.25 day | Engineer |
| **17A.6** | Performance testing | 0.25 day | Engineer |
| **17A.7** | Docs & examples | 0.25 day | Engineer |
| **17A.8** | Code review & polish | 0.25 day | Reviewer |
| **Total** | | **~2-3 days** | |

---

## ✅ Verification Checklist

Before merging to `dev`:

- [ ] All 6 cache tests pass (no TTL tests needed!)
- [ ] All integration tests pass
- [ ] No flakiness (tests run 10x consistently)
- [ ] Cache hit rate >= 90% in E2E tests
- [ ] ZERO stale data (no TTL = cascade is single source of truth)
- [ ] Metrics are accurate
- [ ] Documentation complete
- [ ] Performance benchmarks meet targets
- [ ] Code review approved
- [ ] Branch passes CI/CD

---

## 📚 References

- **Cascade Data**: `fraiseql_rs/src/cascade/mod.rs`
- **Mutation Response**: `fraiseql_rs/src/mutation/response_builder.rs`
- **HTTP Server**: `fraiseql_rs/src/http/axum_server.rs`
- **Query Pipeline**: `fraiseql_rs/src/pipeline/unified.rs`

---

## 🎉 Summary

Phase 17A is a **simple, pragmatic** server-side caching system that:

1. ✅ Leverages cascade metadata you already compute
2. ✅ Automatically invalidates on mutations (cascade = single source of truth)
3. ✅ Works alongside client-side caching (double-caching for resilience)
4. ✅ Takes 1/15th the effort of Phase 17B (2-3 days vs 2+ weeks)
5. ✅ **Gives 90-95% hit rates** (vs Phase 17B's 80-90%)
6. ✅ **ZERO stale data** (no TTL = cascade is only way to clear cache)
7. ✅ Only 6 tests (vs Phase 17B's 54)
8. ✅ ~300 lines of code (vs Phase 17B's 1,100+)

**Key insight**: Don't build complicated TTL expiration logic. The cascade metadata you already compute is more precise than any TTL could be.
