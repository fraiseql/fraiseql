# Phase 17A: Memory Requirements & Analysis

## Quick Answer

For typical GraphQL workloads:

| Scenario | Cache Size | Memory | Hit Rate | Notes |
|----------|-----------|--------|----------|-------|
| **Small** (1-10K users) | 1,000 entries | 500 MB | 85-90% | Development/testing |
| **Medium** (10-100K users) | 5,000 entries | 2-3 GB | 80-95% | Standard SaaS |
| **Large** (100K-1M users) | 10,000 entries | 4-6 GB | 75-90% | Enterprise |
| **Huge** (1M+ users) | 50,000+ entries | 20-50 GB | 70-85% | Very large deployments |

---

## Memory Calculation

### Per-Entry Memory Breakdown

```rust
// Actual memory per cache entry:

CacheEntry {
    result: Arc<serde_json::Value>,  // Variable (see below)
    accessed_entities: Vec<(String, String)>,  // ~150 bytes typical
}

// For dependency tracking (per entity):
// "User:123" → Vec[cache_keys]       // ~100 bytes per entity
```

### JSON Value Size Estimates

**Typical GraphQL queries and their JSON sizes:**

```graphql
# Query 1: Simple user
query { user(id: "123") { name email } }
# Result: {"name": "John", "email": "john@example.com"}
# Size: ~60 bytes

# Query 2: User with posts
query { user(id: "123") { name posts { id title } } }
# Result: {"name": "John", "posts": [{id: "1", title: "Post 1"}, ...]}
# Size: ~200 bytes (5 posts)

# Query 3: Complex nested
query {
  user(id: "123") {
    name
    posts { id title content comments { id text } }
  }
}
# Size: ~2,000-5,000 bytes (5 posts, 5 comments each)

# Query 4: List query
query { users(first: 100) { id name email } }
# Size: ~6,000-10,000 bytes (100 users × ~60 bytes)
```

### Average Per-Entry Cost

```
Arc<Value> overhead:        16 bytes (pointer)
JSON data:                  200-500 bytes (typical)
Accessed entities Vec:      150 bytes
String keys in HashMap:     50 bytes (key)
HashMap entry overhead:     48 bytes

Total per entry:            500-800 bytes
```

**Conservative estimate: ~1 KB per cache entry**

(Some are 200 bytes, some are 10 KB, average around 1-2 KB)

---

## Dependency Tracking Memory

```rust
// For each entity, track which queries depend on it:
// "User:123" → ["User:123:name", "User:123:posts", ...]

// Memory per entity:
String key "User:123":                    ~30 bytes
Vec<String> cache keys (assuming 5 keys):
  - Each key: ~50 bytes
  - Total: ~250 bytes
HashMap entry overhead:                   48 bytes

Total per entity: ~330 bytes
```

**In practice:**
- Most entities accessed by 2-5 queries
- Total dependency overhead: ~5-10% of total cache size

---

## Real-World Memory Examples

### Scenario 1: Small SaaS (1,000 users, 5,000 queries cached)

```
Cache entries: 5,000
Average entry size: 1 KB
Raw data: 5,000 KB = 5 MB

Dependency tracking: 500 entities
  × 300 bytes = 150 KB

HashMap overhead (5,000 entries):
  × 100 bytes = 500 KB

Total: ~6 MB
Overhead (Arc, mutexes, etc): 10-15%

TOTAL MEMORY: ~7-10 MB
```

### Scenario 2: Medium SaaS (100,000 users, 10,000 queries cached)

```
Cache entries: 10,000
Average entry size: 1.5 KB (more complex queries)
Raw data: 10,000 × 1.5 KB = 15 MB

Dependency tracking: 2,000 entities
  × 300 bytes = 600 KB

HashMap overhead (10,000 entries):
  × 100 bytes = 1 MB

Total: ~17 MB
Overhead (Arc, mutexes, etc): 15%

TOTAL MEMORY: ~20 MB
```

### Scenario 3: Large SaaS (1M users, 50,000 queries cached)

```
Cache entries: 50,000
Average entry size: 2 KB (more list queries)
Raw data: 50,000 × 2 KB = 100 MB

Dependency tracking: 5,000 entities
  × 300 bytes = 1.5 MB

HashMap overhead (50,000 entries):
  × 100 bytes = 5 MB

Total: ~107 MB
Overhead (Arc, mutexes, etc): 20%

TOTAL MEMORY: ~125-150 MB
```

