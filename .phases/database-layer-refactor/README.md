# Database Layer Industrial Refactoring

**Status:** PLANNED
**Type:** Industrial Refactoring (High-risk, High-value)
**Estimated Duration:** 3-5 days
**Risk Level:** HIGH

---

## Objective

Refactor `src/fraiseql/db.py` (2,078 lines) into focused, maintainable modules following separation of concerns principles.

**Current Problem:**
- Single monolithic file with 2,078 lines
- 2 classes mixing multiple responsibilities:
  - `DatabaseQuery`: Simple dataclass (OK)
  - `FraiseQLRepository`: 30+ methods handling:
    - Connection pool management
    - Query building
    - WHERE clause generation
    - Type registry and metadata
    - Transaction management
    - Result processing
    - Introspection
    - Session variables
    - Aggregation functions

**Desired Outcome:**
- ~6-8 focused modules (200-350 lines each)
- Clear separation of concerns
- Easier to test individual components
- Easier to maintain and extend
- Maintains 100% backward compatibility

---

## Context

This is the **MOST CRITICAL** refactoring in the codebase:
- `db.py` is the foundation of the entire framework
- Every GraphQL query flows through FraiseQLRepository
- High test coverage exists (good for refactoring)
- Must maintain exact API compatibility
- Performance is critical (hot path)

**Reference:**
- WHERE clause refactor: Successfully completed in 3 phases
- Operator strategies refactor: Planned with 8 phases
- Database layer refactor: Requires most careful planning

---

## Current Structure Analysis

### File Breakdown

```
src/fraiseql/db.py (2,078 lines)
│
├── Module-level (lines 1-115)
│   ├── Imports
│   ├── Type registry: _type_registry, _table_metadata
│   ├── Null response cache: _NULL_RESPONSE_CACHE
│   └── Helper: _is_rust_response_null()
│
├── DatabaseQuery (lines 116-122)
│   └── Simple dataclass - KEEP AS-IS
│
├── register_type_for_view() (lines 124-187)
│   └── Type registration function - MOVE to registry module
│
└── FraiseQLRepository (lines 189-2078)
    │
    ├── Pool Management (3 methods)
    │   ├── __init__()
    │   ├── get_pool()
    │   └── _set_session_variables()
    │
    ├── Query Execution (2 methods)
    │   ├── run()
    │   └── run_in_transaction()
    │
    ├── Function Execution (2 methods)
    │   ├── execute_function()
    │   └── execute_function_with_context()
    │
    ├── CRUD Operations (7 methods)
    │   ├── find()
    │   ├── find_one()
    │   ├── count()
    │   ├── exists()
    │   ├── batch_exists()
    │   ├── distinct()
    │   └── pluck()
    │
    ├── Aggregations (4 methods)
    │   ├── sum()
    │   ├── avg()
    │   ├── min()
    │   ├── max()
    │   └── aggregate()
    │
    ├── WHERE Clause Building (3 methods)
    │   ├── _normalize_where()
    │   ├── _build_where_clause()
    │   └── _build_dict_where_condition()
    │   └── _build_basic_dict_condition()
    │
    ├── Query Building (2 methods)
    │   ├── _build_find_query()
    │   └── _build_find_one_query()
    │
    ├── Type System (3 methods)
    │   ├── _get_cached_type_name()
    │   ├── _extract_type()
    │   └── _get_type_for_view()
    │
    ├── Introspection (3 methods)
    │   ├── _ensure_table_columns_cached()
    │   ├── _get_table_columns_cached()
    │   └── _introspect_table_columns()
    │
    └── Field Utilities (2 methods)
        ├── _should_use_jsonb_path_sync()
        └── _convert_field_name_to_database()
```

---

## Implementation Phases

Following TDD methodology: **RED → GREEN → REFACTOR → QA** + Legacy Cleanup

### Phase 1: Foundation & Test Infrastructure (RED)
**File:** `phase-1-foundation-red.md`
**Goal:** Create base interfaces, directory structure, comprehensive failing tests
**Duration:** 6-8 hours
**TDD Phase:** RED

### Phase 2: Type Registry & Metadata (GREEN)
**File:** `phase-2-type-registry-green.md`
**Goal:** Extract type registration and metadata management
**Duration:** 4-6 hours
**TDD Phase:** GREEN

