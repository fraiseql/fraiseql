# Extracted from: docs/advanced/multi-tenancy.md
# Block number: 14
from fraiseql import query


@query
@requires_role("super_admin")
async def get_tenant_statistics(info) -> list[TenantStats]:
    """Get statistics across all tenants."""
    async with db.connection() as conn:
        await conn.execute("SET LOCAL row_security = off")

        result = await conn.execute("""
            SELECT
                t.id as tenant_id,
                t.name as tenant_name,
                COUNT(DISTINCT u.id) as user_count,
                COUNT(DISTINCT o.id) as order_count,
                COALESCE(SUM(o.total), 0) as total_revenue
            FROM organizations t
            LEFT JOIN users u ON u.tenant_id = t.id
            LEFT JOIN orders o ON o.tenant_id = t.id
            GROUP BY t.id, t.name
            ORDER BY total_revenue DESC
        """)

        return [TenantStats(**row) for row in await result.fetchall()]