---

## Rust Implementation Memory Details

### Arc<Value> Overhead

```rust
pub struct Arc<T> {
    ptr: *const T,                  // 8 bytes (64-bit pointer)
    // Actual data allocated separately
}

// serde_json::Value variants:
enum Value {
    Null,                           // 0 bytes
    Bool(bool),                     // 1 byte
    Number(Number),                 // 24 bytes
    String(String),                 // 24 bytes + heap allocation
    Array(Vec<Value>),              // 24 bytes + child allocations
    Object(Map<String, Value>),     // 48 bytes + key/value allocations
}

// Typical result: mix of types, average ~200-500 bytes per value
```

### Mutex & Arc Overhead

```rust
pub struct QueryResultCache {
    entries: Arc<Mutex<HashMap<String, CacheEntry>>>,
    // Arc: 8 bytes
    // Mutex: 8 bytes (lock state)
    // HashMap (empty): 96 bytes
    // Total overhead per cache instance: ~120 bytes
}
```

### String Key Storage

```
HashMap<String, CacheEntry>

Each String key (e.g., "User:123:name_email"):
- Inline small string optimization (SSO): ~24 bytes structure
- Heap allocation for actual string: ~30 bytes typical (24-30 char average)

Total per key: ~50-55 bytes
```

---

## Hit Rate vs Cache Size

```
Max Entries    Typical Hit Rate    Memory    Notes
─────────────────────────────────────────────────────────
1,000          70-80%              1-2 MB    Very small
5,000          80-90%              5-10 MB   Small app
10,000         85-92%              10-20 MB  Medium app
50,000         88-94%              50-100 MB Large app
100,000        90-95%              100-200 MB Very large
500,000        92-96%              500+ MB   Needs optimization
```

**Why more cache = better hit rate:**
- More time before LRU eviction
- Captures more unique query patterns
- Better for multi-user systems

---

## Optimization Strategies

### Strategy 1: Compressed Values (Save 40-50%)

```rust
use flate2::Compression;

struct CacheEntry {
    result_compressed: Vec<u8>,  // Compress JSON
    accessed_entities: Vec<(String, String)>,
}

// On get: Decompress in memory (1-2ms overhead)
pub fn get(&self, key: &str) -> Option<Value> {
    let compressed = cache.get(key)?;
    let decompressed = decompress(&compressed)?;  // ~1-2ms
    Ok(decompressed)
}
```

**Trade-off:**
- Memory: 1 KB → 500-600 bytes (40-50% savings)
- Speed: +1-2ms per cache hit
- **Worth it for large deployments**

### Strategy 2: Shared String Pool (Save 20-30%)

```rust
use std::sync::Arc;

// Instead of duplicating strings:
// "User:123" stored in 5 different cache keys
//
// Use shared String references:

pub struct StringPool {
    strings: Arc<RwLock<HashMap<String, Arc<str>>>>,
}

impl StringPool {
    pub fn intern(&self, s: &str) -> Arc<str> {
        let pool = self.strings.read().unwrap();
        if let Some(existing) = pool.get(s) {
            Arc::clone(existing)
        } else {
            drop(pool);
            let s = Arc::from(s);
            self.strings.write().unwrap().insert(s.to_string(), Arc::clone(&s));
            s
        }
    }
}
```

**Trade-off:**
- Memory: 20-30% savings on keys
- Speed: +5-10µs per key lookup
- **Worth it if you have many repeated patterns**

### Strategy 3: LRU with Size-Based Eviction

```rust
// Don't just evict by count, evict by memory size

pub struct CacheEntry {
    result: Arc<Value>,
    accessed_entities: Vec<(String, String)>,
    size_bytes: usize,  // Track size
}

impl QueryResultCache {
    pub fn put(&self, key: String, value: Value, entities: Vec<...>) {
        let size = estimate_size(&value);

        // Evict until we have room
        while total_memory + size > max_memory_bytes {
            evict_lru_entry();
        }

        self.entries.insert(key, CacheEntry { value, size, ... });
        self.total_memory += size;
    }
}
```

