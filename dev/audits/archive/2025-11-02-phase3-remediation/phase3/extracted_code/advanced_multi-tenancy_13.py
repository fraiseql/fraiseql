# Extracted from: docs/advanced/multi-tenancy.md
# Block number: 13
from fraiseql import query


@query
@requires_role("super_admin")
async def get_all_tenants_orders(
    info, tenant_id: str | None = None, limit: int = 100
) -> list[Order]:
    """Admin query: Get orders across tenants."""
    # Bypass RLS by using superuser connection or disabling RLS
    async with db.connection() as conn:
        # Disable RLS for this query (requires appropriate permissions)
        await conn.execute("SET LOCAL row_security = off")

        if tenant_id:
            result = await conn.execute(
                "SELECT * FROM orders WHERE tenant_id = $1 LIMIT $2", tenant_id, limit
            )
        else:
            result = await conn.execute("SELECT * FROM orders LIMIT $1", limit)

        return [Order(**row) for row in await result.fetchall()]
