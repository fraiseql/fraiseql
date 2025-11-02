# Extracted from: docs/production/security.md
# Block number: 16
from fraiseql import mutation

# Automatic audit trail via PostgreSQL trigger
# See advanced/event-sourcing.md for complete implementation


@mutation
async def update_order_status(info, order_id: str, status: str) -> Order:
    """Update order status - automatically logged."""
    user_id = info.context["user"].user_id

    async with db.connection() as conn:
        # Set user context for trigger
        await conn.execute("SET LOCAL app.current_user_id = $1", user_id)

        # Update (trigger logs before/after state)
        await conn.execute("UPDATE orders SET status = $1 WHERE id = $2", status, order_id)

    return await fetch_order(order_id)