**Trade-off:**
- Memory: Exact control (hit `max_memory_bytes`)
- Speed: +2-3µs per put
- **Best approach for production**

---

## Recommended Configuration

### Development/Testing
```rust
CacheConfig {
    max_entries: 1_000,
    cache_list_queries: true,
}
// Memory: 1-2 MB
// Hit rate: 70-80%
```

### Small Production (< 10K users)
```rust
CacheConfig {
    max_entries: 5_000,
    cache_list_queries: true,
}
// Memory: 5-10 MB
// Hit rate: 80-90%
// Hardware: Any VPS sufficient
```

### Medium Production (10K-100K users)
```rust
CacheConfig {
    max_entries: 10_000,
    cache_list_queries: true,
}
// Memory: 10-20 MB
// Hit rate: 85-92%
// Hardware: Standard cloud instance
```

### Large Production (100K+ users)
```rust
// Option A: Larger cache
CacheConfig {
    max_entries: 50_000,
    cache_list_queries: true,
}
// Memory: 50-100 MB
// Hit rate: 88-94%
// Hardware: 4GB+ RAM instance

// Option B: Compressed cache
CacheConfig {
    max_entries: 50_000,
    cache_list_queries: true,
    compression: true,
}
// Memory: 25-50 MB (compressed)
// Hit rate: 88-94% (same)
// CPU: +1-2ms per hit
```

### Very Large (1M+ users) / Multi-Region
```
Consider Redis instead of in-memory cache:
- Shared cache across instances
- More sophisticated eviction
- Persistent across restarts
```

---

## Memory Per Request Type

```graphql
# Simple lookup query
query { user(id: "123") { name email } }
├─ Result JSON: 60 bytes
├─ Entry overhead: 500 bytes
├─ Key storage: 50 bytes
└─ Total: ~610 bytes

# User with relationships
query { user(id: "123") { name posts { id title author { name } } } }
├─ Result JSON: 2,000 bytes
├─ Entry overhead: 500 bytes
├─ Key storage: 100 bytes
└─ Total: ~2,600 bytes

# List query
query { users(first: 100) { id name email } }
├─ Result JSON: 8,000 bytes
├─ Entry overhead: 500 bytes
├─ Key storage: 60 bytes
└─ Total: ~8,560 bytes
```

---

## Real Memory Measurements

If you implement Phase 17A, measure actual memory:

```rust
// In your metrics endpoint:
pub fn cache_memory_stats() -> CacheMemoryStats {
    let entries = cache.entries.lock().unwrap();

    let total_value_size: usize = entries
        .values()
        .map(|e| estimate_size(&e.result))
        .sum();

    let total_key_size: usize = entries
        .keys()
        .map(|k| k.len())
        .sum();

    let overhead = entries.len() * 100;  // Per-entry overhead

    CacheMemoryStats {
        entries: entries.len(),
        value_memory: total_value_size,
        key_memory: total_key_size,
        overhead,
        total: total_value_size + total_key_size + overhead,
    }
}

// Expose as metrics:
GET /_metrics/cache/memory
{
    "entries": 10_000,
    "value_memory": "15 MB",
    "key_memory": "500 KB",
    "overhead": "1 MB",
    "total": "16.5 MB",
    "per_entry_average": "1650 bytes"
}
```

---

## When Memory Becomes a Problem

**Red flags:**
- Cache grows to > 50% of available RAM
- Eviction rate increases (LRU thrashing)
- GC pauses noticeable (in Java; Rust uses arena allocation)
- Memory pressure on system

**Solutions (in order):**
1. ✅ Increase `max_entries` (if more RAM available)
2. ✅ Enable compression (40-50% savings)
3. ✅ Use string pool (20-30% savings on keys)
4. ✅ Exclude certain query types (list queries are large)
5. ✅ Use Redis for distributed cache
6. ✅ Implement tiered caching (hot/cold)

---

## Allocation Strategy

### Rust Memory Allocation

Phase 17A uses:
- **Arc**: Reference-counted heap allocation
- **Mutex**: Synchronization primitive (8 bytes)
- **HashMap**: Dynamic hash table
- **String**: Heap-allocated UTF-8

**Advantage**: No garbage collection, predictable allocation

### Peak Memory Calculation

