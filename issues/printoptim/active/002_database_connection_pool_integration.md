# FraiseQL Database Connection Pool Integration

## Problem
FraiseQL is creating its own database connections instead of using the application's connection pool, leading to:
- Connection exhaustion in tests
- Timeout errors: `psycopg_pool.PoolTimeout: couldn't get a connection after 30.00 sec`
- Inability to share connection pools between app and FraiseQL

## Current Setup
```python
fraiseql_config = FraiseQLConfig(
    database_url=settings.database_url,  # FraiseQL creates its own connections
    connection_pool_size=20,
    # ...
)
```

## Expected Behavior
FraiseQL should be able to:
1. Accept an existing psycopg connection pool
2. Share connections with the application
3. Respect connection pool limits in testing environments

## Workaround Attempted
Created separate test fixtures with smaller pools, but this leads to connection exhaustion.

## Questions for FraiseQL Team
1. Can FraiseQL accept an existing AsyncConnectionPool instance?
2. How should connection pooling be handled in test environments?
3. Is there a way to limit FraiseQL's connection usage during tests?

## Impact
- Integration tests failing due to connection timeouts
- Unable to properly test database-dependent features
- CI/CD pipeline reliability issues
