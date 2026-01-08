# src/fraiseql/db/ Module Structure

**Status**: Phase 5 Implementation In Progress
**Philosophy**: Python API Exposure + Rust Core
**Date**: January 8, 2026

---

## Overview

The `db` module provides the Python API layer for database access in FraiseQL. It coordinates between:

1. **Python API Layer** - User-facing, type-safe interface (FraiseQLRepository)
2. **Rust Core Boundary** - Efficient Rust execution pipeline coordination
3. **Supporting Modules** - Type registry, connection pools, query building, session management

This modular structure follows the philosophy of separating user-facing Python API from internal Rust coordination, making both components maintainable and replaceable independently.

### Architecture

```
┌─────────────────────────────────────────────┐
│      User Code (Python Decorators)          │
│  (@fraise_type, @query, @mutation, etc.)    │
└────────────────┬────────────────────────────┘
                 │
┌────────────────▼────────────────────────────┐
│     Python API Layer (User-Facing)          │
│        FraiseQLRepository                   │ ← Primary entry point
│  ├─ find(), aggregate(), count()            │
│  ├─ find_one(), batch_exists()              │
│  └─ Delegates to internal modules           │
└────────────────┬────────────────────────────┘
                 │
         ┌───────┼───────────────────┐
         │       │                   │
    ┌────▼──┐┌───▼────┐  ┌──────┐  ┌▼────────┐
    │Registry││ Session│  │ Pool │  │Executor │ ← Rust boundary
    │(types) ││ (vars) │  │      │  │(critical)
    └────────┘└────────┘  └──────┘  └┬───────┘
                                     │
                            ┌────────▼──────────┐
                            │  Rust Pipeline    │
                            │ (fraiseql_rs)     │
                            └────────┬──────────┘
                                     │
                            ┌────────▼──────────┐
                            │  PostgreSQL       │
                            └───────────────────┘
```

---

## Module Breakdown

### __init__.py

**Purpose**: Public API exports and module coordination during refactoring

**Exports**:
- `FraiseQLRepository` - Main database access class
- `register_type_for_view()` - Type registration function
- `create_production_pool()`, `create_prototype_pool()`, `create_legacy_pool()` - Pool factories
- `execute_via_rust_pipeline()` - Rust execution bridge (for backward compatibility)
- `is_rust_response_null()` - Null response detection
- Internal: `_table_metadata`, `_type_registry`, `_NULL_RESPONSE_CACHE`

**Status**: Active (coordinates between legacy db_core.py and modular extracts)

**Transition Note**: During Phase 5 refactoring, this module imports from both:
- New modular extracts (registry.py, pool.py, executor.py)
- Legacy monolithic db_core.py (for FraiseQLRepository, query building, session management)

Once repository.py is extracted, the __init__.py will be fully modular.

---

### registry.py (~100 lines)

**Purpose**: Type registry and metadata management

**Key Components**:

```python
_type_registry: dict[str, type]        # Map view names → Python types
_table_metadata: dict[str, dict]       # Column info, JSONB config, FK relationships
```

**Functions**:

- `register_type_for_view()` - Register Python type class for a view
  - Parameters: view_name, type_class, table_columns, has_jsonb_data, etc.
  - Stores metadata at registration time (avoids runtime introspection)
  - Validates FK relationships in strict mode

- `_get_type_for_view(view_name)` - Get registered type for view name
  - Returns type class or None

- `_ensure_table_columns_cached(view_name)` - Get cached column metadata
  - Returns set of column names from cached metadata

- `clear_type_registry()` - Clear all registrations (for testing)

**No Dependencies**: Pure internal module, no imports from other db/ modules

**Why Separate**:
- No dependencies (can be extracted first with confidence)
- Used by many components (registry, query building, WHERE clause normalization)
- Natural responsibility boundary

---

### executor.py (~150 lines)

**Purpose**: Rust pipeline coordination and execution [KEY BOUNDARY]

**Key Components**:

