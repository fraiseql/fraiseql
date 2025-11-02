# Extracted from: docs/advanced/authentication.md
# Block number: 16
from fraiseql import mutation
from fraiseql.auth import requires_auth, requires_permission


@mutation
@requires_auth
@requires_permission("orders:refund")
async def refund_order(info, order_id: str, reason: str) -> Order:
    """Refund order - requires authentication and orders:refund permission."""
    user = info.context["user"]

    # Additional custom checks
    order = await fetch_order(order_id)
    if order.user_id != user.user_id and not user.has_role("admin"):
        raise GraphQLError("Can only refund your own orders")

    return await process_refund(order_id, reason)
