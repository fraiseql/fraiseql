# Phase 7: Quick Start Guide

**Status**: Ready to implement after Phase 3.5

**Duration**: 3 weeks (240 hours)

**Goal**: Entity-level cache invalidation using UUIDs from mutation responses

---

## What's the Problem?

**Current (Phase 2)**:

```
Query: { user(id: "uuid-1") { name } }  ← reads v_user
Mutation: updateUser(id: "uuid-2") → triggers invalidation of v_user
Result: Query cache cleared even though it queries different user
Impact: 60-80% cache hit rate
```

**With Phase 7**:

```
Query: { user(id: "uuid-1") { name } }  ← reads User:uuid-1
Mutation: updateUser(id: "uuid-2") → invalidates User:uuid-2 only
Result: Query cache stays valid (only affects uuid-2)
Impact: 90-95% cache hit rate
```

---

## Architecture in 30 Seconds

```
Mutation Execution
        ↓
    Response: { id: "uuid-123", name: "Bob", ... }
        ↓
    UUIDExtractor extracts: uuid-123
        ↓
    EntityKey: User:uuid-123
        ↓
    InvalidationContext: "invalidate User:uuid-123"
        ↓
    EntityDependencyTracker: "which caches depend on User:uuid-123?"
        ↓
    Selective Invalidation (only affected caches cleared)
        ↓
    Cache Hit Rate: 90-95% ✓
```

---

## 5 New Modules

### 1. UUID Extractor

- **File**: `fraiseql-core/src/cache/uuid_extractor.rs`
- **Purpose**: Parse mutation responses to extract entity UUIDs
- **Key Function**: `extract_entity_uuid(response, entity_type) → Option<String>`
- **Size**: ~150 lines
- **Tests**: 8

### 2. Entity Key

- **File**: `fraiseql-core/src/cache/entity_key.rs`
- **Purpose**: Type-safe representation of "EntityType:UUID"
- **Key Type**: `EntityKey { entity_type: String, entity_id: String }`
- **Size**: ~80 lines
- **Tests**: 6

### 3. Cascade Metadata

- **File**: `fraiseql-core/src/cache/cascade_metadata.rs`
- **Purpose**: Map mutations to entity types
- **Key Function**: `get_entity_type(mutation_name) → Option<&str>`
- **Size**: ~100 lines
- **Tests**: 5

### 4. Query Analyzer

- **File**: `fraiseql-core/src/cache/query_analyzer.rs`
- **Purpose**: Extract entity constraints from compiled queries
- **Key Output**: `QueryEntityProfile { entity_type, cardinality }`
- **Size**: ~200 lines
- **Tests**: 10

### 5. Entity Dependency Tracker

- **File**: `fraiseql-core/src/cache/entity_dependency_tracker.rs`
- **Purpose**: Track which caches depend on which entities
- **Key Function**: `get_affected_caches(entity) → Vec<cache_keys>`
- **Size**: ~300 lines
- **Tests**: 12

---

## Implementation Order

### Day 1-5: Foundation (Phase 7.1)

```bash
# Implement UUID extraction
cargo test -p fraiseql-core uuid_extractor

# Implement entity key
cargo test -p fraiseql-core entity_key

# Implement cascade metadata
cargo test -p fraiseql-core cascade_metadata

# Checkpoint: All 19 tests passing
```

### Day 6-10: Tracking (Phase 7.2)

```bash
# Implement query analyzer
cargo test -p fraiseql-core query_analyzer

# Implement entity dependency tracker
cargo test -p fraiseql-core entity_dependency_tracker

# Checkpoint: All 39 tests passing
```

### Day 11-15: Mutation Handling (Phase 7.3)

```bash
# Enhance executor with entity tracking
cargo test -p fraiseql-core executor

# Implement response tracker
cargo test -p fraiseql-core mutation_response_tracker

# Checkpoint: All 49 tests passing
```

### Day 16-20: Invalidation (Phase 7.4)

```bash
# Enhance invalidation system
cargo test -p fraiseql-core invalidation

# Integrate with adapter
cargo test -p fraiseql-core adapter

# Checkpoint: All 59 tests passing
```

### Day 21: Integration (Phase 7.5)

```bash
# Enable in server
cargo build -p fraiseql-server

# Run E2E tests
cargo test -p fraiseql-server entity_cache_e2e_test

# Checkpoint: 90%+ cache hit rate in production
```

---

## Code Pattern: UUID Extraction

```rust
// Pattern 1: Simple response with id field
Response: { id: "abc-123", name: "Alice" }
→ Extract: "abc-123"

// Pattern 2: Nested object
Response: { user: { id: "abc-123", name: "Alice" } }
→ Extract: "abc-123"

// Pattern 3: Array response (batch mutation)
Response: [{ id: "abc-123" }, { id: "def-456" }]
→ Extract: ["abc-123", "def-456"]

// Pattern 4: Null response
Response: null
→ Extract: None (no invalidation)
```

---

## Key Data Structures

### EntityKey

