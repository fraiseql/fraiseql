# Philosophy Impact Analysis: Python API + Rust Core

**Date**: January 8, 2026
**Impact Level**: HIGH - Changes Phase 5 refactoring approach
**Scope**: Core module refactoring strategy

---

## Summary

The FraiseQL philosophy of **"Python API Exposure + Rust Core"** significantly impacts Phase 5 refactoring goals. Instead of mechanically splitting code, Phase 5 should **strategically separate the Python-facing API from Rust-internal coordination**.

### Before Philosophy Integration
```
db.py (2,418 lines)
├── Public API (find, aggregate, etc.)
├── Type Management
├── Session Variables
├── Query Building
├── Rust Pipeline Integration  ← Mixed with Python logic
└── Pool Management
```

### After Philosophy Integration
```
db/ package
├── repository.py (600 lines) - PUBLIC PYTHON API
│   └── find(), aggregate(), count()
│   └── Delegates to: executor, query_builder, session, registry
│
├── executor.py (300 lines) - RUST COORDINATION [KEY CHANGE]
│   └── execute_via_rust_pipeline()
│   └── Rust response handling
│   └── Rust transaction management
│
├── query_builder.py (200 lines) - PURE PYTHON
├── session.py (150 lines) - POSTGRES SESSION VARS
├── registry.py (200 lines) - TYPE MANAGEMENT
└── pool.py (300 lines) - CONNECTION POOLS
```

---

## Philosophy Definition

### Core Principle

**"Python API Exposure + Rust Core"** means:

1. **Python API Exposure**
   - Users interact with pure Python API (FraiseQLRepository)
   - Python types, Python methods, Python patterns
   - No Rust code visible to users
   - Framework agnostic (FastAPI, Starlette, custom)

2. **Rust Core**
   - Heavy lifting done in Rust (JSON transformation, query execution)
   - 7-10x faster than Python equivalent
   - Hidden behind Python API
   - Framework-independent pipeline

### Architecture
```
┌─────────────────────────────────────────┐
│        User Code (Python)               │
│    (@fraise_type, @query, @mutation)    │
└──────────────┬──────────────────────────┘
               │
┌──────────────▼──────────────────────────┐
│      PYTHON API LAYER                   │
│  ┌────────────────────────────────────┐ │
│  │ FraiseQLRepository                 │ │
│  │  - find()                          │ │
│  │  - aggregate()                     │ │
│  │  - count()                         │ │
│  │  - Type management                 │ │
│  │  - Delegates to modules below      │ │
│  └────────────────────────────────────┘ │
└──────────────┬──────────────────────────┘
               │
┌──────────────▼──────────────────────────┐
│  RUST CORE COORDINATION                 │
│  ┌────────────────────────────────────┐ │
│  │ executor.py                        │ │
│  │  - execute_via_rust_pipeline()     │ │
│  │  - Rust response handling          │ │
│  │  - Transaction management          │ │
│  └────────────────────────────────────┘ │
└──────────────┬──────────────────────────┘
               │
┌──────────────▼──────────────────────────┐
│    RUST EXECUTION ENGINE                │
│   (fraiseql_rs compiled extension)      │
└──────────────┬──────────────────────────┘
               │
┌──────────────▼──────────────────────────┐
│       PostgreSQL Database               │
└─────────────────────────────────────────┘
```

---

## Phase 5 Original Plan vs Philosophy-Aware Plan

### Original Phase 4 Roadmap
Distribution of methods was based on **what they do**:

| Module | Purpose | Methods |
|--------|---------|---------|
| query_builder.py | Query building | _build_find_query, _build_dict_where_condition |
| session.py | Session vars | _set_session_variables |
| rust_handler.py | Rust integration | execute_via_rust_pipeline |
| repository.py | Remaining | find, aggregate, type management |

**Problem**: This splits the user-facing API across modules, making the boundary unclear.

### Philosophy-Aware Phase 5 Plan
Distribution is based on **user vs. internal coordination**:

| Module | Purpose | Responsibility |
|--------|---------|-----------------|
| repository.py | PUBLIC API | User-facing methods stay here; delegates to internal modules |
| executor.py | RUST BOUNDARY | All Rust pipeline coordination in one place |
| query_builder.py | Pure Python | Query construction, independent of Rust |
| session.py | Postgres | Session management, independent of Rust |
| registry.py | Type System | Type metadata, independent of Rust |
| pool.py | Pools | Connection management, independent of Rust |

**Benefit**: Clear layering - users see Python API, internals coordinate with Rust.

