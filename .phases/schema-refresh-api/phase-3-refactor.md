# Phase 3: Schema Refresh API - REFACTOR Phase

**Phase**: REFACTOR (Code Quality)
**Objective**: Extract utilities, improve code organization, add comprehensive logging
**Status**: Not Started
**Estimated Effort**: 1.5-2 hours

---

## Context

Phase 2 (GREEN) implemented a working `refresh_schema()` method. This REFACTOR phase improves code quality without changing behavior:

1. Extract cache clearing logic to reusable utility
2. Improve error handling and logging
3. Add type hints and documentation
4. Simplify the refresh method using extracted utilities

**Key Principle**: Tests must continue to pass unchanged. This is pure refactoring.

---

## Files to Create/Modify

### Create
- `src/fraiseql/testing/schema_utils.py` - New utility module for schema testing helpers

### Modify
- `src/fraiseql/fastapi/app.py` - Simplify refresh_schema() using utilities
- `tests/conftest.py` - Use extracted cache clearing utility
- `src/fraiseql/testing/__init__.py` - Export new utilities

### Read
- `tests/conftest.py:129-175` - Current cache clearing implementation (to extract)

---

## Implementation Steps

### Step 1: Create Schema Testing Utilities Module

Create `src/fraiseql/testing/schema_utils.py`:

```python
"""Utilities for GraphQL schema testing and manipulation.

Provides helpers for clearing caches, refreshing schemas, and managing
schema state during testing.
"""

from __future__ import annotations

import logging
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from graphql import GraphQLSchema

logger = logging.getLogger(__name__)


def clear_fraiseql_caches() -> None:
    """Clear all FraiseQL internal caches.

    Clears:
    - Python GraphQL type cache (_graphql_type_cache)
    - Type-to-view mapping registry (_type_registry)
    - View type registry (_view_type_registry)
    - Python SchemaRegistry singleton
    - Rust schema registry (if available)

    This is useful when rebuilding the schema or resetting test state.

    Note:
        This does NOT clear FastAPI dependencies (db_pool, auth_provider).
        Use clear_fraiseql_state() for complete cleanup.

    Example:
        >>> clear_fraiseql_caches()
        >>> # Caches cleared, ready to rebuild schema
    """
    # Clear GraphQL type cache
    try:
        from fraiseql.core.graphql_type import _graphql_type_cache

        _graphql_type_cache.clear()
        logger.debug("Cleared GraphQL type cache")
    except ImportError:
        logger.debug("GraphQL type cache not available")
    except Exception as e:
        logger.warning(f"Failed to clear GraphQL type cache: {e}")

    # Clear type registry
    try:
        from fraiseql.core.type_registry import _type_registry

        _type_registry.clear()
        logger.debug("Cleared type registry")
    except ImportError:
        logger.debug("Type registry not available")
    except Exception as e:
        logger.warning(f"Failed to clear type registry: {e}")

    # Clear view type registry
    try:
        from fraiseql.db import _view_type_registry

        _view_type_registry.clear()
        logger.debug("Cleared view type registry")
    except ImportError:
        logger.debug("View type registry not available")
    except Exception as e:
        logger.warning(f"Failed to clear view type registry: {e}")

    # Clear Python SchemaRegistry
    try:
        from fraiseql.gql.builders import SchemaRegistry

        SchemaRegistry.get_instance().clear()
        logger.debug("Cleared Python SchemaRegistry")
    except ImportError:
        logger.debug("SchemaRegistry not available")
    except Exception as e:
        logger.warning(f"Failed to clear SchemaRegistry: {e}")

    # Reset Rust schema registry
    try:
        from fraiseql._fraiseql_rs import reset_schema_registry_for_testing

        reset_schema_registry_for_testing()
        logger.debug("Reset Rust schema registry")
    except ImportError:
        logger.debug("Rust schema registry not available")
    except Exception as e:
        logger.warning(f"Failed to reset Rust registry: {e}")


def clear_fraiseql_state() -> None:
    """Clear all FraiseQL state including caches and FastAPI dependencies.

    This performs a complete cleanup:
    1. All caches (via clear_fraiseql_caches())
    2. FastAPI global dependencies (db_pool, auth_provider, config)

    Use this for complete teardown in test fixtures.

    Example:
        >>> @pytest.fixture(scope="session", autouse=True)
        >>> def cleanup_after_tests():
        >>>     yield
        >>>     clear_fraiseql_state()
    """
    # Clear all caches first
    clear_fraiseql_caches()

    # Reset FastAPI global dependencies
    try:
        from fraiseql.fastapi.dependencies import (
            set_auth_provider,
            set_db_pool,
            set_fraiseql_config,
        )

        set_db_pool(None)
        set_auth_provider(None)
        set_fraiseql_config(None)
        logger.debug("Reset FastAPI dependencies")
    except ImportError:
        logger.debug("FastAPI dependencies not available")
    except Exception as e:
        logger.warning(f"Failed to reset FastAPI dependencies: {e}")


def validate_schema_refresh(
    old_schema: GraphQLSchema,
    new_schema: GraphQLSchema,
    *,
    expect_new_types: bool = False,
) -> dict[str, set[str]]:
    """Validate that a schema refresh preserved existing elements.

    Args:
        old_schema: The schema before refresh
        new_schema: The schema after refresh
        expect_new_types: If True, verify new schema has MORE types than old

    Returns:
        Dictionary with:
        - "preserved_types": Type names present in both schemas
        - "new_types": Type names only in new schema
        - "lost_types": Type names only in old schema (should be empty!)

    Raises:
        AssertionError: If schema refresh lost types or mutations

    Example:
        >>> old = app.state.graphql_schema
        >>> await app.refresh_schema()
        >>> new = app.state.graphql_schema
        >>> result = validate_schema_refresh(old, new, expect_new_types=True)
        >>> assert len(result["lost_types"]) == 0
    """
    old_types = set(old_schema.type_map.keys())
    new_types = set(new_schema.type_map.keys())

    preserved = old_types & new_types
    added = new_types - old_types
    lost = old_types - new_types

    # Validate no types were lost
    if lost:
        logger.error(f"Schema refresh lost types: {lost}")
        raise AssertionError(f"Schema refresh lost {len(lost)} types: {lost}")

    # Validate mutations preserved
    if old_schema.mutation_type and new_schema.mutation_type:
        old_mutations = set(old_schema.mutation_type.fields.keys())
        new_mutations = set(new_schema.mutation_type.fields.keys())
        lost_mutations = old_mutations - new_mutations

        if lost_mutations:
            logger.error(f"Schema refresh lost mutations: {lost_mutations}")
            raise AssertionError(
                f"Schema refresh lost {len(lost_mutations)} mutations: {lost_mutations}"
            )

    # Log summary
    logger.info(
        f"Schema refresh: {len(preserved)} preserved, {len(added)} added, {len(lost)} lost"
    )

    if expect_new_types and not added:
        logger.warning("Expected new types but none were added")

    return {
        "preserved_types": preserved,
        "new_types": added,
        "lost_types": lost,
    }
```

