# Phase 2: Schema Refresh API - GREEN Phase

**Phase**: GREEN (Implementation)
**Objective**: Implement `refresh_schema()` method to make tests pass
**Status**: Not Started
**Estimated Effort**: 2.5-3 hours

---

## Context

Phase 1 (RED) created failing tests. This GREEN phase implements the `refresh_schema()` method on the FastAPI app to enable dynamic schema updates after database changes.

**Core Challenge**: We need to carefully orchestrate clearing multiple caches (Python, Rust) and rebuilding the schema while preserving app state and dependencies.

---

## Files to Modify

### Modify
- `src/fraiseql/fastapi/app.py` - Add `refresh_schema()` method
- `src/fraiseql/fastapi/app.py` - Store original config for refresh

### Read (for implementation patterns)
- `src/fraiseql/gql/schema_builder.py` - Schema building function
- `src/fraiseql/introspection/auto_discovery.py` - Introspection logic
- `tests/conftest.py:129-175` - Existing cache clearing pattern

---

## Implementation Steps

### Step 1: Store Original Configuration During App Creation

In `src/fraiseql/fastapi/app.py`, modify the `create_fraiseql_app()` function to store configuration needed for refresh.

**Location**: After schema is built (around line 490), before router creation (around line 538)

```python
# After: schema = build_fraiseql_schema(...)

# Store configuration for potential schema refresh
app.state._fraiseql_refresh_config = {
    "database_url": database_url,
    "original_types": list(types),
    "original_queries": list(queries),
    "original_mutations": list(mutations),
    "auto_discover": auto_discover,
    "camel_case_fields": config.auto_camel_case,
    "enable_schema_registry": enable_schema_registry,
}
app.state.graphql_schema = schema
```

**Why**: `refresh_schema()` needs to know the original configuration to re-run introspection and schema building.

### Step 2: Implement `refresh_schema()` Method

Add the following method to the app instance. **Location**: After the app is created, before routers are mounted (around line 540).

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

    # Step 1: Clear all Python caches
    from fraiseql.core.graphql_type import _graphql_type_cache
    from fraiseql.db import _view_type_registry
    from fraiseql.gql.builders import SchemaRegistry

    _graphql_type_cache.clear()
    _view_type_registry.clear()
    SchemaRegistry.get_instance().clear()
    logger.debug("Cleared Python type caches")

    # Step 2: Reset Rust schema registry
    try:
        _fraiseql_rs = importlib.import_module("fraiseql._fraiseql_rs")
        _fraiseql_rs.reset_schema_registry_for_testing()
        logger.debug("Reset Rust schema registry")
    except Exception as e:
        logger.warning(f"Failed to reset Rust registry: {e}")

    # Step 3: Re-run auto-discovery if enabled
    auto_types: list[type] = []
    auto_queries: list = []
    auto_mutations: list = []

    if refresh_config["auto_discover"]:
        from fraiseql.introspection import AutoDiscovery

        logger.debug("Running auto-discovery...")
        discoverer = AutoDiscovery(refresh_config["database_url"])
        auto_types, auto_queries, auto_mutations = await discoverer.discover_all()
        logger.info(
            f"Auto-discovery found: {len(auto_types)} types, "
            f"{len(auto_queries)} queries, {len(auto_mutations)} mutations"
        )

    # Step 4: Rebuild GraphQL schema
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

    # Step 5: Reinitialize Rust schema registry
    if refresh_config["enable_schema_registry"]:
        try:
            from fraiseql.core.schema_serializer import SchemaSerializer

            serializer = SchemaSerializer()
            schema_ir = serializer.serialize_schema(new_schema)
            schema_json = json.dumps(schema_ir)
            _fraiseql_rs.initialize_schema_registry(schema_json)
            logger.debug("Reinitialized Rust schema registry")
        except Exception as e:
            logger.warning(f"Failed to reinitialize Rust registry: {e}")

    # Step 6: Update app state
    app.state.graphql_schema = new_schema

    # Step 7: Clear TurboRegistry cache
    if hasattr(app.state, "turbo_registry") and app.state.turbo_registry:
        app.state.turbo_registry.clear()
        logger.debug("Cleared TurboRegistry cache")

    # Step 8: Replace GraphQL router
    # Remove existing GraphQL routes
    original_route_count = len(app.routes)
    app.routes[:] = [
        route for route in app.routes if not (hasattr(route, "path") and route.path == "/graphql")
    ]
    removed_routes = original_route_count - len(app.routes)
    logger.debug(f"Removed {removed_routes} GraphQL routes")

    # Create new router with refreshed schema
    from fraiseql.fastapi.routers import create_graphql_router

    new_router = create_graphql_router(
        schema=new_schema,
        database_url=refresh_config["database_url"],
        config=config,
        auth_provider=auth_provider,
        turbo_registry=app.state.turbo_registry if hasattr(app.state, "turbo_registry") else None,
    )
    app.include_router(new_router)
    logger.debug("Mounted new GraphQL router")

    refresh_duration = (time.time() - refresh_start) * 1000
    logger.info(f"Schema refresh completed in {refresh_duration:.2f}ms")

    return new_schema