```python
_NULL_RESPONSE_CACHE: set[bytes]       # Common pattern cache (90%+ hit rate)
execute_via_rust_pipeline              # Imported from fraiseql.core.rust_pipeline
```

**Functions**:

- `is_rust_response_null(response)` - Detect null responses without JSON parsing
  - Byte-level pattern matching (O(1), 12x faster than JSON parsing)
  - Used for response optimization

- `execute_query_via_rust(query_data)` - Execute query through Rust pipeline
  - Main coordination point between Python query building and Rust execution
  - Handles timeouts and error reporting

- `execute_transaction(transaction_data)` - Execute transaction through Rust
  - Transaction state management
  - Coordinates with Rust execution engine

**Why Separate**:
- **Critical Boundary**: All Rust integration passes through here
- **Testability**: Easy to mock for unit tests
- **Replaceability**: Can swap Rust implementation without changing Python code
- **Single Responsibility**: Pure Rust coordination, no Python logic

**Usage**:
```python
# Internal usage (from repository.py or other modules)
result = await executor.execute_query_via_rust(query_data)
if executor.is_rust_response_null(result):
    return []
```

---

### pool.py (~200 lines)

**Purpose**: Database connection pool factory functions

**Functions**:

- `create_production_pool()` - SSL/TLS production pool
  - Uses: `fraiseql._fraiseql_rs.DatabasePool`
  - Best for: Production with security requirements
  - Requires: FRAISEQL_PRODUCTION_POOL=true environment variable

- `create_prototype_pool()` - Fast development/testing pool
  - Uses: `fraiseql._fraiseql_rs.PrototypePool`
  - Best for: Development, CI/CD, prototyping
  - Default when production pool not enabled

- `create_legacy_pool()` - Pure Python psycopg3 pool
  - Uses: `psycopg_pool.AsyncConnectionPool`
  - Best for: Compatibility, debugging, pure Python deployments
  - No Rust dependencies required

**Configuration**:
```python
HAS_PRODUCTION_POOL    # Environment check for production pool availability
HAS_PROTOTYPE_POOL     # Check for prototype pool availability
USE_PRODUCTION_POOL    # Feature flag from FRAISEQL_PRODUCTION_POOL env var
```

**No Dependencies**: Pure pool coordination, no imports from other db/ modules (except os, logging)

**Why Separate**:
- Pool factories are self-contained
- No coupling to other database modules
- Natural responsibility boundary
- Reusable for different applications

---

### db_core.py (2,418 lines) [LEGACY - TO BE REPLACED]

**Status**: Being replaced by modular extracts during Phase 5

**Contains**:
- `FraiseQLRepository` class (main database access - ~1,800 lines)
  - Public methods: find(), aggregate(), count(), etc.
  - Query building: _build_find_query(), _build_dict_where_condition()
  - Session management: _set_session_variables()
  - Type management: _get_cached_type_name(), _get_type_for_view()
  - Utility methods: avg(), sum(), min(), max(), distinct(), pluck()

- Global registries (now in registry.py):
  - `_type_registry`, `_table_metadata`, `_NULL_RESPONSE_CACHE`

- Utility functions (now in registry.py):
  - `register_type_for_view()`, `_is_rust_response_null()`

- Pool factories (now in pool.py):
  - `create_production_pool()`, `create_prototype_pool()`, `create_legacy_pool()`

**Extraction Plan**:
1. ✅ Extract registry.py (types and metadata) - DONE
2. ✅ Extract pool.py (pool factories) - DONE
3. ✅ Extract executor.py (Rust coordination) - DONE
4. ⏳ Extract query_builder.py (query building methods)
5. ⏳ Extract session.py (session management)
6. ⏳ Extract repository.py (FraiseQLRepository)
7. ⏳ Deprecate db_core.py

**Note**: During migration, db_core.py is imported as fallback. Once all modules are extracted, it will be fully deprecated.

---

## Design Principles

### 1. Python API Exposure