### Step 2: Export Utilities in Testing Module

Update `src/fraiseql/testing/__init__.py`:

```python
"""Testing utilities for FraiseQL applications."""

from fraiseql.testing.schema_utils import (
    clear_fraiseql_caches,
    clear_fraiseql_state,
    validate_schema_refresh,
)

__all__ = [
    "clear_fraiseql_caches",
    "clear_fraiseql_state",
    "validate_schema_refresh",
]
```

### Step 3: Refactor `tests/conftest.py` to Use Utility

Update `tests/conftest.py` (around line 129):

```python
def _clear_all_fraiseql_state() -> None:
    """Comprehensive cleanup of all FraiseQL global state.

    Delegates to fraiseql.testing.clear_fraiseql_state() utility.
    """
    if not FRAISEQL_AVAILABLE:
        return

    from fraiseql.testing import clear_fraiseql_state

    clear_fraiseql_state()
```

**Delete** the old implementation (lines 142-189) since it's now in the utility.

### Step 4: Refactor `refresh_schema()` in `app.py`

Simplify the refresh method using the extracted utility:

```python
async def refresh_schema() -> GraphQLSchema:
    """Refresh the GraphQL schema by re-introspecting the database.

    This rebuilds the schema from scratch, discovering new database functions
    and views that were created after app initialization. Primarily useful
    for testing scenarios where database functions are created dynamically.

    The method:
    1. Clears all Python and Rust caches
    2. Re-runs auto-discovery (if enabled)
    3. Rebuilds the GraphQL schema
    4. Reinitializes the Rust schema registry
    5. Updates TurboRegistry cache
    6. Replaces the GraphQL route handler

    Returns:
        The newly built GraphQLSchema instance

    Example:
        >>> # In test after creating database functions
        >>> await app.refresh_schema()
        >>> # New mutations now available in schema

    Raises:
        RuntimeError: If refresh config not found (app not created with create_fraiseql_app)

    Note:
        Schema refresh is expensive (~50-200ms) due to database introspection
        and schema rebuilding. Use sparingly, typically once per test class.
    """
    import importlib
    import json
    import time

    if not hasattr(app.state, "_fraiseql_refresh_config"):
        raise RuntimeError(
            "Cannot refresh schema: app not created with create_fraiseql_app(). "
            "Ensure app was created using the standard FraiseQL factory."
        )

    refresh_config = app.state._fraiseql_refresh_config
    logger.info("Starting schema refresh...")
    refresh_start = time.time()

    # Step 1: Clear all caches using utility
    from fraiseql.testing import clear_fraiseql_caches

    clear_fraiseql_caches()

    # Step 2: Re-run auto-discovery if enabled
    auto_types: list[type] = []
    auto_queries: list = []
    auto_mutations: list = []

    if refresh_config["auto_discover"]:
        from fraiseql.introspection import AutoDiscovery

        logger.debug("Running auto-discovery...")
        discoverer = AutoDiscovery(refresh_config["database_url"])
        auto_types, auto_queries, auto_mutations = await discoverer.discover_all()
        logger.info(
            f"Auto-discovery: {len(auto_types)} types, "
            f"{len(auto_queries)} queries, {len(auto_mutations)} mutations"
        )

    # Step 3: Rebuild GraphQL schema
    from fraiseql.gql.schema_builder import build_fraiseql_schema

    all_query_types = (
        list(refresh_config["original_types"])
        + list(refresh_config["original_queries"])
        + auto_types
        + auto_queries
    )
    all_mutations = list(refresh_config["original_mutations"]) + auto_mutations

    new_schema = build_fraiseql_schema(
        query_types=all_query_types,
        mutation_resolvers=all_mutations,
        camel_case_fields=refresh_config["camel_case_fields"],
    )
    logger.debug(f"Rebuilt schema with {len(new_schema.type_map)} types")

    # Step 4: Reinitialize Rust schema registry
    if refresh_config["enable_schema_registry"]:
        try:
            _fraiseql_rs = importlib.import_module("fraiseql._fraiseql_rs")
            from fraiseql.core.schema_serializer import SchemaSerializer

            serializer = SchemaSerializer()
            schema_ir = serializer.serialize_schema(new_schema)
            schema_json = json.dumps(schema_ir)
            _fraiseql_rs.initialize_schema_registry(schema_json)
            logger.debug("Reinitialized Rust schema registry")
        except Exception as e:
            logger.warning(f"Failed to reinitialize Rust registry: {e}")

    # Step 5: Update app state and clear TurboRegistry
    old_schema = app.state.graphql_schema
    app.state.graphql_schema = new_schema

    if hasattr(app.state, "turbo_registry") and app.state.turbo_registry:
        app.state.turbo_registry.clear()
        logger.debug("Cleared TurboRegistry cache")

    # Step 6: Replace GraphQL router
    _replace_graphql_router(app, new_schema, refresh_config)

    # Validate refresh (development builds only)
    if logger.isEnabledFor(logging.DEBUG):
        from fraiseql.testing import validate_schema_refresh

        try:
            result = validate_schema_refresh(old_schema, new_schema)
            logger.debug(
                f"Schema validation: {len(result['preserved_types'])} preserved, "
                f"{len(result['new_types'])} new"
            )
        except AssertionError as e:
            logger.error(f"Schema refresh validation failed: {e}")
            raise

    refresh_duration = (time.time() - refresh_start) * 1000
    logger.info(f"Schema refresh completed in {refresh_duration:.2f}ms")

    return new_schema


def _replace_graphql_router(app, new_schema, refresh_config):
    """Replace GraphQL router with new schema (internal helper)."""
    from fraiseql.fastapi.routers import create_graphql_router

    # Remove existing GraphQL routes
    original_route_count = len(app.routes)
    app.routes[:] = [
        route for route in app.routes if not (hasattr(route, "path") and route.path == "/graphql")
    ]
    removed_routes = original_route_count - len(app.routes)
    logger.debug(f"Removed {removed_routes} GraphQL routes")

    # Create and mount new router
    new_router = create_graphql_router(
        schema=new_schema,
        database_url=refresh_config["database_url"],
        config=refresh_config["config"],
        auth_provider=refresh_config["auth_provider"],
        turbo_registry=app.state.turbo_registry if hasattr(app.state, "turbo_registry") else None,
    )
    app.include_router(new_router)
    logger.debug("Mounted new GraphQL router")

# Attach methods to app
app.refresh_schema = refresh_schema
```