---

## Key Differences

### 1. FraiseQLRepository Remains Public API

**Original Plan**:
```python
# Might distribute public methods across modules
from fraiseql.db.repository import FraiseQLRepository  # ✓ main class
from fraiseql.db.query_builder import find  # ✗ confusing split
```

**Philosophy-Aware Plan**:
```python
# All public API stays in repository.py
from fraiseql.db import FraiseQLRepository

repo = FraiseQLRepository(pool)
results = await repo.find(User)  # Clear, single point

# Internally, find() delegates to query_builder, executor, etc.
```

✅ **Impact**: Users don't see the internal split. Clean Python API.

### 2. Rust Coordination Isolated in executor.py

**Original Plan** (as "rust_handler.py"):
```python
# Rust pipeline in its own module, but unclear if it's critical
# Could be confused with other handlers
def execute_via_rust_pipeline(...): ...
def _is_rust_response_null(...): ...
```

**Philosophy-Aware Plan** (as "executor.py"):
```python
# Explicitly marks this as the RUST BOUNDARY
# All Rust integration in one place
# Clear name indicates execution coordination

class RustExecutor:
    def execute_via_rust_pipeline(...): ...
    def _is_rust_response_null(...): ...
    def _parse_rust_response(...): ...
```

✅ **Impact**: Rust boundary is explicit and replaceable. Good for testing/mocking.

### 3. Public vs. Internal Methods Clear

**Original Plan**:
```python
class FraiseQLRepository:
    async def find(self): ...  # Public
    async def aggregate(self): ...  # Public
    async def _build_find_query(self): ...  # Private, but where?

# Is _build_find_query in repository.py or query_builder.py?
# Users shouldn't need to know.
```

**Philosophy-Aware Plan**:
```python
class FraiseQLRepository:
    async def find(self):
        query = self._build_query_and_execute(...)
        return results

    async def aggregate(self): ...

    async def _build_query_and_execute(self):
        # Delegates to executor, query_builder, session
        query = query_builder.build_find_query(...)
        result = await executor.execute_via_rust_pipeline(query)
        return result

# Implementation details hidden. API is clean.
```

✅ **Impact**: Clear delegation pattern. Easy to understand data flow.

---

## Philosophy-Driven Design Principles

### Principle 1: Python API Exposure
**All public methods should be in FraiseQLRepository class.**

Why? Users think of "the repository" as the interface, not individual modules.

```python
# Users expect:
from fraiseql.db import FraiseQLRepository

# Not:
from fraiseql.db.query_builder import find
from fraiseql.db.repository import FraiseQLRepository
```

### Principle 2: Rust Core Hidden
**Rust coordination should be internal implementation detail.**

Why? Rust is an optimization, not part of the user-facing contract.

```python
# Users shouldn't care about this:
executor.execute_via_rust_pipeline(...)

# They should only use:
repo.find(User)  # "How it works" is hidden
```

### Principle 3: Clear Boundaries
**Each module should have one boundary:**
- repository.py: Python API boundary (user-facing)
- executor.py: Rust boundary (internal)
- query_builder.py: No external boundary (pure Python)

Why? Clear boundaries make code easier to test and replace.

### Principle 4: Delegation Pattern
**FraiseQLRepository should delegate, not contain all logic.**

Why? Keeps the API class small and focused.

```python
# Instead of 1,800 lines of logic in FraiseQLRepository:
async def find(self, type_class):
    return await executor.execute_query(...)

# Executor handles Rust coordination
# Repository handles API contract
```

---

## How Philosophy Changes Phase 5 Goals

### Original Phase 4 Goals
1. ✓ Reduce db.py from 2,418 to <1,500 lines
2. ✓ Extract methods into focused modules
3. ✓ Improve code maintainability
4. ✓ Keep tests passing

### Philosophy-Aware Phase 5 Goals
1. ✓ Reduce db.py from 2,418 to <1,500 lines
2. ✓ Extract methods into focused modules
3. ✓ **Separate Python API from Rust coordination** (NEW)
4. ✓ **Make Rust boundary explicit and replaceable** (NEW)
5. ✓ **Improve code maintainability**
6. ✓ **Keep tests passing**

**Added Goals**:
- Define clear Python API boundary (repository.py)
- Define clear Rust coordination boundary (executor.py)
- Enable future Rust optimizations without Python changes
- Enable easy testing/mocking of Rust integration

---

## Implementation Impact

### Module Sizes (Philosophy-Aware)

