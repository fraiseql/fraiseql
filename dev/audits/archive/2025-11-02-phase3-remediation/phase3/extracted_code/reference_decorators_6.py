# Extracted from: docs/reference/decorators.md
# Block number: 6
from fraiseql import query


@query
async def get_user(info, id: UUID) -> User:
    db = info.context["db"]
    return await db.find_one("v_user", where={"id": id})


@query
async def search_users(info, name_filter: str | None = None, limit: int = 10) -> list[User]:
    db = info.context["db"]
    filters = {}
    if name_filter:
        filters["name__icontains"] = name_filter
    return await db.find("v_user", where=filters, limit=limit)