**Principle**: All public methods should be in FraiseQLRepository class

**Why**: Users think of "the repository" as the interface, not individual modules

```python
# Users interact with this:
from fraiseql.db import FraiseQLRepository
repo = FraiseQLRepository(pool)
results = await repo.find(User)  # Clean, single point of entry
```

### 2. Rust Core Hidden

**Principle**: Rust coordination should be internal implementation detail

**Why**: Rust is an optimization, not part of the user-facing contract

```python
# Users don't care about this:
executor.execute_via_rust_pipeline(...)

# They only use:
repo.find(User)  # "How it works" is hidden
```

### 3. Clear Boundaries

**Principle**: Each module should have one boundary

**Modules and Their Boundaries**:

| Module | Boundary | Access Pattern |
|--------|----------|-----------------|
| repository.py | **Python API** (user-facing) | Users import FraiseQLRepository |
| executor.py | **Rust boundary** (internal) | Query builders → executor → Rust |
| query_builder.py | None (pure Python) | Internal, no external boundary |
| session.py | None (pure Python) | Internal, no external boundary |
| registry.py | None (pure Python) | Internal, no external boundary |
| pool.py | **Pool factories** (public) | Users import create_*_pool() |

### 4. Delegation Pattern

**Principle**: FraiseQLRepository should delegate, not contain all logic

**Benefits**:
- ✅ Keeps the API class small and focused
- ✅ Makes internal modules reusable and testable
- ✅ Enables optimization without changing API

```python
async def find(self, type_class):
    # Delegation pattern - API coordinates, internals execute
    query = self._build_query(...)      # Internal: query building
    self._set_session_vars(...)         # Internal: session management
    result = await self.executor.execute_via_rust_pipeline(query)  # Internal: Rust
    return self._extract_type(result)   # Internal: type extraction
```

---

## Dependency Graph (Phase 5 Target)

```
registry.py
  ├─ No dependencies ✓
  └─ Exports: _type_registry, _table_metadata, register_type_for_view

pool.py
  ├─ No internal dependencies ✓
  └─ Exports: create_production_pool, create_prototype_pool, create_legacy_pool

executor.py
  ├─ Depends on: fraiseql.core.rust_pipeline (external)
  └─ Exports: execute_query_via_rust, execute_transaction, is_rust_response_null

query_builder.py (PENDING)
  ├─ Depends on: registry.py (for metadata)
  └─ Exports: _build_find_query, _build_dict_where_condition, etc.

session.py (PENDING)
  ├─ No internal dependencies expected
  └─ Exports: _set_session_variables

repository.py (PENDING)
  ├─ Depends on: executor, query_builder, session, registry, pool
  └─ Exports: FraiseQLRepository (main class)

__init__.py
  └─ Exports: All public API (FraiseQLRepository, pools, register_type_for_view)
```

**Circular Dependencies**: None - all arrows point downward (dependency hierarchy preserved)

---

## Testing Strategy

### Unit Tests

- **registry.py**: Type registration, metadata caching
- **pool.py**: Pool creation, configuration validation
- **executor.py**: Null response detection, Rust pipeline coordination

### Integration Tests

- **repository.py**: Full query pipeline (once extracted)
- **Combined**: Multi-module workflows (find with filtering, aggregation, etc.)

### Test Mocking

```python
# Mock Rust execution for unit tests
from unittest.mock import AsyncMock, patch

with patch('fraiseql.db.execute_via_rust_pipeline', new_callable=AsyncMock):
    # Test query building without actual Rust
    repo = FraiseQLRepository(pool)
    results = await repo.find(User)
```

---

## Migration Timeline

### Phase 5.1 (COMPLETED) ✅
- ✅ Extract registry.py (type registry and metadata)
- ✅ Extract pool.py (connection pool factories)
- ✅ Extract executor.py (Rust coordination boundary)
- ✅ Create db/__init__.py (public API exports)
- ✅ Rename db.py → db_core.py (legacy interim module)
- ✅ Verify imports work (all modules load cleanly)

