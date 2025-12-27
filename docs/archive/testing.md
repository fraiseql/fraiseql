# FraiseQL Testing Guide

## Dynamic Schema Refresh

### Problem

FraiseQL builds its GraphQL schema once during app initialization by introspecting the database. This static schema approach provides excellent performance and safety, but makes it challenging to test features that require dynamically created database functions or views.

**Example scenario**: You want to test that mutations properly handle error cases by creating test-specific database functions that return error responses.

### Solution: `app.refresh_schema()`

The `refresh_schema()` method allows you to rebuild the GraphQL schema after making database changes. This is primarily useful for testing scenarios where you need to create database objects dynamically.

```python
@pytest.fixture
async def app_with_custom_mutations(app, db_url):
    """App with dynamically created mutations."""
    # Create database functions
    async with await psycopg.AsyncConnection.connect(db_url) as conn:
        await conn.execute("""
            CREATE FUNCTION my_test_mutation()
            RETURNS mutation_response
            LANGUAGE plpgsql AS $$
            BEGIN
                RETURN mutation_success(NULL::integer);
            END;
            $$;
        """)
        await conn.commit()

    # Refresh schema to discover new function
    await app.refresh_schema()

    yield app
```

### How It Works

When you call `app.refresh_schema()`, FraiseQL:

1. **Clears all caches** - Python GraphQL type cache, type registries, Rust schema registry
2. **Re-runs auto-discovery** - If enabled, scans database for new views and functions
3. **Rebuilds GraphQL schema** - Creates new schema with discovered types
4. **Reinitializes Rust registry** - Updates the execution engine with new schema
5. **Updates TurboRegistry cache** - Clears execution plans
6. **Replaces GraphQL route** - Updates FastAPI router with new schema

The entire process takes ~50-200ms depending on schema complexity.

### When to Use

**✅ Use schema refresh when**:
- Testing features that create database functions dynamically
- Creating test-specific mutations that shouldn't be in production schema
- Verifying auto-discovery behavior
- Developing plugins that add GraphQL types at runtime

**❌ Don't use schema refresh when**:
- Testing existing schema (no dynamic functions needed)
- Performance is critical (refresh adds ~50-200ms overhead)
- Functions can be added to example app's `init.sql` instead
- In production code (restart the app instead)

### Performance Considerations

Schema refresh is an expensive operation:
- Database introspection queries (~20-50ms)
- GraphQL schema rebuilding (~20-80ms)
- Rust registry re-initialization (~10-40ms)
- Router replacement (~5-10ms)

**Total**: ~50-200ms per refresh

**Recommended pattern**: Use class-scoped fixtures to refresh once per test class, not per test function.

```python
@pytest.fixture(scope="class")  # ← Class scope, not function
async def app_with_mutations(app, db_url):
    # Setup...
    await app.refresh_schema()
    yield app
```

### Complete Example

Here's a complete example of using schema refresh in tests:

```python
# tests/integration/test_my_feature.py

import pytest
import psycopg
from fraiseql.db import DatabaseQuery


@pytest.fixture(scope="class")
async def app_with_test_mutations(blog_simple_app, blog_simple_db_url):
    """Create test mutations and refresh schema."""

    # Create test database function
    async with await psycopg.AsyncConnection.connect(blog_simple_db_url) as conn:
        await conn.execute("""
            CREATE OR REPLACE FUNCTION test_error_handling()
            RETURNS mutation_response
            LANGUAGE plpgsql AS $$
            BEGIN
                RETURN mutation_validation_error(
                    'Test error message',
                    'TestEntity',
                    NULL
                );
            END;
            $$;
        """)
        await conn.commit()

    # Define Python wrapper mutation
    from fraiseql.mutations import mutation

    @mutation
    async def test_error_handling(info) -> dict:
        """Test mutation that calls the database function."""
        db = info.context["db"]
        query = DatabaseQuery("SELECT * FROM test_error_handling()", [])
        result = await db.run(query)
        return result[0] if result else {}

    # Add to refresh config and refresh
    if hasattr(blog_simple_app.state, "_fraiseql_refresh_config"):
        blog_simple_app.state._fraiseql_refresh_config["original_mutations"].append(
            test_error_handling
        )

    await blog_simple_app.refresh_schema()

    yield blog_simple_app


class TestErrorHandling:
    """Test error handling with dynamic mutations."""

    @pytest.mark.asyncio
    async def test_mutation_error_response(
        self,
        app_with_test_mutations,
        blog_simple_graphql_client,
    ):
        """Test that mutations return proper error structure."""
        query = """
            mutation {
                testErrorHandling {
                    status
                    message
                }
            }
        """

        result = await blog_simple_graphql_client.execute(query)

        assert result.get("errors") is None
        data = result["data"]["testErrorHandling"]
        assert data["status"] == "validation:"
        assert data["message"] == "Test error message"
```

## Schema Testing Utilities

FraiseQL provides utilities in `fraiseql.testing` for schema manipulation during tests.

### `clear_fraiseql_caches()`

Clears all internal caches without recreating the schema. Useful for testing cache behavior.

```python
from fraiseql.testing import clear_fraiseql_caches

clear_fraiseql_caches()
# All caches cleared:
# - Python GraphQL type cache
# - Type registries
# - Rust schema registry
```