```rust
pub struct EntityKey {
    entity_type: String,  // "User", "Post", "Comment"
    entity_id: String,    // UUID: "550e8400-e29b-41d4-a716-446655440000"
}

// Serializes as: "User:550e8400-e29b-41d4-a716-446655440000"
```

### QueryEntityProfile

```rust
pub struct QueryEntityProfile {
    query_name: String,
    entity_type: Option<String>,  // None if list query
    cardinality: QueryCardinality, // Single/Multiple/List
}

// Single: WHERE id = ? → 1 entity (91% hit rate)
// Multiple: WHERE id IN (?, ...) → N entities (88% hit rate)
// List: no WHERE → all entities (60% hit rate)
```

### MutationResult

```rust
pub struct MutationResult {
    mutation_name: String,
    affected_entities: Vec<EntityKey>,  // [User:uuid-123]
    affected_views: Vec<String>,         // [v_user]
    response: serde_json::Value,
}
```

---

## Testing Checklist

### Unit Tests (61 total)

- [ ] UUID extractor: 8 tests
- [ ] Entity key: 6 tests
- [ ] Cascade metadata: 5 tests
- [ ] Query analyzer: 10 tests
- [ ] Entity dependency tracker: 12 tests
- [ ] Mutation response tracker: 10 tests
- [ ] Invalidation context: 10 tests

### Integration Tests (39 total)

- [ ] Entity cache E2E: 15 tests
- [ ] Cache coherency: 8 tests
- [ ] Performance: 6 tests
- [ ] Mutation tracking: 10 tests

### Performance Benchmarks

- [ ] UUID extraction: < 10µs
- [ ] Batch extraction: < 1ms for 100
- [ ] Query analysis: < 5µs
- [ ] Entity tracking: < 1µs
- [ ] Invalidation lookup: < 100µs

### Acceptance Criteria

- [ ] 95%+ code coverage
- [ ] 90-95% cache hit rate
- [ ] All tests passing
- [ ] Zero clippy warnings
- [ ] Documentation complete

---

## Performance Targets

| Operation | Target | Baseline |
|-----------|--------|----------|
| UUID extraction | < 10µs | N/A |
| Query analysis | < 5µs | N/A |
| Entity tracking | < 1µs | N/A |
| Cache hit rate | 90-95% | 60-80% |
| Throughput gain | 10-20% | baseline |
| Memory overhead | < 30% | N/A |

---

## Common Pitfalls to Avoid

### 1. UUID Parsing

❌ WRONG: String matching for UUID format
✅ RIGHT: Use uuid crate validation

### 2. Race Conditions

❌ WRONG: Mutable HashMap without locks
✅ RIGHT: RwLock<HashMap> for safe concurrent access

### 3. Memory Leaks

❌ WRONG: Unbounded growth of entity tracking maps
✅ RIGHT: Cleanup on cache entry eviction

### 4. False Negatives

❌ WRONG: Only track queried entities
✅ RIGHT: Also track entities in WHERE clauses

### 5. False Positives

❌ WRONG: Invalidate all queries reading same view
✅ RIGHT: Only invalidate queries reading specific entity

---

## Success Metrics

### Before Phase 7

```
Cache hit rate: 60-80%
Average latency: 100-200ms
Queries/sec: 50-100
```

### After Phase 7

```
Cache hit rate: 90-95%
Average latency: 50-75ms (50% reduction)
Queries/sec: 150-200 (2-3x improvement)
```

---

## Files Changed Summary

| File | Changes | Lines |
|------|---------|-------|
| NEW: uuid_extractor.rs | Create | 150 |
| NEW: entity_key.rs | Create | 80 |
| NEW: cascade_metadata.rs | Create | 100 |
| NEW: query_analyzer.rs | Create | 200 |
| NEW: entity_dependency_tracker.rs | Create | 300 |
| NEW: mutation_response_tracker.rs | Create | 150 |
| executor.rs | Enhance | +50 |
| invalidation.rs | Enhance | +80 |
| adapter.rs | Enhance | +60 |
| planner.rs | Enhance | +40 |
| cache/mod.rs | Export | +10 |
| **TOTAL** | | **~1220** |

---

## Verification Commands

```bash
# Run all Phase 7 tests
cargo test -p fraiseql-core entity_cache
cargo test -p fraiseql-server entity_cache_e2e_test

# Check code quality
cargo clippy -p fraiseql-core -- -D warnings
cargo fmt --check

# Run benchmarks
cargo bench -p fraiseql-core entity_cache

# Coverage report
cargo tarpaulin -p fraiseql-core -o Html
```

---

## Next Steps After Phase 7

**Phase 8: Coherency Validation**

- Audit logging for all cache operations
- Validation tests (no stale reads)
- Performance regression detection

**Phase 9: Optimization**

- Batch invalidation
- LRU eviction with entity tracking
- Memory pressure handling

**Phase 10+: Advanced**

- Distributed caching (Redis)
- Multi-tenant cache isolation
- Cache warming strategies
