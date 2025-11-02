# Extracted from: docs/reference/database.md
# Block number: 1
from fraiseql import query


@query
async def get_user(info, id: UUID) -> User:
    db = info.context["db"]
    return await db.find_one("v_user", where={"id": id})
