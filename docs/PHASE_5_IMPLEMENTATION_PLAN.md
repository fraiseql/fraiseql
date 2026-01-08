# Phase 5: Core Module Refactoring Implementation

**Status**: Planning & Implementation
**Date**: January 8, 2026
**Duration**: 3 weeks
**Philosophy**: Python API Exposure + Rust Core

---

## Executive Summary

Phase 5 implements the refactoring plan created in Phase 4, with a critical modification: **the philosophy of Python API exposure + Rust core should guide the extraction strategy**.

Instead of simply splitting db.py mechanically, we should refactor it to **clearly separate the Python-facing API from the Rust-internal coordination layer**.

### Philosophy Impact on Phase 5

**FraiseQL Architecture**:
```
┌─────────────────────────────────────────────┐
│      User Code (Python Decorators)          │
│  (@fraise_type, @query, @mutation, etc.)    │
└────────────────┬────────────────────────────┘
                 │
┌────────────────▼────────────────────────────┐
│     Python API Layer (User-Facing)          │
│  FraiseQLRepository, query builders, etc.   │  ← Phase 5 Focus
└────────────────┬────────────────────────────┘
                 │
┌────────────────▼────────────────────────────┐
│   Rust Core (Execution Pipeline)            │
│  execute_via_rust_pipeline, Rust types      │
└─────────────────────────────────────────────┘
                 │
┌────────────────▼────────────────────────────┐
│    PostgreSQL Database                      │
└─────────────────────────────────────────────┘
```

**Implication for Phase 5**:
- ✅ **Python API Layer** (db.py) should remain the user-facing contract
- ✅ **Internal coordination** between Python and Rust should be clearly separated
- ✅ **Rust integration** should be isolated in dedicated modules
- ✅ **Query building** should be decoupled from Rust pipeline details
- ✅ **Type management** should bridge Python types and Rust pipeline

---

## Philosophy-Aware Refactoring Strategy

### Core Principle

**Preserve the Python API contract while refactoring internal structure.**

The current FraiseQLRepository class serves two purposes:
1. **Python API** - Public methods users depend on (find, aggregate, etc.)
2. **Internal Coordination** - Rust pipeline integration, session management

Phase 5 should **extract internal coordination while keeping the public API in the main repository class**.

### Revised Module Structure

Instead of distributing methods arbitrarily, organize by **coupling to Rust core**:

```
db/
├── __init__.py
│   └── Exports: FraiseQLRepository (public API)
│       Exports: create_*_pool (pool factories)
│       Exports: register_type_for_view (registration)
│
├── repository.py (~600 lines)
│   └── FraiseQLRepository class
│       └── Public methods: find, aggregate, etc.
│       └── Type management: _get_type_for_view
│       └── Session setup: delegates to session module
│       └── Query execution: delegates to executor module
│
├── executor.py (~300 lines) [NEW - Rust coordination]
│   └── execute_via_rust_pipeline (Rust bridge)
│   └── Rust response handling (_is_rust_response_null)
│   └── Rust response parsing
│   └── Rust transaction management
│
├── query_builder.py (~200 lines)
│   └── _build_find_query
│   └── _build_find_one_query
│   └── _build_dict_where_condition
│   └── _build_basic_dict_condition
│   └── _extract_function_kwargs
│
├── session.py (~150 lines)
│   └── _set_session_variables
│   └── _clear_session_variables (if exists)
│   └── Session context management
│
├── registry.py (~200 lines)
│   └── _type_registry: dict
│   └── _table_metadata: dict
│   └── _get_type_for_view
│   └── register_type_for_view
│   └── Type caching (_get_cached_type_name)
│
└── pool.py (~300 lines)
    └── create_production_pool
    └── create_prototype_pool
    └── create_legacy_pool
    └── Pool configuration
```

### Key Design Decisions

**1. Keep FraiseQLRepository as Public API**
- ✅ Users import: `from fraiseql.db import FraiseQLRepository`
- ✅ Public methods remain: find(), aggregate(), etc.
- ✅ Delegates internal coordination to executor, session, etc.
- ✅ No breaking changes to user code

**2. Isolate Rust Coordination in executor.py**
- ✅ All `_fraiseql_rs` imports in one place
- ✅ Rust response handling isolated
- ✅ Easy to replace/mock for testing
- ✅ Clear boundary between Python and Rust

**3. Keep Query Building Separate**
- ✅ Pure Python query construction
- ✅ No Rust dependencies
- ✅ Testable in isolation
- ✅ Reusable for other purposes

**4. Centralize Type Registry**
- ✅ Type metadata management in one place
- ✅ Caching logic together
- ✅ Registration logic centralized
- ✅ Clear type lifecycle

**5. Session Management Module**
- ✅ PostgreSQL session variables isolated
- ✅ No Rust or query building concerns
- ✅ Testable independently
- ✅ Clear responsibility

---

