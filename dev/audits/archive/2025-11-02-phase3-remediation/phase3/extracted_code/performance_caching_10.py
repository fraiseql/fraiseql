# Extracted from: docs/performance/caching.md
# Block number: 10
from fastapi import HTTPException, Request


@app.middleware("http")
async def tenant_context_middleware(request: Request, call_next):
    # Extract tenant from subdomain, JWT, or header
    tenant_id = await resolve_tenant_id(request)

    if not tenant_id:
        raise HTTPException(400, "Tenant not identified")

    # Store in request state
    request.state.tenant_id = tenant_id

    # Set in PostgreSQL session for RLS
    async with pool.connection() as conn:
        await conn.execute("SET LOCAL app.current_tenant_id = $1", tenant_id)

    response = await call_next(request)
    return response