# Attach method to app instance
app.refresh_schema = refresh_schema
```

**Why Each Step**:

1. **Clear Python caches**: Old type definitions must be removed
2. **Reset Rust registry**: Rust extension holds schema metadata
3. **Re-run discovery**: Find new database functions/views
4. **Rebuild schema**: Create fresh GraphQL schema with all types
5. **Reinit Rust**: Update Rust extension with new schema
6. **Update app state**: Store new schema reference
7. **Clear TurboRegistry**: Invalidate cached execution plans
8. **Replace router**: Ensure GraphQL endpoint uses new schema

### Step 3: Handle Router Dependencies

The `create_graphql_router()` call needs `config` and `auth_provider` which are in scope during `create_fraiseql_app()`. We need to store these too.

**Update** the config storage (Step 1) to include:

```python
app.state._fraiseql_refresh_config = {
    "database_url": database_url,
    "original_types": list(types),
    "original_queries": list(queries),
    "original_mutations": list(mutations),
    "auto_discover": auto_discover,
    "camel_case_fields": config.auto_camel_case,
    "enable_schema_registry": enable_schema_registry,
    "config": config,  # ADD THIS
    "auth_provider": auth_provider,  # ADD THIS
}
```

### Step 4: Run Tests to Verify GREEN

```bash
uv run pytest tests/unit/fastapi/test_schema_refresh.py -v
```

**Expected Output**:
```
test_refresh_schema_discovers_new_mutations PASSED
test_refresh_schema_preserves_existing_types PASSED
test_refresh_schema_clears_caches PASSED

====== 3 passed in 2.50s ======
```

---

## Verification Commands

### Run refresh tests
```bash
uv run pytest tests/unit/fastapi/test_schema_refresh.py -v
```

**Expected**: All 3 tests PASS ✅

### Run full test suite (ensure no regressions)
```bash
uv run pytest tests/unit/fastapi/ -v
```

**Expected**: All tests PASS, no new failures

### Lint the changes
```bash
uv run ruff check src/fraiseql/fastapi/app.py
```

**Expected**: No linting errors

### Type check
```bash
uv run mypy src/fraiseql/fastapi/app.py
```

**Expected**: No type errors (or only pre-existing ones)

---

## Acceptance Criteria

- [ ] `refresh_schema()` method implemented on app instance
- [ ] Original config stored during app creation
- [ ] All caches properly cleared (Python, Rust, TurboRegistry)
- [ ] Schema rebuilt with auto-discovery
- [ ] GraphQL router replaced with new schema
- [ ] All 3 tests from Phase 1 now PASS
- [ ] No regressions in existing tests
- [ ] Code passes ruff linting
- [ ] Comprehensive logging at DEBUG and INFO levels

---

## DO NOT

- ❌ Skip any cache clearing steps (causes stale schema bugs)
- ❌ Forget to replace the GraphQL router (endpoint will serve old schema)
- ❌ Modify test files (tests should pass as-is from Phase 1)
- ❌ Add features beyond basic refresh (keep it simple for GREEN)

---

## Edge Cases to Handle

### 1. App Not Created with `create_fraiseql_app()`
**Handle**: Raise `RuntimeError` with clear message

### 2. Rust Extension Import Failure
**Handle**: Log warning, continue (maintains backward compatibility)

### 3. Auto-Discovery Disabled
**Handle**: Skip discovery, only rebuild with original types

### 4. Empty Schema After Refresh
**Handle**: Should not happen if original types preserved, but log warning if mutation_type is None

---

## Performance Considerations

**Schema refresh is expensive** (~50-200ms depending on schema size):
- Database introspection queries
- GraphQL schema building
- Rust registry serialization/initialization

**Acceptable** because:
- Only used in testing (not production hot path)
- Called once per test class, not per test
- Still faster than restarting the entire app

---

## Testing Strategy

The Phase 1 tests cover:
- ✅ Discovery of new mutations
- ✅ Preservation of existing types
- ✅ Cache clearing

**Additional manual verification**:
```python
# In Python REPL with blog_simple app
from examples.blog_simple.app import app
initial = app.state.graphql_schema.mutation_type.fields.keys()
# Create DB function...
await app.refresh_schema()
updated = app.state.graphql_schema.mutation_type.fields.keys()
assert set(initial).issubset(set(updated))  # Old preserved
assert "newMutation" in updated  # New discovered
```

---

## Phase Completion

**Definition of Done**:
- All Phase 1 tests pass
- No test regressions
- Code is clean, documented, and linted
- Ready for Phase 3 (REFACTOR)

**Commit Message**:
```bash
feat(fastapi): add schema refresh API [GREEN]

Implement app.refresh_schema() method to enable dynamic schema updates
after database changes. This unblocks testing of features that require
dynamically created database functions (e.g., WP-034 native error arrays).

Implementation:
- Store original config during app creation
- Clear all caches (Python, Rust, TurboRegistry)
- Re-run auto-discovery to find new functions/views
- Rebuild GraphQL schema with all types
- Reinitialize Rust schema registry
- Replace GraphQL router with refreshed schema

The refresh process is comprehensive and safe, preserving all existing
schema elements while discovering new ones.

Closes: Phase 2 of schema refresh API implementation
Tests: All tests in test_schema_refresh.py now pass
```
