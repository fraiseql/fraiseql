# Extracted from: docs/core/fraiseql-philosophy.md
# Block number: 1
from fraiseql import query


# ❌ Traditional approach - repetitive and error-prone
@query
async def get_user(info, id: UUID) -> User:
    # Must manually get database from somewhere
    db = get_database_from_somewhere()
    # Or pass it through complex dependency injection
    return await db.find_one("users", {"id": id})
