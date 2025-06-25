# Apollo Sandbox Not Serving on GET Requests

**RESOLVED: 2025-01-24**

## Issue Description

When configuring FraiseQL with Apollo Sandbox enabled, the GraphQL endpoint does not serve the Apollo Sandbox UI when accessed via GET request in the browser. Instead, it returns a 422 error indicating a missing query parameter.

## Current Behavior

1. Configuration:
```python
fraiseql_config = FraiseQLConfig(
    database_url=settings.database_url,
    environment="development",
    enable_introspection=True,
    enable_playground=True,
    playground_tool="apollo-sandbox",
    auth_enabled=False,
)
```

2. When accessing `http://localhost:8000/graphql` in browser:
   - Returns: `{"detail":[{"type":"missing","loc":["query","query"],"msg":"Field required","input":null}]}`
   - HTTP Status: 422 Unprocessable Content

3. The GraphQL endpoint works correctly for POST requests with queries

## Expected Behavior

When `enable_playground=True` and `playground_tool="apollo-sandbox"`, accessing `/graphql` via GET request in a browser should serve the Apollo Sandbox HTML interface.

## Environment

- FraiseQL version: (latest as of June 2025)
- FastAPI integration
- Python 3.13
- Running in development mode

## Workaround

Currently need to use external GraphQL clients to interact with the endpoint.

## Additional Context

- The endpoint only accepts POST requests (returns 405 Method Not Allowed for GET with curl)
- Introspection queries work correctly
- No HTML is served regardless of Accept headers

## Reproduction Steps

1. Configure FraiseQL with Apollo Sandbox as shown above
2. Start the server
3. Navigate to http://localhost:8000/graphql in a browser
4. Observe 422 error instead of Apollo Sandbox UI

## Resolution

Fixed in commit [pending] by implementing Option 1 from TICKET_002:

1. **Modified GET /graphql handler** in `src/fraiseql/fastapi/routers.py`:
   - When no query parameter is provided and playground is enabled, serves the playground HTML
   - Respects the `playground_tool` configuration (apollo-sandbox or graphiql)
   - Maintains security by only serving playground in development mode

2. **Removed /playground endpoint**:
   - No longer needed since /graphql now serves dual purpose
   - Follows standard GraphQL server conventions

3. **Behavior after fix**:
   - GET `/graphql` (no query) → Apollo Sandbox UI
   - GET `/graphql?query={...}` → Execute GraphQL query
   - POST `/graphql` → Execute GraphQL query (unchanged)

## Code Changes

```python
# Before: Required query parameter
async def graphql_get_endpoint(
    query: str,  # Required
    ...
)

# After: Optional query parameter with playground serving
async def graphql_get_endpoint(
    query: str | None = None,  # Optional
    ...
):
    # Serve playground if no query provided
    if query is None and config.enable_playground:
        if config.playground_tool == "apollo-sandbox":
            return HTMLResponse(content=APOLLO_SANDBOX_HTML)
        return HTMLResponse(content=GRAPHIQL_HTML)
```

## Testing

To verify the fix:
1. Access http://localhost:8000/graphql in a browser
2. Should see Apollo Sandbox UI (or GraphiQL based on config)
3. Queries from the UI should work correctly