# Extracted from: docs/database/DATABASE_LEVEL_CACHING.md
# Block number: 3
from fraiseql import query


@query
async def active_users(info, limit: int = 10) -> list[User]:
    """Uses partial index automatically
    Query planner chooses idx_users_active
    0.05ms vs 0.15ms (3x faster)
    """
    repo = Repository(info.context["db"], info.context)
    return await repo.find("users", where={"active": True, "deleted_at": None}, limit=limit)
