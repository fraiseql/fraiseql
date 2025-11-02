# Extracted from: docs/core/fraiseql-philosophy.md
# Block number: 2
from fraiseql import query


# ✅ FraiseQL - database automatically available
@query
async def get_user(info, id: UUID) -> User:
    db = info.context["db"]  # Always available!
    return await db.find_one("v_user", where={"id": id})