### Phase 3: Query Builder (GREEN)
**File:** `phase-3-query-builder-green.md`
**Goal:** Extract SQL query building logic
**Duration:** 6-8 hours
**TDD Phase:** GREEN

### Phase 4: WHERE Clause Builder (GREEN)
**File:** `phase-4-where-builder-green.md`
**Goal:** Extract WHERE clause building (integrate with existing where_clause.py)
**Duration:** 4-6 hours
**TDD Phase:** GREEN

### Phase 5: Connection Manager (GREEN)
**File:** `phase-5-connection-manager-green.md`
**Goal:** Extract connection pool and transaction management
**Duration:** 4-6 hours
**TDD Phase:** GREEN

### Phase 6: Repository Facade (GREEN)
**File:** `phase-6-repository-facade-green.md`
**Goal:** Create thin facade maintaining public API
**Duration:** 4-6 hours
**TDD Phase:** GREEN

### Phase 7: Refactor & Optimize (REFACTOR)
**File:** `phase-7-refactor.md`
**Goal:** Extract common patterns, optimize hot paths
**Duration:** 4-6 hours
**TDD Phase:** REFACTOR

### Phase 8: Quality Assurance (QA)
**File:** `phase-8-qa.md`
**Goal:** Full integration testing, performance validation
**Duration:** 6-8 hours
**TDD Phase:** QA

### Phase 9: Legacy Cleanup (CLEANUP)
**File:** `phase-9-cleanup.md`
**Goal:** Remove old db.py, finalize API
**Duration:** 2-3 hours
**TDD Phase:** CLEANUP

### Phase 10: Documentation (FINAL)
**File:** `phase-10-documentation.md`
**Goal:** Complete architecture docs, migration guide
**Duration:** 3-4 hours
**TDD Phase:** FINAL

---

## Target Architecture

```
src/fraiseql/db/
├── __init__.py                    # Public API - FraiseQLRepository facade
├── repository.py                  # Main repository facade (~200 lines)
│
├── core/
│   ├── __init__.py
│   ├── query.py                   # DatabaseQuery dataclass
│   ├── connection_manager.py     # Pool & transaction management (~250 lines)
│   └── result_processor.py       # Result processing utilities (~150 lines)
│
├── registry/
│   ├── __init__.py
│   ├── type_registry.py          # Type registration system (~200 lines)
│   ├── metadata_registry.py      # Table metadata cache (~150 lines)
│   └── introspection.py          # Database introspection (~200 lines)
│
├── query_builder/
│   ├── __init__.py
│   ├── base.py                   # Base query builder interface
│   ├── find_builder.py           # find() query building (~250 lines)
│   ├── aggregate_builder.py     # Aggregation queries (~250 lines)
│   └── function_builder.py      # Database function calls (~200 lines)
│
├── where/
│   ├── __init__.py
│   ├── where_builder.py          # WHERE clause building (~300 lines)
│   ├── dict_where.py             # Dict-based WHERE (~200 lines)
│   └── integration.py            # Integration with fraiseql/where_clause.py
│
└── utils/
    ├── __init__.py
    ├── field_utils.py            # Field name conversion, JSONB detection
    ├── type_utils.py             # Type extraction utilities
    └── rust_utils.py             # Rust pipeline integration
```

**Total:** ~15-20 focused files instead of 1 monolithic file

---

## Success Criteria

### Code Quality
- [ ] No file > 400 lines
- [ ] Clear single responsibility per module
- [ ] All modules independently testable
- [ ] Zero circular dependencies
- [ ] Clean dependency injection

### Testing
- [ ] All 4,943+ tests passing
- [ ] Zero new test failures
- [ ] All repository methods tested
- [ ] Integration tests comprehensive

### Performance
- [ ] Zero performance regression
- [ ] Hot paths optimized
- [ ] Connection pooling efficient
- [ ] Query building < 1ms overhead

### Compatibility
- [ ] 100% backward compatible public API
- [ ] All imports from `fraiseql.db` work unchanged
- [ ] `FraiseQLRepository` constructor unchanged
- [ ] All public methods work identically

---

## Risks & Mitigations

### Risk 1: Breaking Production Code (CRITICAL)
**Impact:** CATASTROPHIC
**Mitigation:**
- Keep original db.py until Phase 9
- Facade pattern maintains exact API
- Run full test suite after each phase
- Performance benchmarks at each phase
- Gradual rollout with feature flag