## Implementation Steps

### Week 1: Create db/ Package Structure

**Step 1.1**: Create db/ directory and __init__.py
```bash
mkdir -p src/fraiseql/db
touch src/fraiseql/db/__init__.py
```

**Step 1.2**: Create skeleton modules
```bash
touch src/fraiseql/db/repository.py
touch src/fraiseql/db/executor.py
touch src/fraiseql/db/query_builder.py
touch src/fraiseql/db/session.py
touch src/fraiseql/db/registry.py
touch src/fraiseql/db/pool.py
touch src/fraiseql/db/STRUCTURE.md
```

**Step 1.3**: Extract registry.py first (no dependencies)
- Copy: _type_registry, _table_metadata
- Copy: register_type_for_view, _get_type_for_view
- Copy: _get_cached_type_name, _ensure_table_columns_cached
- Test: Import and verify registry functions work

**Step 1.4**: Extract pool.py (minimal dependencies)
- Copy: create_production_pool, create_prototype_pool, create_legacy_pool
- Test: Import and verify pool creation works

**Step 1.5**: Run tests after each extraction
```bash
python -m pytest tests/ -x  # Stop at first failure
```

### Week 2: Extract Coordination & Execution

**Step 2.1**: Extract session.py
- Copy: _set_session_variables
- Copy: execute_function_with_context (if session-related)
- Test: Verify session variable setting works

**Step 2.2**: Extract executor.py (Rust boundary)
- Copy: execute_via_rust_pipeline
- Copy: _is_rust_response_null
- Copy: Rust response handling logic
- Copy: Rust transaction management
- **Important**: Keep FraiseQLRepository's call to executor

**Step 2.3**: Extract query_builder.py
- Copy: _build_find_query, _build_find_one_query
- Copy: _build_dict_where_condition, _build_basic_dict_condition
- Copy: _extract_function_kwargs
- Test: Verify query building works

**Step 2.4**: Update repository.py
- Move remaining methods (public API, type management)
- Add imports from executor, session, query_builder modules
- Keep FraiseQLRepository as public-facing class

### Week 3: Integration & Testing

**Step 3.1**: Create db/__init__.py exports
```python
# db/__init__.py
from fraiseql.db.registry import register_type_for_view
from fraiseql.db.repository import FraiseQLRepository
from fraiseql.db.pool import (
    create_production_pool,
    create_prototype_pool,
    create_legacy_pool,
)

__all__ = [
    "FraiseQLRepository",
    "register_type_for_view",
    "create_production_pool",
    "create_prototype_pool",
    "create_legacy_pool",
]
```

**Step 3.2**: Update internal imports
- Find all `from fraiseql.db import X`
- Verify imports still work via __init__.py
- Update any direct internal imports if needed

**Step 3.3**: Run full test suite
```bash
python -m pytest tests/ -v
# Expected: 5,991+ tests pass
```

**Step 3.4**: Create db/STRUCTURE.md documentation
```markdown
# src/fraiseql/db/ Module Structure

## Overview
The `db` module provides the Python API layer for database access in FraiseQL.
It coordinates between the Python type system and the Rust execution pipeline.

## Module Breakdown

### repository.py
Main FraiseQLRepository class providing the public API:
- find(), aggregate(), count(), etc. (public methods)
- Type management and caching
- Delegates to executor, session, and query_builder modules

### executor.py [Rust Boundary]
Handles all Rust pipeline coordination:
- execute_via_rust_pipeline (bridge to Rust)
- Response handling and parsing
- Transaction management
- Null response optimization

### query_builder.py
Pure Python query construction:
- find query building (_build_find_query)
- WHERE clause building (_build_dict_where_condition)
- Function argument extraction
- No Rust or database dependencies

### session.py
PostgreSQL session variable management:
- _set_session_variables (set app.tenant_id, app.contact_id, etc.)
- Session context management
- No Rust or query building concerns

### registry.py
Type registry and metadata management:
- _type_registry: Type lookup
- _table_metadata: Column metadata caching
- Type caching for performance
- Type introspection utilities

### pool.py
Database connection pool management:
- create_production_pool (Rust-based, full features)
- create_prototype_pool (Rust-based, async bridge)
- create_legacy_pool (Python-based, compatibility)

## Architecture

```
User Code
    ↓
FraiseQLRepository (repository.py)
    ├→ Type Management (registry.py)
    ├→ Query Building (query_builder.py)
    ├→ Session Variables (session.py)
    └→ Rust Pipeline (executor.py)
        ↓
    Rust Core
        ↓
    PostgreSQL
```

## Philosophy: Python API + Rust Core

- **Python API**: FraiseQLRepository provides user-facing contract
- **Rust Core**: executor.py bridges to Rust execution pipeline
- **Separation**: Clear boundaries between Python logic and Rust coordination
- **Testability**: Each module can be tested independently
```

