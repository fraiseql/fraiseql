# Phase 7: Entity-Level Caching with UUID-Based Cascade Integration

## Executive Summary

Phase 7 implements entity-level cache invalidation leveraging GraphQL mutation return values. Instead of invalidating all queries reading a view when ANY entity changes, we extract UUIDs from mutation responses to invalidate only queries affecting specific entities.

**Expected Outcome**: 90-95% cache hit rate (vs current 60-80%), 10-20% throughput improvement

**Effort**: 3 weeks (240 hours)

**Risk Level**: Medium (requires careful UUID extraction and dependency tracking)

---

## Vision

Current cache invalidation is view-level:

```
mutation updateUser(id: "abc-123") -> User { id, name, email }
Result: ALL queries reading v_user invalidated
Problem: Query "{ user(id: "xyz-456") { name } }" doesn't need invalidation
```

Phase 7 enables entity-level invalidation:

```
mutation updateUser(id: "abc-123") -> User { id: "abc-123", ... }
Result: ONLY queries reading User:abc-123 invalidated
Benefit: Query about xyz-456 remains cached (95%+ hit rate)
```

---

## Architecture Overview

### Data Flow

```
GraphQL Mutation Query
    ↓
Executor (with UUID extraction)
    ↓ (extracts return value)
Mutation Response { entity_type: "User", entity_id: "uuid-123" }
    ↓
InvalidationContext (entity-aware)
    ↓
DependencyTracker (entity tracking)
    ↓
CachedDatabaseAdapter (selective invalidation)
    ↓
Cache Hit Rate: 90-95% ✓
```

### Key Components

1. **UUID Extractor** (new)
   - Parses mutation return values
   - Extracts entity UUIDs from responses
   - Handles nested entities and null cases

2. **Entity Dependency Tracker** (enhanced)
   - Current: `cache_key → [v_user, v_post]` (views)
   - New: `cache_key → [User:uuid-123, Post:uuid-456]` (entities)
   - Tracks which queries depend on which entity UUIDs

3. **Entity-Level InvalidationContext** (enhanced)
   - Current: `{ reason: Mutation { mutation_name: "updateUser" }, modified_views: ["v_user"] }`
   - New: `{ ..., modified_entities: [{ entity_type: "User", entity_id: "uuid-123" }] }`

4. **Query Plan Analyzer** (new)
   - Analyzes compiled query templates
   - Extracts entity constraints (WHERE id = ?, etc.)
   - Records which entities each query depends on

5. **Cascade Metadata Parser** (new)
   - Reads mutation definitions to find which entities are modified
   - Maps mutation return types to entity keys

---

## Implementation Plan

### Phase 7.1: Foundation - UUID Extraction & Schema Enhancement

**Duration**: 5 days (40 hours)

**Files to Create/Modify**:

- `fraiseql-core/src/cache/uuid_extractor.rs` (new)
- `fraiseql-core/src/cache/entity_key.rs` (new, replaces string "User:uuid")
- `fraiseql-core/src/cache/cascade_metadata.rs` (new)
- `fraiseql-core/src/cache/mod.rs` (export new modules)
- `fraiseql-core/src/compiler/ir.rs` (add entity tracking to IRMutation)

**Key Tasks**:

1. **Create UUID Extractor** (`uuid_extractor.rs`)

   ```rust
   pub struct UUIDExtractor;

   impl UUIDExtractor {
       /// Extract entity UUID from mutation response
       pub fn extract_entity_uuid(
           response: &serde_json::Value,
           entity_type: &str,  // e.g., "User"
       ) -> Result<Option<String>> {
           // Parse response: { id: "uuid-123", name: "Bob", ... }
           // Return: Some("uuid-123")
       }

       /// Extract multiple entity UUIDs (for batch mutations)
       pub fn extract_batch_uuids(
           response: &serde_json::Value,
           entity_type: &str,
       ) -> Result<Vec<String>> {
           // Handle array responses
       }

       /// Validate UUID format
       pub fn is_valid_uuid(id: &str) -> bool {
           // Check UUID v4 format
       }
   }
   ```

   **Test Coverage** (8 tests):
   - Extract single UUID from response
   - Extract UUID from nested object
   - Handle null response
   - Handle array of UUIDs
   - Validate UUID format
   - Skip non-UUID id fields
   - Batch mutations (multiple entities)
   - Error cases (missing id, invalid format)

