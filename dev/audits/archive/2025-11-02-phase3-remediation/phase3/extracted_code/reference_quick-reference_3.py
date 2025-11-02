# Extracted from: docs/reference/quick-reference.md
# Block number: 3
from uuid import UUID

from fraiseql import query


@query
async def user(info, id: UUID) -> User | None:
    """Get user by ID."""
    db = info.context["db"]
    return await db.get_by_id("users", id)