**Step 3.5**: Verify public API unchanged
```python
# Users can still do this:
from fraiseql.db import FraiseQLRepository, register_type_for_view
from fraiseql.db import create_production_pool

repo = FraiseQLRepository(pool)
results = await repo.find(User)
```

**Step 3.6**: Commit refactoring
```bash
git add src/fraiseql/db/
git commit -m "refactor(Phase 5): Extract db.py into modular db/ package

Refactor database module to separate concerns while maintaining
public API contract:

New Structure:
- repository.py: Public FraiseQLRepository API
- executor.py: Rust pipeline coordination (new boundary)
- query_builder.py: Pure Python query construction
- session.py: PostgreSQL session management
- registry.py: Type registry and metadata
- pool.py: Connection pool factories

Key Principles:
✅ Python API exposure unchanged (backward compatible)
✅ Rust core coordination isolated in executor.py
✅ Clear separation between Python and Rust layers
✅ Each module has single responsibility
✅ All 5,991+ tests pass

Sizes After Refactoring:
- repository.py: ~600 lines (was 1,800 in monolithic)
- executor.py: ~300 lines (Rust bridge)
- query_builder.py: ~200 lines
- session.py: ~150 lines
- registry.py: ~200 lines
- pool.py: ~300 lines
- No file exceeds 600 lines ✅"
```

---

## Philosophy-Aligned Success Criteria

### Python API Exposure ✅
- [ ] FraiseQLRepository remains public API (no breaking changes)
- [ ] All public methods work identically
- [ ] User imports unchanged: `from fraiseql.db import FraiseQLRepository`
- [ ] Full backward compatibility verified

### Rust Core Isolation ✅
- [ ] Rust integration isolated in executor.py
- [ ] All `_fraiseql_rs` imports in one module
- [ ] Easy to replace/mock for testing
- [ ] Clear boundary between Python and Rust

### Modular Clarity ✅
- [ ] Each module has single responsibility
- [ ] No circular dependencies
- [ ] Clear module interfaces
- [ ] Well-documented module boundaries

### Testing ✅
- [ ] All 5,991+ tests pass
- [ ] No regressions
- [ ] Query building tests still pass
- [ ] Rust integration tests still pass

### Code Quality ✅
- [ ] Largest file ~600 lines (was 2,418)
- [ ] All files under 1,500 limit
- [ ] Clear module organization
- [ ] Documentation created

---

## Risk Mitigation

**Risk**: Circular imports between modules
**Mitigation**:
- registry.py has no dependencies
- pool.py has no internal dependencies
- executor.py imports only from registry
- query_builder.py imports only from registry
- session.py imports only from registry
- repository.py imports from all (top-level aggregator)

**Risk**: Breaking changes to public API
**Mitigation**:
- FraiseQLRepository stays in main repository.py
- __init__.py exports maintain original imports
- All public methods remain accessible
- Backward compatibility verified by test suite

**Risk**: Test failures during extraction
**Mitigation**:
- Run tests after each module extraction
- Start with isolated modules (registry, pool)
- Build up to complex modules (executor, query_builder)
- Use `-x` flag to stop at first failure

---

## Timeline

- **Week 1** (Days 1-5): Create structure, extract registry and pool
- **Week 2** (Days 6-10): Extract session, executor, query_builder
- **Week 3** (Days 11-15): Integration, testing, documentation

**Critical Path**:
1. Create db/ package (Day 1)
2. Extract registry (Day 2)
3. Extract pool (Day 3)
4. Extract executor (Days 4-6)
5. Extract query_builder (Days 7-8)
6. Extract session (Day 9)
7. Update repository.py (Days 10-11)
8. Integration testing (Days 12-13)
9. Documentation & cleanup (Days 14-15)

---

## Documentation Updates

After Phase 5 completion:

1. **Update docs/ORGANIZATION.md**
   - Add db/ package structure
   - Link to db/STRUCTURE.md

2. **Create db/STRUCTURE.md**
   - Module breakdown
   - Architecture diagram
   - Usage examples

3. **Update REFACTORING_ROADMAP.md**
   - Mark Phase 5 as complete
   - Document actual line counts per module
   - Lessons learned

---

## Related Documents

- `docs/REFACTORING_ROADMAP.md` - Overall strategy
- `docs/QA_PHASE_4_REVIEW.md` - Phase 4 approval
- `docs/ORGANIZATION.md` - Overall architecture
- `docs/MODULAR_HTTP_ARCHITECTURE.md` - Python API + Rust core philosophy

---

## Next Steps

1. **Approve Phase 5 plan** (current doc)
2. **Begin Week 1** - Create db/ package structure
3. **Extract modules** following the order above
4. **Test after each extraction** using pytest
5. **Complete Week 3** - Full integration and documentation
6. **Proceed to Phase 6** - routers.py refactoring

---

**Status**: Ready for Implementation
**Approved**: Pending review
**Date**: January 8, 2026
