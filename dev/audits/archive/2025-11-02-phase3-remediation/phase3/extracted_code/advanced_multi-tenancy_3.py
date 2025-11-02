# Extracted from: docs/advanced/multi-tenancy.md
# Block number: 3

from fraiseql.db import get_db_pool


async def set_tenant_context(tenant_id: str):
    """Set tenant_id in PostgreSQL session variable."""
    pool = get_db_pool()
    async with pool.connection() as conn:
        await conn.execute("SET LOCAL app.current_tenant_id = $1", tenant_id)


# Middleware to set tenant context
from starlette.middleware.base import BaseHTTPMiddleware


class TenantContextMiddleware(BaseHTTPMiddleware):
    async def dispatch(self, request, call_next):
        # Extract tenant from request (subdomain, header, JWT)
        tenant_id = await resolve_tenant_id(request)

        # Store in request state
        request.state.tenant_id = tenant_id

        # Set in database session
        await set_tenant_context(tenant_id)

        response = await call_next(request)
        return response
