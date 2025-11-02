# Extracted from: docs/diagrams/multi-tenant-isolation.md
# Block number: 1
from contextvars import ContextVar

tenant_context: ContextVar[str] = ContextVar("tenant_id")


class TenantMiddleware:
    async def __call__(self, request, call_next):
        # Extract tenant from various sources
        tenant_id = (
            request.headers.get("X-Tenant-ID")
            or request.url.hostname.split(".")[0]
            or jwt_decode(request.headers.get("Authorization", "")).get("tenant_id")
        )

        if not tenant_id:
            raise HTTPException(400, "No tenant identified")

        # Set context for entire request
        token = tenant_context.set(tenant_id)
        try:
            response = await call_next(request)
            return response
        finally:
            tenant_context.reset(token)
