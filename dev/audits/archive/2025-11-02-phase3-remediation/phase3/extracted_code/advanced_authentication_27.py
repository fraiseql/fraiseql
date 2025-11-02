# Extracted from: docs/advanced/authentication.md
# Block number: 27
from fraiseql import mutation

# Define roles with associated permissions
ROLES = {
    "user": ["orders:read:self", "orders:write:self", "profile:read:self", "profile:write:self"],
    "manager": ["orders:read:team", "orders:write:team", "users:read:team", "reports:read:team"],
    "admin": ["admin:all"],
}


# Check in resolver
@mutation
async def delete_order(info, order_id: str) -> bool:
    user = info.context["user"]

    if not user.has_any_permission(["orders:delete", "admin:all"]):
        raise GraphQLError("Insufficient permissions")

    order = await fetch_order(order_id)

    # Owners can delete own orders
    if order.user_id != user.user_id and not user.has_permission("admin:all"):
        raise GraphQLError("Can only delete your own orders")

    await delete_order_by_id(order_id)
    return True
