# Phase 1: Schema Refresh API - RED Phase

**Phase**: RED (Test-First)
**Objective**: Write failing test demonstrating schema refresh capability
**Status**: Not Started
**Estimated Effort**: 1-1.5 hours

---

## Context

Currently, FraiseQL builds its GraphQL schema once during app initialization. This makes it impossible to test features that require dynamically created database functions (like WP-034 native error arrays). We need a `refresh_schema()` method on the FastAPI app to rebuild the schema after database changes.

This RED phase writes a test that:
1. Starts an app with initial schema
2. Creates a new database function
3. Calls `app.refresh_schema()` to discover the new function
4. Verifies the new mutation is available in GraphQL

**Related**: This unblocks WP-034 tests in `tests/integration/graphql/mutations/test_native_error_arrays.py`

---

## Files to Create/Modify

### Create
- `tests/unit/fastapi/test_schema_refresh.py` - New test file for refresh API

### Read (for context)
- `src/fraiseql/fastapi/app.py` - Understand app structure
- `tests/fixtures/examples/conftest_examples.py` - Fixture patterns
- `tests/conftest.py` - Existing cleanup patterns

---

## Implementation Steps

### Step 1: Create Test File Structure

Create `tests/unit/fastapi/test_schema_refresh.py`:

```python
"""Tests for GraphQL schema refresh capability.

Tests the ability to rebuild the GraphQL schema after database changes,
which is essential for testing dynamically created functions/views.
"""

import pytest
from graphql import graphql_sync


@pytest.mark.asyncio
class TestSchemaRefresh:
    """Test suite for schema refresh functionality."""

    async def test_refresh_schema_discovers_new_mutations(
        self,
        blog_simple_app,
        blog_simple_db_url,
    ):
        """Test that refresh_schema() discovers newly created database functions.

        Scenario:
        1. App starts with initial schema (blog_simple mutations)
        2. Create a new mutation function in the database
        3. Call app.refresh_schema()
        4. Verify new mutation is available in GraphQL schema
        """
        # ARRANGE: Get initial schema
        initial_schema = blog_simple_app.state.graphql_schema
        initial_mutations = initial_schema.mutation_type.fields.keys()

        # Verify test function doesn't exist yet
        assert "testSchemaRefresh" not in initial_mutations

        # ACT: Create new mutation function
        import psycopg
        async with await psycopg.AsyncConnection.connect(blog_simple_db_url) as conn:
            await conn.execute("""
                CREATE OR REPLACE FUNCTION test_schema_refresh()
                RETURNS mutation_response
                LANGUAGE plpgsql
                AS $$
                BEGIN
                    RETURN mutation_success(NULL::integer);
                END;
                $$;
            """)
            await conn.commit()

        # Refresh schema to discover new function
        # THIS WILL FAIL - method doesn't exist yet (RED phase)
        await blog_simple_app.refresh_schema()

        # ASSERT: Verify new mutation is available
        refreshed_schema = blog_simple_app.state.graphql_schema
        refreshed_mutations = refreshed_schema.mutation_type.fields.keys()

        assert "testSchemaRefresh" in refreshed_mutations
        assert refreshed_schema is not initial_schema  # New schema instance

    async def test_refresh_schema_preserves_existing_types(
        self,
        blog_simple_app,
    ):
        """Test that refresh_schema() preserves original types and mutations.

        Ensures we don't lose existing schema elements during refresh.
        """
        # ARRANGE: Get initial schema elements
        initial_schema = blog_simple_app.state.graphql_schema
        initial_types = set(initial_schema.type_map.keys())
        initial_mutations = set(initial_schema.mutation_type.fields.keys())

        # ACT: Refresh without adding anything new
        await blog_simple_app.refresh_schema()

        # ASSERT: All original elements still present
        refreshed_schema = blog_simple_app.state.graphql_schema
        refreshed_types = set(refreshed_schema.type_map.keys())
        refreshed_mutations = set(refreshed_schema.mutation_type.fields.keys())

        assert initial_types.issubset(refreshed_types)
        assert initial_mutations.issubset(refreshed_mutations)

    async def test_refresh_schema_clears_caches(
        self,
        blog_simple_app,
    ):
        """Test that refresh_schema() properly clears all internal caches.

        Ensures GraphQL type cache and Rust registry are reset.
        """
        # ARRANGE: Force some cache population
        initial_schema = blog_simple_app.state.graphql_schema
        _ = graphql_sync(initial_schema, "{ __schema { types { name } } }")

        # ACT: Refresh schema
        await blog_simple_app.refresh_schema()

        # ASSERT: Caches are cleared
        from fraiseql.core.graphql_type import _graphql_type_cache

        # Cache should be cleared during refresh
        # (Implementation will populate it again, but the clear happened)
        refreshed_schema = blog_simple_app.state.graphql_schema
        assert refreshed_schema is not initial_schema
```

### Step 2: Run Test to Verify It Fails

```bash
uv run pytest tests/unit/fastapi/test_schema_refresh.py -v
```

**Expected Output**:
```
test_refresh_schema_discovers_new_mutations FAILED
  AttributeError: 'Starlette' object has no attribute 'refresh_schema'
```

This proves:
- ✅ Test is correctly written
- ✅ Feature doesn't exist yet (RED phase complete)
- ✅ Ready for GREEN phase implementation

---

## Verification Commands

### Run the new test file
```bash
uv run pytest tests/unit/fastapi/test_schema_refresh.py -v
```

**Expected**: All 3 tests FAIL with `AttributeError: 'Starlette' object has no attribute 'refresh_schema'`

### Verify test file syntax
```bash
uv run ruff check tests/unit/fastapi/test_schema_refresh.py
```

**Expected**: No linting errors

---

## Acceptance Criteria

- [ ] Test file created with 3 test cases
- [ ] Tests import necessary dependencies (psycopg, graphql, pytest)
- [ ] Tests use existing fixtures (`blog_simple_app`, `blog_simple_db_url`)
- [ ] All tests FAIL with `AttributeError` (method doesn't exist)
- [ ] Code passes ruff linting
- [ ] Test logic is clear and well-documented

---

## DO NOT

- ❌ Implement the `refresh_schema()` method (that's Phase 2 GREEN)
- ❌ Modify any production code in `src/`
- ❌ Skip writing comprehensive test cases
- ❌ Write tests that would pass without implementation

---

## Notes

**Why This Test Design?**

1. **Uses blog_simple_app**: Reuses existing test infrastructure, no custom setup
2. **Creates real DB function**: Tests actual introspection, not mocks
3. **Three test aspects**: Discovery (new functions), preservation (existing schema), caching (cleanup)
4. **Clear assertions**: Easy to verify GREEN phase success

**Test Isolation**:

Each test uses `blog_simple_app` fixture which provides a fresh database per test class (class-scoped isolation). Database functions created in tests are automatically cleaned up.

---

## Phase Completion

**Definition of Done**:
- All 3 tests fail with the expected error
- Test code is clean and passes linting
- Ready to move to Phase 2 (GREEN - implement the feature)

**Commit Message**:
```bash
test(fastapi): add schema refresh API tests [RED]

Add failing tests for app.refresh_schema() method that will enable
dynamic schema updates after database changes. Required for testing
WP-034 native error arrays feature.

Tests cover:
- Discovery of newly created mutations
- Preservation of existing schema elements
- Cache clearing during refresh

Related: Phase 1 of schema refresh API implementation
```
