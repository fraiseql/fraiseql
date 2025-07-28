# Response: Database Context Missing in FraiseQL v0.1.0a15

## Issue Identified

You've found a critical bug in v0.1.0a15. When a custom `context_getter` is provided to `create_fraiseql_app`, it completely replaces the default context building, which means the database repository is not injected into the context.

## Root Cause

In the GraphQL router implementation, the code was:

```python
if context_getter:
    async def get_context(http_request: Request) -> dict[str, Any]:
        return await context_getter(http_request)  # Only returns custom context!

    context_dependency = Depends(get_context)
else:
    context_dependency = Depends(build_graphql_context)  # Includes db
```

This meant that custom context completely replaced the default context instead of extending it.

## Fix Released: v0.1.0a16

I've just released v0.1.0a16 which fixes this issue. The custom context is now merged with the default context:

```python
if context_getter:
    async def get_merged_context(
        http_request: Request,
        default_context: dict[str, Any] = Depends(build_graphql_context),
    ) -> dict[str, Any]:
        custom_context = await context_getter(http_request)
        # Merge contexts - custom values override defaults
        return {**default_context, **custom_context}
```

## Immediate Solution

Upgrade to v0.1.0a16:

```bash
pip install --upgrade fraiseql==0.1.0a16
```

Your code should work without any changes after upgrading.

## What's Included in the Default Context

The default context now always includes:
- `db`: FraiseQLRepository instance
- `user`: Current user (if authenticated)
- `authenticated`: Boolean flag
- `loader_registry`: DataLoader registry for N+1 prevention
- `n1_detector`: N+1 query detector (in development mode)

Your custom context values (`tenant_id`, `contact_id`) will be merged with these.

## Verification

After upgrading, your debug query should show:

```python
Context keys: ['db', 'user', 'authenticated', 'loader_registry', 'n1_detector', 'tenant_id', 'contact_id']
```

## Apologies

This was a breaking change that should not have happened between alpha releases. The issue has been fixed and tests have been added to prevent this regression in the future.

## Alternative Workaround (if you can't upgrade immediately)

If you need a workaround before upgrading, you can modify your context_getter:

```python
from fraiseql.db import FraiseQLRepository
from fraiseql.fastapi.dependencies import get_db_pool

async def get_context(request):
    """Get context with tenant_id and database."""
    pool = get_db_pool()
    db = FraiseQLRepository(pool)

    return {
        "db": db,
        "tenant_id": request.headers.get("tenant-id", "550e8400-e29b-41d4-a716-446655440000"),
        "contact_id": request.headers.get("contact-id"),
    }
```

But upgrading to v0.1.0a16 is the recommended solution.