2. **Create EntityKey Type** (`entity_key.rs`)

   ```rust
   #[derive(Debug, Clone, Eq, PartialEq, Hash)]
   pub struct EntityKey {
       pub entity_type: String,  // "User", "Post", etc.
       pub entity_id: String,     // UUID
   }

   impl EntityKey {
       pub fn new(entity_type: &str, entity_id: &str) -> Result<Self> {
           // Validate both are non-empty
           // Validate UUID format
       }

       pub fn to_cache_key(&self) -> String {
           format!("{}:{}", self.entity_type, self.entity_id)
       }
   }
   ```

   **Test Coverage** (6 tests):
   - Create valid entity key
   - Reject empty entity type
   - Reject invalid UUID
   - Serialize to cache key format
   - Deserialize from cache key format
   - Hash consistency for HashMap

3. **Create Cascade Metadata Module** (`cascade_metadata.rs`)

   ```rust
   pub struct CascadeMetadata {
       /// Mutation name → entity type it affects
       pub mutation_entity_map: HashMap<String, String>,
   }

   impl CascadeMetadata {
       /// Build from compiled schema
       pub fn from_schema(schema: &CompiledSchema) -> Self {
           // Extract all mutations
           // For each mutation, map to return_type
           // Example: "updateUser" → "User"
       }

       /// Get entity type for mutation
       pub fn get_entity_type(&self, mutation_name: &str) -> Option<&str> {
           self.mutation_entity_map.get(mutation_name).map(|s| s.as_str())
       }
   }
   ```

   **Test Coverage** (5 tests):
   - Build from schema
   - Map mutation to entity type
   - Handle unknown mutation
   - Multiple mutations same entity
   - Nested entity types

4. **Enhance IRMutation** (modify `compiler/ir.rs`)

   ```rust
   pub struct IRMutation {
       pub name: String,
       pub return_type: String,
       pub nullable: bool,
       pub arguments: Vec<IRArgument>,
       pub description: Option<String>,
       pub operation: MutationOperation,

       // NEW: Entity cascade metadata
       pub affected_views: Vec<String>,     // Which views this mutation affects
       pub affected_entities: Vec<String>,  // Which entity types (for future extension)
   }
   ```

**Acceptance Criteria**:

- UUID extraction handles 95%+ of real mutation responses
- EntityKey properly hashes/compares
- CascadeMetadata accurately maps mutations
- All edge cases tested (nulls, arrays, nested)

---

### Phase 7.2: Dependency Tracking - Query Analysis

**Duration**: 5 days (40 hours)

**Files to Create/Modify**:

- `fraiseql-core/src/cache/query_analyzer.rs` (new)
- `fraiseql-core/src/cache/entity_dependency_tracker.rs` (new)
- `fraiseql-core/src/runtime/planner.rs` (integrate analyzer)

**Key Tasks**:

