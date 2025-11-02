# Extracted from: docs/advanced/authentication.md
# Block number: 14
from fraiseql import mutation
from fraiseql.auth import requires_any_permission


@mutation
@requires_any_permission("orders:write", "admin:all")
async def update_order(info, order_id: str, status: str) -> Order:
    """Update order - requires orders:write OR admin:all permission."""
    return await update_order_status(order_id, status)
