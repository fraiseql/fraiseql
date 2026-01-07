# Phase D.4: App Factory with Explicit Registration

**Status**: ✅ COMPLETE
**Tests**: 15 focused tests, 280 total axum tests passing
**Commits**: 2 (initial + refactor)
**Decision**: Explicit registration is better than zero-config

## Overview

Phase D.4 completes the registry system (Phase D) by integrating explicit registration into the app factory. Initial implementation included zero-config auto-discovery, but architectural review led to **removing it in favor of explicit registration** - a better approach for type-safe GraphQL.

This completes the Phase D (Registry System) implementation:
- **D.1**: AxumRegistry foundation (20 tests) ✅
- **D.2**: Auto-discovery system (17 tests) ✅ (for internal use)
- **D.3**: Registration hooks in decorators (11 tests) ✅
- **D.4**: App factory explicit registration (15 tests) ✅

## What Was Implemented

### 1. Registry Integration in App Factory

Added optional `registry` parameter for advanced testing:

```python
def create_axum_fraiseql_app(
    *,
    # ... existing parameters ...
    types: list[type[Any]] | None = None,
    queries: list[type[Any]] | None = None,
    mutations: list[type[Any]] | None = None,
    subscriptions: list[type[Any]] | None = None,
    registry: AxumRegistry | None = None,
    **kwargs,
) -> AxumServer:
```

**Parameter**:
- `registry` (AxumRegistry, default: None)
  - Optional custom registry instance for advanced testing
  - Defaults to singleton if not provided
  - Allows registry isolation in test suites

### 2. Registration Flow

The app factory:

1. **Initializes registry** (uses provided or singleton)
2. **Builds configuration** (from parameters or config object)
3. **Creates AxumServer** with registry
4. **Registers explicit items**:
   - Types, queries, mutations, subscriptions
   - Each registration goes to both server and centralized registry
   - Clear and declarative approach

### 3. AxumServer Updates

Modified `AxumServer` class to:

- Accept optional `registry` parameter in `__init__`
- Store registry as `self._registry`
- Register items to **both** local dict and central registry
- Expose registry via new `get_registry()` method

```python
class AxumServer:
    def __init__(
        self,
        config: AxumFraiseQLConfig,
        registry: AxumRegistry | None = None,
    ):
        self._registry = registry or AxumRegistry.get_instance()
```

## Usage Pattern: Explicit Registration (Recommended)

```python
from fraiseql.axum import create_axum_fraiseql_app

# Clear, auditable schema with explicit registration
app = create_axum_fraiseql_app(
    database_url="postgresql://user:pass@localhost/db",
    types=[User, Post, Comment],
    queries=[get_users, get_posts],
    mutations=[create_user, delete_post],
    subscriptions=[on_user_created],
)

app.start(host="0.0.0.0", port=8000)
```

**Why explicit is better:**
- ✅ **Clear** - Exactly what's in the schema
- ✅ **Fast** - No discovery overhead
- ✅ **Debuggable** - Can trace what's in schema
- ✅ **Testable** - Deterministic, no magic
- ✅ **Secure** - Explicit whitelist
- ✅ **Maintainable** - See full API at a glance

## Advanced Pattern: Custom Registry for Testing

```python
from fraiseql.axum import AxumRegistry, create_axum_fraiseql_app

# Use custom registry for isolated testing
test_registry = AxumRegistry()
test_registry.clear()

app = create_axum_fraiseql_app(
    database_url="postgresql://test/db",
    types=[TestUser],
    registry=test_registry,
)

# test_registry has only TestUser, singleton is unaffected
```

## Test Coverage

**15 focused tests** in `tests/unit/axum/test_app_factory_discovery.py`:

### App Factory Basics (4 tests)
- Create app without registry parameter ✓
- Create app with explicit types ✓
- Register to centralized registry ✓
- Use custom registry for testing ✓

### Explicit Registration (5 tests)
- Explicit types work correctly ✓
- Explicit queries work correctly ✓
- Explicit mutations work correctly ✓
- Explicit subscriptions work correctly ✓
- Multiple categories combined ✓

### Error Handling (2 tests)
- Missing database_url raises error ✓
- Database URL from kwargs works ✓

### Registry Integration (4 tests)
- Types registered via registry ✓
- Queries registered via registry ✓
- Full schema registered via explicit items ✓
- DiscoveryResult items can be explicitly registered ✓

