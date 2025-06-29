# FraiseQL Test Database Pool Initialization Issue

## Problem

When running tests with FraiseQL v0.1.0b1, we get the error:
```
RuntimeError: Database pool not initialized. Call create_fraiseql_app first.
```

This happens because:
1. The app is created at module level in `app.py`
2. FraiseQL creates the app without a database pool in test environments
3. Tests can't inject their own pool after the app is created

## Current Workaround Attempts

We tried:
1. Using `set_db_pool()` before importing the app - doesn't work because app is already created
2. Creating a test-specific app factory - still fails because FraiseQL's dependencies expect the pool to be set during app creation
3. Setting environment variables - FraiseQL doesn't automatically create a pool from DATABASE_URL

## Expected Behavior

FraiseQL should either:
1. Automatically create a database pool from DATABASE_URL environment variable when the app is created
2. Provide a way to inject a test database pool after app creation
3. Provide documentation on how to properly set up tests with custom database connections

## Test Code

```python
# tests/integration/test_simple_integration.py
import pytest
from httpx import AsyncClient, ASGITransport

@pytest.mark.asyncio
async def test_graphql_endpoint():
    """Test that GraphQL endpoint is accessible."""
    from printoptim_backend.entrypoints.api.app import app

    async with AsyncClient(transport=ASGITransport(app=app), base_url="http://test") as client:
        response = await client.post(
            "/graphql",
            json={"query": "{ __typename }"},
            headers={"tenant-id": "22222222-2222-2222-2222-222222222222"},
        )
        assert response.status_code == 200
```

## Environment

- FraiseQL version: 0.1.0b1
- Python version: 3.13.3
- PostgreSQL version: 17
- Testing framework: pytest with pytest-asyncio

## Impact

This blocks all integration testing with FraiseQL, forcing us to either:
- Skip integration tests
- Use a different testing approach
- Wait for a fix in FraiseQL