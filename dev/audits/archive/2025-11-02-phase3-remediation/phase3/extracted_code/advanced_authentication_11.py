# Extracted from: docs/advanced/authentication.md
# Block number: 11
from fraiseql import mutation, query
from fraiseql.auth import requires_auth


@query
@requires_auth
async def get_my_orders(info) -> list[Order]:
    """Get current user's orders - requires authentication."""
    user = info.context["user"]  # Guaranteed to exist
    return await fetch_user_orders(user.user_id)


@mutation
@requires_auth
async def update_profile(info, name: str, email: str) -> User:
    """Update user profile - requires authentication."""
    user = info.context["user"]
    return await update_user_profile(user.user_id, name, email)
