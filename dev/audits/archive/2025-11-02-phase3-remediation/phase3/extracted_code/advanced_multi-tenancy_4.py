# Extracted from: docs/advanced/multi-tenancy.md
# Block number: 4
from uuid import UUID

from fraiseql import query, type_


@type_
class Order:
    id: UUID
    tenant_id: UUID  # Automatically filtered
    user_id: UUID
    total: float
    status: str


@query
async def get_orders(info: GraphQLResolveInfo) -> list[Order]:
    """Get orders for current tenant."""
    tenant_id = info.context["tenant_id"]

    # Explicit tenant filtering (recommended for clarity)
    async with db.connection() as conn:
        result = await conn.execute("SELECT * FROM orders WHERE tenant_id = $1", tenant_id)
        return [Order(**row) for row in await result.fetchall()]


@query
async def get_order(info: GraphQLResolveInfo, order_id: UUID) -> Order | None:
    """Get specific order - tenant isolation enforced."""
    tenant_id = info.context["tenant_id"]

    async with db.connection() as conn:
        result = await conn.execute(
            "SELECT * FROM orders WHERE id = $1 AND tenant_id = $2", order_id, tenant_id
        )
        row = await result.fetchone()
        return Order(**row) if row else None