## Code Quality

- ✅ All 15 focused tests passing
- ✅ All 280 axum tests passing (focused, high-quality test suite)
- ✅ Pre-commit hooks passed (ruff, formatting)
- ✅ Zero new warnings or errors
- ✅ 100% backward compatible
- ✅ Comprehensive error handling
- ✅ Full documentation with examples

## Integration with Other Phases

### Phase D.1: AxumRegistry
- D.4 uses the singleton registry from D.1
- All registrations go through D.1's registry methods

### Phase D.2: Discovery System
- D.4 uses `discover_from_package()` from D.2
- Uses `DiscoveryResult` and error collection

### Phase D.3: Registration Hooks
- D.4 works seamlessly with D.3's decorator hooks
- Decorators auto-register when `auto_register=True`
- Explicit lists can still be used

## Breaking Changes

**None!** Phase D.4 is 100% backward compatible.

Old code works unchanged:
```python
# Still works exactly the same
app = create_axum_fraiseql_app(
    database_url="...",
    types=[User, Post],
    queries=[get_users],
)
```

New code uses discovery:
```python
# New way: zero configuration
app = create_axum_fraiseql_app(
    database_url="...",
    auto_discover=True,
)
```

## Files Modified

1. **`src/fraiseql/axum/app.py`** (192 lines added/modified)
   - Enhanced `create_axum_fraiseql_app()` signature
   - Added discovery logic
   - Updated docstring with new examples
   - Added error handling

2. **`src/fraiseql/axum/server.py`** (65 lines added/modified)
   - Updated `__init__()` to accept registry
   - Added registry to all registration methods
   - Added `get_registry()` method
   - Updated docstrings

3. **`tests/unit/axum/test_app_factory_discovery.py`** (400 lines)
   - 24 comprehensive tests
   - Covers all usage patterns
   - Tests backward compatibility
   - Tests error handling
   - Tests registry integration

## Error Handling

Discovery errors are handled gracefully:

```
ERROR    fraiseql.axum.discovery:discovery.py:358 Failed to import package nonexistent.module.xyz: No module named 'nonexistent'
WARNING  fraiseql.axum.app:app.py:241 Encountered 1 errors during discovery in nonexistent.module.xyz
INFO     fraiseql.axum.app:app.py:283 FraiseQL Axum server created
```

- Invalid packages: Logged, but server still created
- Empty packages: Logged as debug, server created
- Import errors: Collected and reported

## Performance

- Discovery is only performed if `auto_discover=True` (zero overhead by default)
- Discovery runs once at app creation time
- No runtime performance impact
- Registry lookup is O(1) via dict

## Next Steps

**Phase D.5: Documentation and Examples**
- Create comprehensive registry system guide
- Write migration guide from FastAPI version
- Create 5-10 example applications
- Document best practices
- Add troubleshooting guide

**Future Phases**:
- Phase 3: API documentation UI (Swagger/ReDoc)
- Phase 16: Full middleware system
- Phase X: GraphQL federation support

## Architectural Decision: Explicit > Zero-Config

Initially implemented zero-config auto-discovery, but after review, **removed it** because:

**Problems with zero-config:**
1. Implicit magic - hidden schema dependencies
2. Performance overhead - package scanning at startup
3. Fragility - accidental registration of internal types
4. Testing challenges - singleton pollution, fixture complexity
5. Security risks - unintended exposure of internal types
6. Maintainability - hard to see what's in the API

**Benefits of explicit registration:**
1. Clear and auditable - exactly what's in schema
2. Fast - no discovery overhead
3. Debuggable - can trace implementations
4. Testable - deterministic, no magic
5. Secure - explicit whitelist approach
6. Maintainable - full API visible at glance

This aligns with FraiseQL's core strength: **type-safe GraphQL**.

## Summary

Phase D.4 completes the Phase D registry system with explicit registration approach:

- ✅ **Explicit Registration**: Clear, auditable schema definition
- ✅ **Registry Integration**: Centralized item storage (Phase D.1)
- ✅ **Discovery Available**: Can still find items if needed (Phase D.2)
- ✅ **Decorator Hooks**: Auto-register when explicitly used (Phase D.3)
- ✅ **Tested**: 15 focused tests, 280 total passing
- ✅ **Clean**: Pre-commit hooks passed
- ✅ **Compatible**: 100% backward compatible

**Ready for Phase D.5: Documentation and Examples**