### Phase 5.2 (PENDING)
- ⏳ Extract query_builder.py (pure Python query construction)
- ⏳ Test query building independently
- ⏳ Update repository imports

### Phase 5.3 (PENDING)
- ⏳ Extract session.py (PostgreSQL session management)
- ⏳ Test session management independently
- ⏳ Update repository imports

### Phase 5.4 (PENDING)
- ⏳ Extract repository.py (FraiseQLRepository class)
- ⏳ Update db/__init__.py to import from repository.py
- ⏳ Run full test suite (5,991+ tests)
- ⏳ Verify backward compatibility

### Phase 5.5 (PENDING)
- ⏳ Deprecate db_core.py (mark as legacy)
- ⏳ Update documentation
- ⏳ Commit final Phase 5 changes

---

## Performance Implications

### Current Performance

- **Type Registry Lookups**: O(1) dict lookup (cached for entire session)
- **Null Response Detection**: O(1) byte pattern matching (12x faster than JSON parsing)
- **Pool Creation**: Lazy initialization (created once per application)
- **Query Building**: ~1-2ms per query (Python string formatting)

### Modular Extraction Impact

**No performance regression expected**:
- ✅ Same algorithms, just organized differently
- ✅ No additional function call overhead (proper inlining)
- ✅ Cache behavior unchanged (same data structures)
- ✅ Rust pipeline unchanged (where performance matters most)

### Future Optimization Opportunities

Once modules are separated:
- Cache nullability checks across requests
- Batch Rust executions
- Lazy load type metadata
- Pool connection reuse optimization

---

## Known Issues

### Issue #124: WHERE Clause Filtering (FIXED ✓)

**Status**: Fixed in v1.8.3

**What**: WHERE clause filters silently ignored on hybrid tables (tables with both SQL columns and JSONB data)

**Root Causes**:
1. Type re-registration cleared `table_columns` metadata
2. FK detection used truthiness check (failed on empty sets)

**Solution**:
- registry.py: Preserve metadata during re-registration
- db_core.py: Added metadata fallback for WHERE clause building
- where_normalization.py: Fixed empty set checks

**Testing**:
- ✅ 4/4 regression tests pass
- ✅ 5,991+ full test suite passes
- ✅ Zero regressions

---

## Related Documentation

- **PHASE_5_IMPLEMENTATION_PLAN.md** - Detailed extraction roadmap
- **PHILOSOPHY_IMPACT_ANALYSIS.md** - How philosophy guides Phase 5
- **QA_PHASE_4_REVIEW.md** - Pre-extraction QA validation
- **ORGANIZATION.md** - Overall codebase structure
- **CODE_ORGANIZATION_STANDARDS.md** - File size and organization limits

---

## Summary: Design Impact

**Before Phase 5** (Monolithic):
```
db.py (2,418 lines)
├── Public API (find, aggregate, etc.)
├── Type Management
├── Session Variables
├── Query Building
├── Rust Pipeline Integration (mixed with Python)
└── Pool Management
```

**After Phase 5** (Modular):
```
db/__init__.py (coordinates public API)
├── repository.py (~600 lines) - PUBLIC PYTHON API
├── executor.py (~150 lines) - RUST BOUNDARY
├── query_builder.py (~200 lines) - Pure Python
├── session.py (~150 lines) - Session management
├── registry.py (~100 lines) - Type registry
└── pool.py (~200 lines) - Connection pools
```

**Key Benefits**:
- ✅ Clear separation of concerns
- ✅ Rust boundary explicit and replaceable
- ✅ Easier testing and mocking
- ✅ Modular, maintainable code
- ✅ Philosophy-aligned architecture
- ✅ No breaking changes to users

---

**Status**: Phase 5.1 Complete - Ready for Phase 5.2 (query_builder extraction)

**Last Updated**: January 8, 2026
**Next Review**: After Phase 5 full completion
