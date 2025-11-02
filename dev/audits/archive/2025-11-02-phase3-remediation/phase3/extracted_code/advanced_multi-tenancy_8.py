# Extracted from: docs/advanced/multi-tenancy.md
# Block number: 8
from fastapi import FastAPI, HTTPException, Request

app = FastAPI()


@app.middleware("http")
async def tenant_context_middleware(request: Request, call_next):
    """Set tenant context for all requests."""
    try:
        # 1. Resolve tenant (try multiple strategies)
        tenant_id = None

        # Try JWT first
        if "Authorization" in request.headers:
            try:
                tenant_id = extract_tenant_from_jwt(request)
            except:
                pass

        # Try subdomain
        if not tenant_id:
            try:
                subdomain = extract_tenant_from_subdomain(request)
                tenant_id = await resolve_tenant_id(subdomain)
            except:
                pass

        # Try header
        if not tenant_id:
            try:
                tenant_id = extract_tenant_from_header(request)
            except:
                pass

        if not tenant_id:
            raise HTTPException(status_code=400, detail="Tenant not identified")

        # 2. Store in request state
        request.state.tenant_id = tenant_id

        # 3. Set in database session
        await set_tenant_context(tenant_id)

        # 4. Continue request
        response = await call_next(request)
        return response

    except HTTPException:
        raise
    except Exception as e:
        raise HTTPException(status_code=500, detail=f"Tenant resolution failed: {e}")