1. **Create Query Analyzer** (`query_analyzer.rs`)

   ```rust
   pub struct QueryAnalyzer;

   impl QueryAnalyzer {
       /// Analyze compiled query to extract entity constraints
       pub fn analyze_query(
           query_def: &IRQuery,
           query_str: &str,
       ) -> Result<QueryEntityProfile> {
           // Parse WHERE clause for entity ID constraints
           // Example: WHERE id = ? → depends on single entity
           // Example: WHERE id IN (?, ?) → depends on multiple entities
       }
   }

   pub struct QueryEntityProfile {
       /// Query name
       pub query_name: String,

       /// Entity type this query filters on
       pub entity_type: Option<String>,  // "User" or None if listing

       /// How many entities does this typically return?
       pub cardinality: QueryCardinality,  // Single, Multiple, List
   }

   pub enum QueryCardinality {
       Single,     // WHERE id = ? → 1 entity (91% cache hits)
       Multiple,   // WHERE id IN (?, ...) → N entities (88% cache hits)
       List,       // WHERE 1=1 → all entities (60% cache hits)
   }
   ```

   **Test Coverage** (10 tests):
   - Parse WHERE id = ? constraint
   - Parse WHERE id IN (...) constraint
   - List queries (no entity constraint)
   - Nested entity queries
   - Complex WHERE clauses
   - Multiple WHERE conditions
   - Extract return type
   - Handle aggregate queries
   - Error cases
   - Cardinality classification

2. **Create Entity Dependency Tracker** (`entity_dependency_tracker.rs`)

   ```rust
   pub struct EntityDependencyTracker {
       /// cache_key → set of entity keys
       cache_to_entities: HashMap<String, HashSet<EntityKey>>,

       /// entity_key → set of cache keys (reverse mapping)
       entity_to_caches: HashMap<EntityKey, HashSet<String>>,
   }

   impl EntityDependencyTracker {
       pub fn record_entity_access(
           &mut self,
           cache_key: &str,
           entities: Vec<EntityKey>,
       ) {
           // Update both mappings
       }

       pub fn get_affected_caches(
           &self,
           entity: &EntityKey,
       ) -> Vec<String> {
           // Find all cache keys depending on this entity
       }

       pub fn get_entity_dependencies(
           &self,
           cache_key: &str,
       ) -> Vec<EntityKey> {
           // Find all entities this cache depends on
       }
   }
   ```

   **Test Coverage** (12 tests):
   - Record single entity access
   - Record multiple entities
   - Find affected caches by entity
   - Find dependencies by cache key
   - Update existing tracking
   - Remove cache entry
   - Bidirectional consistency
   - Multiple caches same entity
   - One cache multiple entities
   - Scale test (1000 caches, 100 entities)
   - Memory efficiency
   - Concurrent access safety

3. **Integrate with Query Planner** (modify `runtime/planner.rs`)

   ```rust
   pub struct QueryPlanner {
       // existing fields...

       /// Entity dependency information
       entity_analyzer: Arc<QueryAnalyzer>,
       entity_deps: Arc<RwLock<EntityDependencyTracker>>,
   }

   impl QueryPlanner {
       pub async fn plan_with_entity_tracking(
           &self,
           query_match: &QueryMatch,
       ) -> Result<(ExecutionPlan, Vec<EntityKey>)> {
           // Execute normal planning
           let plan = self.plan(query_match)?;

           // Analyze for entity constraints
           let entities = self.entity_analyzer.analyze_query(
               query_match.query_def,
               &plan.query_string,
           )?;

           // Record in dependency tracker
           let cache_key = plan.cache_key.clone();
           self.entity_deps.write().await.record_entity_access(
               &cache_key,
               entities.clone(),
           );

           Ok((plan, entities))
       }
   }
   ```

**Acceptance Criteria**:

- Query analyzer accurately identifies entity constraints
- Entity tracker maintains consistency (bidirectional)
- Cardinality classification matches actual query patterns
- 99% query coverage

---

### Phase 7.3: Mutation Execution - UUID Extraction & Response Tracking

**Duration**: 5 days (40 hours)

**Files to Create/Modify**:

- `fraiseql-core/src/runtime/executor.rs` (enhance mutation execution)
- `fraiseql-core/src/cache/mutation_response_tracker.rs` (new)
- `fraiseql-core/src/cache/mod.rs` (integrate)

**Key Tasks**:

1. **Create Mutation Response Tracker** (`mutation_response_tracker.rs`)

   ```rust
   pub struct MutationResponseTracker {
       /// Track which entities were modified by each mutation execution
       cascade_metadata: Arc<CascadeMetadata>,
       uuid_extractor: Arc<UUIDExtractor>,
   }

   pub struct MutationResult {
       /// Mutation name
       pub mutation_name: String,

       /// Affected entities (extracted from response)
       pub affected_entities: Vec<EntityKey>,

       /// Views affected (from schema)
       pub affected_views: Vec<String>,

       /// Response JSON
       pub response: serde_json::Value,
   }

   impl MutationResponseTracker {
       pub async fn track_mutation(
           &self,
           mutation_name: &str,
           response: &serde_json::Value,
       ) -> Result<MutationResult> {
           // Get entity type from cascade metadata
           let entity_type = self.cascade_metadata
               .get_entity_type(mutation_name)
               .ok_or_else(|| FraiseQLError::Validation {
                   message: format!("Unknown mutation: {}", mutation_name),
                   path: None,
               })?;

           // Extract UUIDs from response
           let affected_uuids = self.uuid_extractor.extract_entity_uuid(
               response,
               entity_type,
           )?;

           let affected_entities = affected_uuids
               .into_iter()
               .map(|id| EntityKey::new(entity_type, &id))
               .collect::<Result<Vec<_>>>()?;

           Ok(MutationResult {
               mutation_name: mutation_name.to_string(),
               affected_entities,
               affected_views: vec![],  // TODO: from schema
               response: response.clone(),
           })
       }
   }
   ```

   **Test Coverage** (10 tests):
   - Track single entity mutation
   - Track multi-entity mutation (batch)
   - Extract UUIDs from nested response
   - Handle null response
   - Validate mutation name
   - Populate affected entities
   - Error handling (invalid UUID)
   - Performance: 1000 mutations/sec
   - Memory: no leaks with large responses
   - Concurrent tracking

2. **Enhance Executor** (modify `runtime/executor.rs`)

   ```rust
   impl<A: DatabaseAdapter> Executor<A> {
       /// Execute mutation with entity tracking
       pub async fn execute_mutation_tracked(
           &self,
           mutation_name: &str,
           query: &str,
           variables: Option<&serde_json::Value>,
       ) -> Result<(String, MutationResult)> {
           // 1. Execute mutation (existing)
           let result_str = self.execute_mutation(mutation_name, query, variables).await?;
           let result_json: serde_json::Value = serde_json::from_str(&result_str)?;

           // 2. Track entity changes (NEW)
           let mut_result = self.response_tracker.track_mutation(
               mutation_name,
               &result_json,
           ).await?;

           // 3. Return both
           Ok((result_str, mut_result))
       }
   }
   ```

**Acceptance Criteria**:

- Extract UUIDs from 99%+ of mutation responses
- Handle batch mutations correctly
- Performance: < 1ms per mutation tracking
- Zero false positives (don't track wrong entities)

---

### Phase 7.4: Cache Invalidation - Entity-Aware

**Duration**: 5 days (40 hours)

**Files to Create/Modify**:

- `fraiseql-core/src/cache/invalidation.rs` (enhance)
- `fraiseql-core/src/cache/adapter.rs` (integrate entity tracking)

**Key Tasks**:

1. **Enhance InvalidationContext** (modify `invalidation.rs`)

   ```rust
   pub struct InvalidationContext {
       /// Views modified (existing)
       pub modified_views: Vec<String>,

       /// Entities modified (NEW)
       pub modified_entities: Vec<EntityKey>,

       pub reason: InvalidationReason,
   }

   impl InvalidationContext {
       /// Create for mutation with entity tracking
       pub fn for_entity_mutation(
           mutation_name: &str,
           modified_entities: Vec<EntityKey>,
           modified_views: Vec<String>,
       ) -> Self {
           Self {
               modified_views,
               modified_entities,
               reason: InvalidationReason::Mutation {
                   mutation_name: mutation_name.to_string(),
               },
           }
       }

       /// Check if this invalidation affects a specific cache
       pub fn affects_cache(
           &self,
           cache_key: &str,
           entity_deps: &EntityDependencyTracker,
       ) -> bool {
           // Check if any modified entities affect this cache
           for entity in &self.modified_entities {
               if entity_deps.affects_cache(cache_key, entity) {
                   return true;
               }
           }
           false
       }
   }
   ```

   **Test Coverage** (10 tests):
   - Create with entities
   - Create backward-compatible (views only)
   - Check cache affection
   - Multiple entities
   - No false positives
   - No false negatives
   - Log formatting
   - Serialization

2. **Enhance CachedDatabaseAdapter** (modify `adapter.rs`)

   ```rust
   pub struct CachedDatabaseAdapter<A: DatabaseAdapter> {
       // existing fields...
       entity_deps: Arc<RwLock<EntityDependencyTracker>>,
   }

   impl<A: DatabaseAdapter> CachedDatabaseAdapter<A> {
       pub async fn invalidate_by_entity(
           &self,
           context: &InvalidationContext,
       ) -> Result<()> {
           let mut cache = self.cache.write().await;

           for entity in &context.modified_entities {
               let affected_caches = self.entity_deps
                   .read()
                   .await
                   .get_affected_caches(entity);

               for cache_key in affected_caches {
                   cache.invalidate(&cache_key);
               }
           }

           Ok(())
       }
   }
   ```

**Acceptance Criteria**:

- Entity-aware invalidation reduces false invalidations by 40%+
- No cache coherency violations
- Performance: invalidate 1000 caches in < 1ms
- Zero memory leaks

---

### Phase 7.5: Integration & E2E Tests

**Duration**: 3 days (24 hours)

**Files to Create/Modify**:

- `fraiseql-server/src/main.rs` (enable entity caching)
- `fraiseql-server/tests/entity_cache_e2e_test.rs` (new)
- `fraiseql-core/src/cache/tests.rs` (comprehensive suite)

**Key Tasks**:

1. **Enable in Server** (modify `fraiseql-server/src/main.rs`)

   ```rust
   // Initialize cascade metadata from schema
   let cascade_metadata = CascadeMetadata::from_schema(&schema);

   // Create trackers
   let uuid_extractor = Arc::new(UUIDExtractor::new());
   let query_analyzer = Arc::new(QueryAnalyzer::new());
   let mut_response_tracker = Arc::new(
       MutationResponseTracker::new(cascade_metadata, uuid_extractor)
   );

   // Pass to executor
   let executor = Executor::with_entity_tracking(
       schema,
       Arc::new(db_adapter),
       query_analyzer,
       mut_response_tracker,
   );
   ```

2. **Create E2E Test Suite** (`entity_cache_e2e_test.rs`)

   ```rust
   #[tokio::test]
   async fn test_entity_level_cache_invalidation() {
       // 1. Cache query for User:1
       // 2. Cache query for User:2
       // 3. Mutate User:1
       // 4. Verify: User:1 cache invalidated, User:2 cache intact
   }

   #[tokio::test]
   async fn test_batch_mutation_entity_extraction() {
       // 1. Batch create 3 users
       // 2. Extract 3 UUIDs
       // 3. Invalidate all 3
   }

   #[tokio::test]
   async fn test_entity_cache_hit_rate() {
       // 1. Run 100 queries (varying entities)
       // 2. Perform 10 mutations
       // 3. Assert hit rate > 90%
   }
   ```

   **Test Coverage** (15 tests):
   - Single entity mutation
   - Batch mutations
   - Multiple entity types
   - Null entity handling
   - Cache hit rate > 90%
   - No false negatives
   - Performance under load
   - Concurrent mutations
   - Memory stability
   - Error recovery
   - Backward compatibility (view-based fallback)
   - List query handling
   - Nested entity queries
   - UUID validation
   - Cascade metadata accuracy

**Acceptance Criteria**:

- Entity caching enabled in server
- 90%+ cache hit rate in realistic workloads
- E2E tests pass
- No performance regression

---

## Testing Strategy

### Unit Tests (Per Module)

| Module | Tests | Focus |
|--------|-------|-------|
| uuid_extractor | 8 | UUID extraction accuracy |
| entity_key | 6 | Type safety, hashing |
| cascade_metadata | 5 | Mutation → entity mapping |
| query_analyzer | 10 | WHERE clause parsing |
| entity_dependency_tracker | 12 | Bidirectional tracking |
| mutation_response_tracker | 10 | Response parsing |
| invalidation | 10 | Entity-aware invalidation |

**Total Unit Tests**: 61

### Integration Tests

| Test Suite | Tests | Focus |
|-----------|-------|-------|
| entity_cache_e2e_test.rs | 15 | End-to-end caching |
| cache_coherency_test.rs | 8 | No stale reads |
| performance_test.rs | 6 | Hit rate, latency |
| mutation_tracking_test.rs | 10 | Accurate tracking |

**Total Integration Tests**: 39

**Total Coverage**: 100 tests, targeting 95%+ code coverage

### Performance Benchmarks

```bash
# UUID extraction performance
criterion_group!(benches,
    bench_extract_single_uuid,        # < 10µs
    bench_extract_batch_uuids,        # < 1ms for 100 entities
    bench_query_analysis,             # < 5µs per query
    bench_entity_tracking,            # < 1µs per record
    bench_invalidation_lookup,        # < 100µs for 1000 caches
);
```

---

## Risks & Mitigation

| Risk | Impact | Mitigation |
|------|--------|-----------|
| UUID extraction fails on mutation response | High | Comprehensive test coverage, fallback to view-level |
| Entity tracking memory grows unbounded | High | Automatic cleanup, TTL-based removal |
| Incorrect WHERE clause parsing | High | Conservative parsing, extensive tests |
| Concurrent access race conditions | Medium | RwLock protection, thorough testing |
| Cache coherency violations | Critical | Validation tests, audit logging |

---

## Success Criteria

1. **Functional**:
   - Entity-level invalidation working end-to-end
   - All 100 tests passing
   - Zero cache coherency violations

2. **Performance**:
   - Cache hit rate: 90-95% (vs 60-80% baseline)
   - Throughput improvement: 10-20%
   - UUID extraction: < 10µs per mutation
   - Query analysis: < 5µs per query

3. **Code Quality**:
   - 95%+ test coverage
   - Zero clippy warnings
   - All documentation complete

4. **Backward Compatibility**:
   - View-level caching still works
   - Gradual migration possible
   - No breaking changes to public API

---

## Deliverables

### Code

- `uuid_extractor.rs` (150 lines)
- `entity_key.rs` (80 lines)
- `cascade_metadata.rs` (100 lines)
- `query_analyzer.rs` (200 lines)
- `entity_dependency_tracker.rs` (300 lines)
- `mutation_response_tracker.rs` (150 lines)
- Enhancements to: `invalidation.rs`, `adapter.rs`, `executor.rs`, `planner.rs`

**Total New Code**: ~1000 lines

### Tests

- 61 unit tests
- 39 integration tests
- 6 performance benchmarks

### Documentation

- Architecture guide
- UUID extraction specification
- Entity tracking concepts
- Performance tuning guide
- Troubleshooting guide

---

## Timeline

| Week | Phase | Deliverables |
|------|-------|--------------|
| 1 | 7.1 | UUID extractor, EntityKey, CascadeMetadata |
| 1-2 | 7.2 | Query analyzer, Entity dependency tracker |
| 2 | 7.3 | Mutation tracking, Executor enhancement |
| 2-3 | 7.4 | Cache invalidation, Adapter integration |
| 3 | 7.5 | Server integration, E2E tests |

---

## Next Phase

**Phase 8: Coherency Validation & Audit Logging**

- Validate no stale reads occur
- Comprehensive audit trail for cache operations
- Performance regression testing