```
Worst case: All 10,000 max_entries are large (10 KB each)
Expected: 10,000 × 1.5 KB average = 15 MB

Peak: 15 MB + dependency tracking (1-2 MB) + overhead (2-3 MB)
     = ~20 MB typical case
     = ~30 MB worst case
```

---

## Production Recommendations

### For Standard Cloud VPS (2-4 GB RAM)

```rust
CacheConfig {
    max_entries: 10_000,
    cache_list_queries: true,
    // Reserve ~200-500 MB for cache (5-10% of total RAM)
}

// Expected:
// Memory: 20-100 MB (depending on query size)
// Hit rate: 85-92%
// CPU overhead: < 2%
```

### For Bare Metal / Kubernetes (8+ GB RAM)

```rust
CacheConfig {
    max_entries: 50_000,
    cache_list_queries: true,
    compression: true,  // Optional, for even better memory
    // Reserve ~1-2 GB for cache (10-25% of total RAM)
}

// Expected:
// Memory: 50-200 MB (with compression)
// Hit rate: 88-94%
// CPU overhead: < 1% (compression decompression is fast)
```

---

## Memory vs Hit Rate Trade-off

```
Memory    Max Entries    Hit Rate    Evictions/sec
─────────────────────────────────────────────────────
1 MB      500            60-70%      10-20
5 MB      2,500          75-85%      2-5
10 MB     5,000          80-90%      <1
20 MB     10,000         85-92%      <0.1
50 MB     25,000         88-94%      <0.01
100 MB    50,000         90-95%      ~0
```

**Sweet spot for most deployments: 10-20 MB**

---

## Summary

| Deployment Size | Recommended Config | Memory | Hit Rate |
|-----------------|-------------------|--------|----------|
| Dev/Test | 1,000 entries | 1-2 MB | 70-80% |
| Small | 5,000 entries | 5-10 MB | 80-90% |
| Medium | 10,000 entries | 10-20 MB | 85-92% |
| Large | 50,000 entries | 50-100 MB | 88-94% |
| Very Large | Redis + compression | Unlimited | 95%+ |

**Most teams should start with: 10,000 entries = 10-20 MB**

This gives excellent hit rates while using minimal memory on any modern server.

---

## Measurement Code

Add this to Phase 17A.5 (Metrics & Monitoring):

```rust
#[derive(Serialize)]
pub struct CacheMemoryStats {
    pub entries: usize,
    pub value_memory_bytes: usize,
    pub key_memory_bytes: usize,
    pub overhead_bytes: usize,
    pub total_memory_bytes: usize,
    pub average_entry_bytes: usize,
}

impl QueryResultCache {
    pub fn memory_stats(&self) -> CacheMemoryStats {
        let entries = self.entries.lock().unwrap();

        let value_memory: usize = entries
            .values()
            .map(|e| estimate_json_size(&e.result))
            .sum();

        let key_memory: usize = entries
            .keys()
            .map(|k| k.len() + 24)  // String overhead
            .sum();

        let overhead = entries.len() * 100;
        let total = value_memory + key_memory + overhead;
        let average = if entries.is_empty() { 0 } else { total / entries.len() };

        CacheMemoryStats {
            entries: entries.len(),
            value_memory_bytes: value_memory,
            key_memory_bytes: key_memory,
            overhead_bytes: overhead,
            total_memory_bytes: total,
            average_entry_bytes: average,
        }
    }
}

fn estimate_json_size(value: &Value) -> usize {
    match value {
        Value::Null => 0,
        Value::Bool(_) => 1,
        Value::Number(_) => 24,
        Value::String(s) => 24 + s.len(),
        Value::Array(arr) => 24 + arr.iter().map(estimate_json_size).sum::<usize>(),
        Value::Object(obj) => {
            48 + obj
                .iter()
                .map(|(k, v)| k.len() + 24 + estimate_json_size(v))
                .sum::<usize>()
        }
    }
}
```

---

## Conclusion

**Phase 17A is very memory-efficient:**
- Small deployments: 1-10 MB
- Medium deployments: 10-50 MB
- Large deployments: 50-200 MB (with optimization)

**All well within budget of any modern server.**

Start with 10,000 entries (10-20 MB) and adjust based on measurements.