| Module | Purpose | Lines | Notes |
|--------|---------|-------|-------|
| repository.py | Python API | ~600 | Users interact here |
| executor.py | Rust bridge | ~300 | Internal implementation |
| query_builder.py | Query construction | ~200 | Pure Python, testable |
| session.py | Session vars | ~150 | Postgres integration |
| registry.py | Type management | ~200 | Type system |
| pool.py | Connection pools | ~300 | Pool factories |

**Total**: ~1,750 lines (was 2,418)
**Largest module**: 600 lines (was 1,800)

### Testing Impact

**Benefits of philosophy-aware refactoring**:

1. **Easier Unit Testing**
   ```python
   # Can test executor independently
   executor = RustExecutor()
   result = executor.execute_via_rust_pipeline(query)

   # Can mock executor for repository tests
   repo = FraiseQLRepository(pool, executor=MockExecutor())
   ```

2. **Clearer Dependency Flow**
   ```python
   # repository.py depends on:
   #   - executor (for Rust)
   #   - query_builder (for queries)
   #   - session (for session vars)
   #   - registry (for types)
   # Each of those is independent
   ```

3. **Easier Future Optimizations**
   ```python
   # If we want to optimize executor, we can:
   # - Replace with faster Rust bridge
   # - Add caching layer
   # - Mock for testing
   # Without touching repository.py public API
   ```

---

## Risks Mitigated by Philosophy Awareness

### Risk: Users confused by module split
**Mitigation**: Repository.py keeps all public API. Users don't see split.

### Risk: Rust coordination scattered
**Mitigation**: executor.py concentrates all Rust integration.

### Risk: Rust pipeline hard to test/replace
**Mitigation**: executor.py is isolated and mockable.

### Risk: Future Rust changes affect Python API
**Mitigation**: executor.py as boundary isolates changes.

---

## Examples: Philosophy in Action

### Example 1: Adding a new public method

**Before**: Where does it go? Into which module?
```python
async def new_method(self, ...):
    # Should this go in repository.py or query_builder.py?
```

**After**: Clear answer - always repository.py
```python
class FraiseQLRepository:
    async def new_method(self, ...):
        # Delegate to appropriate internal modules
        result = await executor.do_something(...)
        return result
```

### Example 2: Optimizing Rust integration

**Before**: Rust code scattered, hard to optimize
```python
def execute_find():
    # Rust pipeline called from many places
    # Hard to optimize centrally
    ...

def execute_aggregate():
    # Rust pipeline called again
    # Similar patterns scattered
    ...
```

**After**: Rust integration in executor.py, easy to optimize
```python
class RustExecutor:
    async def execute_via_rust_pipeline(self, query):
        # All Rust optimization in one place
        # Can add caching, batching, etc. here
        # No impact on repository.py API
        ...
```

### Example 3: Testing repository without Rust

**Before**: Hard to test repository without actually calling Rust
```python
def test_find():
    # Actually needs Rust to work
    repo = FraiseQLRepository(pool)
    # Can't easily mock Rust part
```

**After**: Easy to mock executor
```python
def test_find():
    # Can mock the Rust executor
    executor_mock = Mock()
    executor_mock.execute_via_rust_pipeline.return_value = {...}

    repo = FraiseQLRepository(pool, executor=executor_mock)
    result = await repo.find(User)

    # Test pure Python logic without Rust
```

---

## Summary: Philosophy Impact on Phase 5

| Aspect | Original Plan | Philosophy-Aware Plan | Impact |
|--------|---------------|----------------------|--------|
| FraiseQLRepository | May split across modules | Stays in repository.py | ✅ Clear public API |
| Rust integration | In "rust_handler.py" | In "executor.py" (explicit boundary) | ✅ Clear Rust boundary |
| User-facing API | Users see module split | Users see clean API | ✅ Better UX |
| Testing | Hard to mock Rust | Easy to mock executor | ✅ Testability |
| Future changes | Changes scattered | Changes in executor.py | ✅ Maintainability |

---

## Conclusion

**Philosophy "Python API + Rust Core" changes Phase 5 from a mechanical code split into a strategic architectural refactoring.**

Instead of:
> "Let's split db.py into smaller modules"

We're doing:
> "Let's separate the Python API layer from the Rust coordination layer while maintaining backward compatibility"

This approach:
- ✅ Preserves the Python API contract
- ✅ Makes Rust integration explicit and replaceable
- ✅ Improves testability
- ✅ Enables future optimizations
- ✅ Clarifies architectural boundaries

**Phase 5 is now guided by philosophy, not just mechanics.**
