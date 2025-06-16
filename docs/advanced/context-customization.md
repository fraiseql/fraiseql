# Context Customization

FraiseQL allows you to customize the GraphQL context that's available to all resolvers. This is useful for adding request-specific data, feature flags, or custom services.

## Default Context

By default, FraiseQL provides:
- `db`: Database connection from the pool
- `user`: Authenticated user context (if auth is enabled)

## Custom Context Getter

Use the `context_getter` parameter to add custom data:

```python
from fastapi import Request
from fraiseql import create_fraiseql_app

async def custom_context_getter(request: Request) -> dict[str, Any]:
    """Build custom GraphQL context."""
    # Get default context
    from fraiseql.fastapi.dependencies import build_graphql_context
    context = await build_graphql_context()
    
    # Add custom data
    context["request"] = request
    context["ip_address"] = request.client.host
    context["request_id"] = request.headers.get("X-Request-ID")
    
    # Add feature flags
    context["features"] = {
        "new_ui": True,
        "beta_features": request.headers.get("X-Beta") == "true",
        "debug_mode": request.headers.get("X-Debug") == "true"
    }
    
    # Add custom services
    context["cache"] = app.state.cache
    context["email_service"] = app.state.email_service
    
    return context

app = create_fraiseql_app(
    database_url="postgresql://localhost/mydb",
    context_getter=custom_context_getter,
)
```

## Using Custom Context in Resolvers

### In Query Functions

```python
from fraiseql import query

@query
async def debug_info(info) -> dict[str, Any]:
    """Get debug information (only in debug mode)."""
    if not info.context["features"]["debug_mode"]:
        raise Exception("Debug mode not enabled")
    
    return {
        "request_id": info.context["request_id"],
        "ip_address": info.context["ip_address"],
        "db_pool_stats": {
            "size": info.context["db"].pool.size,
            "idle": info.context["db"].pool.idle,
            "busy": info.context["db"].pool.busy
        }
    }
```

### In Mutations

```python
from fraiseql import mutation

@mutation
class SendEmail:
    input: SendEmailInput
    success: SendEmailSuccess
    failure: SendEmailFailure
    
    async def execute(self, db, input_data, info):
        # Access custom services
        email_service = info.context["email_service"]
        
        # Check feature flags
        if info.context["features"]["new_email_system"]:
            result = await email_service.send_v2(
                to=input_data.to,
                subject=input_data.subject,
                body=input_data.body
            )
        else:
            result = await email_service.send(
                input_data.to,
                input_data.subject,
                input_data.body
            )
        
        return SendEmailSuccess(message_id=result.id)
```

## Advanced Patterns

### Per-Request Caching

```python
async def context_with_cache(request: Request) -> dict[str, Any]:
    """Add per-request cache to context."""
    context = await build_graphql_context()
    
    # Create request-specific cache
    context["request_cache"] = {}
    
    # Add cache helper
    async def cached_fetch(key: str, fetcher):
        if key not in context["request_cache"]:
            context["request_cache"][key] = await fetcher()
        return context["request_cache"][key]
    
    context["cached_fetch"] = cached_fetch
    
    return context

# Usage in resolver
@query
async def expensive_calculation(info) -> dict[str, Any]:
    return await info.context["cached_fetch"](
        "expensive_result",
        lambda: perform_expensive_calculation()
    )
```

### Request Tracking

```python
import uuid
from datetime import datetime

async def context_with_tracking(request: Request) -> dict[str, Any]:
    """Add request tracking to context."""
    context = await build_graphql_context()
    
    # Generate or get request ID
    request_id = request.headers.get("X-Request-ID", str(uuid.uuid4()))
    
    # Add tracking info
    context["tracking"] = {
        "request_id": request_id,
        "start_time": datetime.utcnow(),
        "user_agent": request.headers.get("User-Agent"),
        "referer": request.headers.get("Referer"),
    }
    
    # Add tracking helper
    async def track_event(event_name: str, data: dict = None):
        await db.execute(
            """
            INSERT INTO events (request_id, event_name, data, timestamp)
            VALUES ($1, $2, $3, $4)
            """,
            request_id,
            event_name,
            data or {},
            datetime.utcnow()
        )
    
    context["track_event"] = track_event
    
    return context
```

### Multi-Tenant Support

```python
async def context_with_tenant(request: Request) -> dict[str, Any]:
    """Add tenant context for multi-tenant apps."""
    context = await build_graphql_context()
    
    # Extract tenant from header, subdomain, or JWT
    tenant_id = request.headers.get("X-Tenant-ID")
    if not tenant_id:
        # Extract from subdomain
        host = request.headers.get("Host", "")
        if host.endswith(".myapp.com"):
            tenant_id = host.split(".")[0]
    
    # Add tenant context
    if tenant_id:
        context["tenant_id"] = tenant_id
        # Create tenant-scoped DB connection
        context["tenant_db"] = await get_tenant_db(tenant_id)
    
    return context

# Usage in resolver
@query
async def tenant_data(info) -> dict[str, Any]:
    """Get data for current tenant."""
    tenant_db = info.context.get("tenant_db")
    if not tenant_db:
        raise Exception("No tenant context")
    
    return await tenant_db.fetch_one(
        "SELECT * FROM tenant_settings"
    )
```

## Best Practices

1. **Keep context lightweight**: Don't add heavy objects that aren't needed
2. **Use lazy loading**: Initialize expensive resources only when accessed
3. **Type your context**: Consider creating a TypedDict for context structure
4. **Handle missing context**: Always check if custom context exists
5. **Document context**: List all available context keys in your API docs

## Context Type Hints

For better type safety:

```python
from typing import TypedDict, Optional

class GraphQLContext(TypedDict):
    db: Any  # Your DB type
    user: Optional[UserContext]
    request: Request
    features: dict[str, bool]
    cache: Any  # Your cache type

# Use in resolvers
@query
async def typed_query(info: Info[GraphQLContext]) -> str:
    # Now you get type hints!
    return f"User: {info.context['user'].email}"
```

## See Also

- [Authentication](../authentication.md) - How user context is added
- [Lifespan Management](./lifecycle-management.md) - Initialize context resources
- [Query Decorator](../api-reference/query-decorator.md) - Using context in queries