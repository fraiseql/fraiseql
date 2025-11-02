# Extracted from: docs/advanced/authentication.md
# Block number: 13
from fraiseql import mutation, query
from fraiseql.auth import requires_role


@query
@requires_role("admin")
async def get_all_users(info) -> list[User]:
    """Get all users - admin only."""
    return await fetch_all_users()


@mutation
@requires_role("moderator")
async def ban_user(info, user_id: str, reason: str) -> bool:
    """Ban user - moderator only."""
    await ban_user_by_id(user_id, reason)
    return True