### `clear_fraiseql_state()`

Complete cleanup including caches and FastAPI dependencies. Use this for thorough teardown in test fixtures.

```python
from fraiseql.testing import clear_fraiseql_state

@pytest.fixture(scope="session", autouse=True)
def cleanup_after_tests():
    yield
    clear_fraiseql_state()
```

### `validate_schema_refresh()`

Verify that schema refresh preserved existing elements and optionally added new ones.

```python
from fraiseql.testing import validate_schema_refresh

old_schema = app.state.graphql_schema
await app.refresh_schema()
new_schema = app.state.graphql_schema

result = validate_schema_refresh(
    old_schema,
    new_schema,
    expect_new_types=True  # Verify new types were added
)

assert len(result["lost_types"]) == 0  # No types lost
assert len(result["new_types"]) > 0    # New types discovered
assert len(result["preserved_types"]) > 0  # Existing types kept
```

## Best Practices

### 1. Minimize Refresh Calls

Each refresh costs ~50-200ms. Use class-scoped fixtures:

```python
# ✅ GOOD: Refresh once per class
@pytest.fixture(scope="class")
async def app_with_mutations(app, db_url):
    await app.refresh_schema()
    yield app

# ❌ BAD: Refresh for every test
@pytest.fixture(scope="function")
async def app_with_mutations(app, db_url):
    await app.refresh_schema()  # Called 10 times for 10 tests!
    yield app
```

### 2. Create Functions Before Refresh

Schema refresh only discovers what exists in the database at refresh time:

```python
# ✅ GOOD: Create then refresh
await conn.execute("CREATE FUNCTION my_func() ...")
await app.refresh_schema()  # Discovers my_func

# ❌ BAD: Refresh then create
await app.refresh_schema()
await conn.execute("CREATE FUNCTION my_func() ...")  # Not in schema!
```

### 3. Clean Up is Automatic

Database fixtures handle cleanup automatically. No need to drop functions manually:

```python
# ✅ GOOD: Let fixture handle cleanup
@pytest.fixture
async def my_fixture(app, db_url):
    await conn.execute("CREATE FUNCTION ...")
    await app.refresh_schema()
    yield app
    # Database dropped automatically

# ❌ UNNECESSARY: Manual cleanup
@pytest.fixture
async def my_fixture(app, db_url):
    await conn.execute("CREATE FUNCTION test_func() ...")
    await app.refresh_schema()
    yield app
    await conn.execute("DROP FUNCTION test_func()")  # Not needed
```

### 4. Enable Debug Logging

Schema refresh includes validation in debug mode:

```python
import logging
logging.getLogger("fraiseql").setLevel(logging.DEBUG)

# Now refresh will log:
# - Cache clearing operations
# - Auto-discovery results
# - Schema validation (types preserved/added/lost)
# - Router replacement
# - Timing information
```

## Limitations and Caveats

### Auto-Discovery Patterns

Auto-discovery looks for specific naming patterns:
- Views: `v_%` (e.g., `v_users`)
- Functions: `fn_%` (e.g., `fn_create_user`)

Test functions with other names won't be auto-discovered. You'll need to either:
1. Rename them to match the pattern
2. Provide Python wrapper functions and add them to `original_mutations`

### Rust Registry Singleton

The Rust schema registry is a global singleton. If you see warnings like "Re-initialization is not allowed", this is expected. The refresh mechanism handles this by calling `reset_schema_registry_for_testing()` before re-initialization.

### Production Usage

Schema refresh is designed for **testing only**. In production:
- Restart the application to pick up schema changes
- Don't call `refresh_schema()` during request handling
- Schema refresh is not thread-safe

## Troubleshooting

### Functions Not Discovered

**Problem**: Created function but it's not in schema after refresh.

**Solutions**:
1. Check function naming matches auto-discovery pattern (`fn_%`)
2. Verify function created before calling `refresh_schema()`
3. Check function has proper return type
4. Enable DEBUG logging to see what was discovered

### Schema Validation Errors

**Problem**: `AssertionError: Schema refresh lost X types`

**Solutions**:
1. Check that `original_types` in refresh config is correct
2. Verify manual types are being preserved
3. Use `validate_schema_refresh()` to debug what changed

### Performance Issues

**Problem**: Tests are slow due to many refreshes.

**Solutions**:
1. Change fixture scope from `function` to `class`
2. Batch function creation (create multiple, refresh once)
3. Consider pre-creating functions in template database instead

## Additional Resources

- **Schema Refresh Implementation**: `src/fraiseql/fastapi/app.py` (`refresh_schema()` method)
- **Testing Utilities**: `src/fraiseql/testing/schema_utils.py`
- **Test Architecture**: `/home/lionel/.claude/skills/fraiseql-testing.md`
- **Example Tests**: `tests/unit/fastapi/test_schema_refresh.py`

## Future Enhancements

Once the schema refresh API is mature, it could enable:

1. **Hot-reloading in development** - Watch SQL files, auto-refresh on changes
2. **Dynamic plugin systems** - Load GraphQL types from plugins at runtime
3. **Multi-tenant schemas** - Different schema per tenant database
4. **Migration testing** - Apply migration, refresh, verify GraphQL changes

These are not currently implemented but represent possible future directions.
