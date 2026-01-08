# FraiseQL v2.0 Refactoring Roadmap

**Status**: Phase 4 Analysis Complete
**Date**: January 8, 2026
**Goal**: Address large files and improve code organization

---

## Executive Summary

The codebase has identified **1 critical large file** and **20+ large test files** that exceed size guidelines. This document outlines the refactoring strategy with priorities and timelines.

### Size Violations Identified

| File | Lines | Limit | Status | Priority |
|------|-------|-------|--------|----------|
| `src/fraiseql/db.py` | 2,418 | 1,500 | **CRITICAL** | Phase 4 |
| `src/fraiseql/fastapi/routers.py` | 1,404 | 1,500 | Warning | Phase 5+ |
| `tests/unit/db/test_db_utility_methods.py` | 929 | 500 | Large | Future |
| `tests/regression/issue_124/...edge_cases.py` | 831 | 500 | Large | Future |
| `tests/integration/caching/...cache_integration.py` | 774 | 500 | Large | Future |

---

## Phase 4: Core Module Refactoring

### Priority 1: src/fraiseql/db.py (2,418 lines)

**Current Structure**:
```
db.py
├── Imports (14 lines)
├── Global constants & registries (~100 lines)
│   ├── _type_registry: dict
│   ├── _table_metadata: dict
│   ├── _NULL_RESPONSE_CACHE: set
│   └── Feature flags & Rust imports
├── Utility functions (~70 lines)
│   └── _is_rust_response_null(response)
├── DatabaseQuery class (~50 lines)
├── Helper functions (~100 lines)
│   └── register_type_for_view()
├── FraiseQLRepository class (~1,800 lines)
│   ├── __init__
│   ├── Type caching (_get_cached_type_name)
│   ├── Session management (_set_session_variables)
│   ├── Where clause building (_build_find_query, _build_dict_where_condition)
│   ├── Query execution (execute, find, find_one, etc.)
│   ├── Rust integration (execute_via_rust_pipeline)
│   └── Type introspection (_get_type_for_view)
└── Pool factory functions (~300 lines)
    ├── create_production_pool()
    ├── create_prototype_pool()
    └── create_legacy_pool()
```

**Refactoring Plan**:

#### Option A: Extract into Submodules (Recommended)

```
db/
├── __init__.py (imports & exports)
├── repository.py (~900 lines) - FraiseQLRepository class
├── query_builder.py (~300 lines) - Query building methods
├── rust_handler.py (~200 lines) - Rust pipeline integration
├── session.py (~150 lines) - Session variable management
├── pool.py (~300 lines) - Pool factory functions
├── registry.py (~200 lines) - Type registry & metadata
└── utils.py (~150 lines) - Helper functions
```

**Benefits**:
- ✅ Each module has single responsibility
- ✅ Easier to test and maintain
- ✅ Clear separation of concerns
- ✅ Backward compatible (just import from `db/__init__.py`)

**Implementation Steps**:

1. **Create db/ package** (Week 1)
   ```bash
   mkdir -p src/fraiseql/db
   touch src/fraiseql/db/__init__.py
   ```

2. **Extract modules** (Week 1-2)
   - Move `FraiseQLRepository` to `repository.py`
   - Move query building methods to `query_builder.py`
   - Move Rust integration to `rust_handler.py`
   - Move session management to `session.py`
   - Move pool factories to `pool.py`
   - Move registries to `registry.py`
   - Keep utilities in `utils.py`

3. **Update imports** (Week 2)
   ```python
   # db/__init__.py
   from fraiseql.db.registry import _type_registry, _table_metadata
   from fraiseql.db.repository import FraiseQLRepository
   from fraiseql.db.pool import create_production_pool, create_prototype_pool
   from fraiseql.db.utils import register_type_for_view

   __all__ = [
       "FraiseQLRepository",
       "create_production_pool",
       "create_prototype_pool",
       "register_type_for_view",
   ]
   ```

4. **Test & verify** (Week 2-3)
   - Run full test suite
   - Verify no import changes needed by users
   - Update internal imports as needed

5. **Document** (Week 3)
   - Create `db/STRUCTURE.md` explaining modules
   - Update `docs/ORGANIZATION.md` to reference new structure

---

## Phase 5: FastAPI Router Refactoring

