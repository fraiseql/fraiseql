# Extracted from: docs/performance/caching-migration.md
# Block number: 14
# Ensure tenant middleware runs BEFORE GraphQL
@app.middleware("http")
async def tenant_middleware(request: Request, call_next):
    request.state.tenant_id = await resolve_tenant(request)
    return await call_next(request)


# Then use in repository context
context = {"tenant_id": request.state.tenant_id}
