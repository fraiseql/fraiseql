# Extracted from: docs/reference/quick-reference.md
# Block number: 2
from fraiseql import query


@query
async def users(info) -> list[User]:
    """Get all users."""
    db = info.context["db"]
    return await db.find("users")