### Priority 2: src/fraiseql/fastapi/routers.py (1,404 lines)

**Current Structure**:
```
routers.py
├── GraphQLRouter class (~1,400 lines)
│   ├── Route registration
│   ├── Query execution
│   ├── Mutation execution
│   ├── Subscription handling
│   ├── Introspection
│   ├── Middleware integration
│   └── Error handling
```

**Refactoring Strategy**:

Extract into:
- `graphql_query_route.py` (~300 lines) - Query handling
- `graphql_mutation_route.py` (~250 lines) - Mutation handling
- `graphql_subscription_route.py` (~200 lines) - Subscription handling
- `graphql_introspection_route.py` (~150 lines) - Introspection
- `graphql_router.py` (~500 lines) - Main router

**Timeline**: Phase 5 (Weeks 4-5)

---

## Test File Refactoring

### Large Test Files (Future Priority)

Current large test files exceed 500-line limit:
- `tests/unit/db/test_db_utility_methods.py` (929 lines)
- `tests/regression/issue_124/test_where_clause_edge_cases.py` (831 lines)
- `tests/integration/caching/test_pg_fraiseql_cache_integration.py` (774 lines)

**Strategy**:
1. Identify logical test groups within each file
2. Split into focused test modules (e.g., `test_db_utility_methods_connection.py`, `test_db_utility_methods_transactions.py`)
3. Keep fixtures/helpers in separate `conftest.py` if needed

**Timeline**: Phase 6+ (after core modules stabilize)

---

## Implementation Timeline

### Phase 4 (Weeks 1-3): Core Module Refactoring
- [ ] Week 1: Create db/ package, extract modules
- [ ] Week 2: Update imports, fix breakages
- [ ] Week 3: Test, document, commit

### Phase 5 (Weeks 4-5): FastAPI Router Refactoring
- [ ] Week 1-2: Extract router methods into modules
- [ ] Week 3: Test, document, commit

### Phase 6+ (Future): Test Refactoring
- [ ] Large test file splitting
- [ ] Test fixture consolidation
- [ ] Test organization by concern

---

## Success Criteria

✅ **Phase 4 Complete When**:
- [ ] `db.py` refactored into db/ package with max 200 lines in any file
- [ ] All imports updated and working
- [ ] Full test suite passes (5,991+ tests)
- [ ] No breaking changes to public API
- [ ] `db/STRUCTURE.md` created
- [ ] Commit with clear message

✅ **Phase 5 Complete When**:
- [ ] `routers.py` refactored with max 500 lines in any file
- [ ] All imports updated and working
- [ ] FastAPI integration tests pass
- [ ] No breaking changes to public API
- [ ] Documentation updated

---

## Risk Mitigation

**Risks**:
1. **Import cycles** - Carefully plan import order
2. **Circular dependencies** - Use dependency injection
3. **Test breakage** - Run tests after each module extraction
4. **User breakage** - Maintain public API exports in `__init__.py`

**Mitigation**:
- Create db/ package with proper `__init__.py` exports
- Use typing and interfaces to decouple modules
- Run tests frequently during refactoring
- Document changes in release notes

---

## Files Affected

**Primary**:
- `src/fraiseql/db.py` → `src/fraiseql/db/`

**Secondary**:
- `src/fraiseql/fastapi/routers.py` → `src/fraiseql/fastapi/` (submodules)

**Documentation**:
- `docs/ORGANIZATION.md` (update)
- `docs/REFACTORING_ROADMAP.md` (this file)
- `src/fraiseql/db/STRUCTURE.md` (new)
- `src/fraiseql/fastapi/STRUCTURE.md` (new)

---

## Decision Record

**Decision**: Refactor db.py into db/ package (Option A)

**Rationale**:
- Cleanest separation of concerns
- Backward compatible with current imports
- Easier to test individual components
- Clear module responsibilities
- Reduces cognitive load on developers

**Approved**: January 8, 2026

---

## Related Documentation

- `docs/ORGANIZATION.md` - Overall codebase structure
- `docs/CODE_ORGANIZATION_STANDARDS.md` - Standards being enforced
- `scripts/check_file_sizes.py` - Size validation script

---

**Next Steps**:
1. Review and approve refactoring plan
2. Begin Phase 4 implementation
3. Create db/ package structure
4. Extract modules one at a time
5. Test and commit changes
