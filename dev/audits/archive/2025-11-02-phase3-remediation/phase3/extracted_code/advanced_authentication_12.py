# Extracted from: docs/advanced/authentication.md
# Block number: 12
from fraiseql import mutation
from fraiseql.auth import requires_permission


@mutation
@requires_permission("orders:create")
async def create_order(info, product_id: str, quantity: int) -> Order:
    """Create order - requires orders:create permission."""
    user = info.context["user"]
    return await create_order_for_user(user.user_id, product_id, quantity)


@mutation
@requires_permission("users:delete")
async def delete_user(info, user_id: str) -> bool:
    """Delete user - requires users:delete permission."""
    await delete_user_by_id(user_id)
    return True
