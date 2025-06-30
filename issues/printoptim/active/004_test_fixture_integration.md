# FraiseQL Test Fixture Integration

## Problem
The `graphql_client` test fixture provided by FraiseQL doesn't integrate well with async test patterns and custom contexts.

## Issues Encountered
1. Fixture scope conflicts with pytest-asyncio
2. Cannot inject custom database connections into graphql_client
3. No clear way to set FraiseQL context in tests

## Current Test Pattern
```python
@pytest.mark.asyncio
async def test_with_context(graphql_client):
    # How to set custom context for this test?
    result = await graphql_client.execute(query)
```

## Expected Behavior
Need ability to:
1. Set custom FraiseQL context per test
2. Use test-specific database connections
3. Mock FraiseQL internals for unit tests

## Questions for FraiseQL Team
1. What's the recommended pattern for testing with custom contexts?
2. How to provide test-specific database connections to FraiseQL?
3. Is there a test mode that uses less database connections?
4. Can we have a mock mode that doesn't require database?

## Impact
- Cannot properly test multi-tenant features
- Test isolation issues
- High number of skipped tests due to fixture problems