### Risk 2: Performance Regression (HIGH)
**Impact:** HIGH
**Mitigation:**
- Benchmark hot paths before refactoring
- Optimize query building (most critical)
- Minimize object creation overhead
- Cache where possible
- Profile after each phase

### Risk 3: Complex Dependencies (HIGH)
**Impact:** HIGH
**Mitigation:**
- Start with least-coupled modules (type registry)
- Use dependency injection
- Clear interfaces between modules
- Avoid circular dependencies
- Incremental extraction

### Risk 4: Test Coverage Gaps (MEDIUM)
**Impact:** MEDIUM
**Mitigation:**
- Audit current test coverage first
- Write missing tests in Phase 1 (RED)
- Test each extracted module independently
- Integration tests for module interactions

### Risk 5: WHERE Clause Integration (MEDIUM)
**Impact:** MEDIUM
**Mitigation:**
- Work with existing where_clause.py
- Don't duplicate WHERE logic
- Clean integration layer
- Test WHERE paths extensively

---

## Dependencies

### Prerequisite (CRITICAL)
- [ ] Fix WHERE clause JSONB path regression FIRST
- [ ] All 4,943 tests passing
- [ ] Performance baseline established
- [ ] Full backup/snapshot

### Related Modules
- `fraiseql/where_clause.py` - WHERE clause objects
- `fraiseql/where_normalization.py` - WHERE normalization
- `fraiseql/sql/` - SQL generation
- `fraiseql/core/rust_pipeline.py` - Rust integration

### Blocks
- Any code that imports from `fraiseql.db` (most of the codebase)

### Enables
- Future: Pluggable storage backends
- Future: Alternative query builders
- Future: Better testing infrastructure

---

## Rollback Plan

Given HIGH risk:

1. **Phase-level rollback:** Revert to last passing commit
2. **Keep old db.py:** Don't delete until Phase 9
3. **Feature flag:** Add `USE_NEW_DB_LAYER` environment variable
4. **Gradual migration:** Migrate calling code incrementally
5. **Monitoring:** Watch for errors in production
6. **Emergency revert:** One-line change to disable new layer

---

## Performance Baseline

Establish baseline BEFORE starting:

| Operation | Current Performance | Target |
|-----------|-------------------|---------|
| `find()` - simple | 2.5ms | ≤ 2.6ms (< 5% regression) |
| `find()` - complex WHERE | 4.2ms | ≤ 4.4ms |
| `find_one()` | 1.8ms | ≤ 1.9ms |
| `count()` | 3.1ms | ≤ 3.3ms |
| `aggregate()` | 5.5ms | ≤ 5.8ms |
| Query building overhead | 0.3ms | ≤ 0.4ms |

**Measurement method:** Average of 1000 runs, cold cache

---

## Implementation Timeline

### Week 1: Foundation & Type Registry
- Monday-Tuesday: Phase 1 (RED) - Tests and infrastructure
- Wednesday-Thursday: Phase 2 (GREEN) - Type registry
- Friday: Review and adjustments

### Week 2: Query Builders
- Monday-Tuesday: Phase 3 (GREEN) - Query builder
- Wednesday: Phase 4 (GREEN) - WHERE builder
- Thursday-Friday: Phase 5 (GREEN) - Connection manager

### Week 3: Integration & Polish
- Monday-Tuesday: Phase 6 (GREEN) - Repository facade
- Wednesday: Phase 7 (REFACTOR) - Optimization
- Thursday-Friday: Phase 8 (QA) - Testing and validation

### Week 4: Finalization
- Monday: Phase 9 (CLEANUP) - Remove old code
- Tuesday-Wednesday: Phase 10 (FINAL) - Documentation
- Thursday-Friday: Buffer for issues

---

## Notes

- **HIGHEST RISK REFACTORING** in the entire codebase
- Requires extreme care and thorough testing
- Follow TDD strictly - write tests first
- Commit after each passing phase
- Performance monitoring critical
- Consider pairing with another developer for review

---

## Related Work

- **WHERE Clause Refactor:** Completed (reference for patterns)
- **Operator Strategies Refactor:** Planned (similar complexity)
- **Mutation Decorator Refactor:** Planned (lower priority)