### Step 5: Add Comprehensive Logging

Ensure all key operations have appropriate log levels:
- **INFO**: High-level operations (refresh started/completed, discovery results)
- **DEBUG**: Internal steps (cache clearing, router replacement)
- **WARNING**: Non-critical failures (Rust registry unavailable)
- **ERROR**: Critical issues (validation failures)

Already covered in the refactored code above.

### Step 6: Run Tests to Verify No Behavioral Changes

```bash
uv run pytest tests/unit/fastapi/test_schema_refresh.py -v
```

**Expected**: All 3 tests still PASS (same as Phase 2)

---

## Verification Commands

### Run refresh tests (must still pass)
```bash
uv run pytest tests/unit/fastapi/test_schema_refresh.py -v
```

**Expected**: 3/3 PASSED ✅

### Run full test suite (no regressions)
```bash
uv run pytest tests/ -v -k "not integration"
```

**Expected**: All unit tests pass

### Verify new module imports correctly
```bash
python -c "from fraiseql.testing import clear_fraiseql_caches, validate_schema_refresh; print('OK')"
```

**Expected**: `OK`

### Lint all changed files
```bash
uv run ruff check src/fraiseql/fastapi/app.py src/fraiseql/testing/schema_utils.py tests/conftest.py
```

**Expected**: No errors

