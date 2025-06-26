# Database Connection Handling in FraiseQL Context

## Issue Description

We're experiencing issues with database connection management in FraiseQL v0.1.0a14. When queries are executed, we get the error:
```
'NoneType' object has no attribute 'context'
```

This appears to be because the database connection is being closed before the resolver can use it.

## Current Implementation

### Initial Attempt (Not Working)
```python
async def get_context(request: Request) -> dict[str, Any]:
    """Get context for GraphQL requests."""
    pool = get_db_pool(request)
    async with pool.connection() as conn:  # Connection gets closed after context returns
        return {
            "db": conn,
            "request": request,
            "tenant_id": request.headers.get("tenant-id"),
            "contact_id": request.headers.get("contact-id"),
        }
```

### Current Workaround
```python
# Context getter
async def get_context(request: Request) -> dict[str, Any]:
    """Get context for GraphQL requests."""
    pool = get_db_pool(request)
    # Don't use async with here - FraiseQL will manage the connection lifetime
    conn = await pool.getconn()
    # Store connection on request state for cleanup
    request.state.db_conn = conn
    return {
        "db": conn,
        "request": request,
        "tenant_id": request.headers.get("tenant-id", "550e8400-e29b-41d4-a716-446655440000"),
        "contact_id": request.headers.get("contact-id"),
    }

# Cleanup middleware
@app.middleware("http")
async def cleanup_db_connection(request: Request, call_next):
    """Clean up database connection after request."""
    response = await call_next(request)
    # Clean up connection if it exists
    if hasattr(request.state, "db_conn"):
        pool = get_db_pool(request)
        await pool.putconn(request.state.db_conn)
    return response
```

## Questions

1. Is this the recommended approach for handling database connections with FraiseQL?
2. Should FraiseQL handle the connection lifecycle internally when using `create_fraiseql_app`?
3. Is there a better pattern for managing async database connections with psycopg3's AsyncConnectionPool?

## Additional Context

- Using psycopg3 with AsyncConnectionPool
- FastAPI with FraiseQL v0.1.0a14
- The connection needs to persist throughout the entire GraphQL request execution
- Multiple resolvers may need to use the same connection in a single request

## Expected Behavior

The database connection should remain open and available throughout the entire GraphQL request execution, and be properly cleaned up after the request completes.

## Request

Please provide guidance on the best practice for handling database connections with FraiseQL, particularly when using async connection pools. If our current workaround is not ideal, please suggest the recommended approach.