---

## Acceptance Criteria

- [ ] New `schema_utils.py` module created with 3 utilities
- [ ] Utilities exported in `fraiseql.testing.__init__.py`
- [ ] `tests/conftest.py` refactored to use utility (code removed)
- [ ] `refresh_schema()` simplified using `clear_fraiseql_caches()`
- [ ] Router replacement extracted to helper function
- [ ] All tests still pass (no behavioral changes)
- [ ] Comprehensive logging at all levels
- [ ] Code passes linting and type checking
- [ ] Documentation complete in all docstrings

---

## DO NOT

- ❌ Change test behavior (tests must pass unchanged)
- ❌ Add new features (pure refactoring only)
- ❌ Skip extracting utilities (code reuse is the goal)
- ❌ Remove logging (we want MORE logging, not less)

---

## Code Quality Improvements

### Before (Phase 2)
```python
# 80+ lines of cache clearing inline in refresh_schema()
# 30+ lines of router replacement inline
# Duplicate code between conftest.py and app.py
```

### After (Phase 3)
```python
# 1 line: clear_fraiseql_caches()
# 1 line: _replace_graphql_router(app, schema, config)
# Shared utility used by both testing and app code
# Validation helper for debugging
```

**Benefits**:
- Reduced duplication
- Easier to test utilities independently
- Better separation of concerns
- Reusable for future features

---

## Testing the Refactor

**Manual verification** that utilities work:

```python
# Test cache clearing
from fraiseql.testing import clear_fraiseql_caches
from fraiseql.core.graphql_type import _graphql_type_cache

_graphql_type_cache["test"] = "value"
clear_fraiseql_caches()
assert "test" not in _graphql_type_cache  # Cleared!

# Test validation
from fraiseql.testing import validate_schema_refresh
result = validate_schema_refresh(old_schema, new_schema)
assert "preserved_types" in result
assert "new_types" in result
assert len(result["lost_types"]) == 0
```

---

## Phase Completion

**Definition of Done**:
- All utilities extracted and tested
- Code duplication eliminated
- All existing tests pass unchanged
- Comprehensive logging added
- Ready for Phase 4 (QA - use in real integration tests)

**Commit Message**:
```bash
refactor(testing): extract schema refresh utilities [REFACTOR]

Extract reusable utilities from schema refresh implementation:
- clear_fraiseql_caches(): Clear all Python/Rust caches
- clear_fraiseql_state(): Complete state cleanup
- validate_schema_refresh(): Verify refresh correctness

Benefits:
- Eliminates code duplication between app.py and conftest.py
- Provides reusable utilities for testing
- Improves code organization and maintainability
- Adds validation helper for debugging

The refresh_schema() method is now simplified to ~40 lines
(down from ~80) by using extracted utilities.

Related: Phase 3 of schema refresh API implementation
```
