# Extracted from: docs/core/fraiseql-philosophy.md
# Block number: 5
from fraiseql import query


# PostgreSQL JSONB → GraphQL JSON directly
# No Python object instantiation needed!
@query
async def user(info, id: UUID) -> User:
    db = info.context["db"]
    # Returns JSONB directly - 10-100x faster
    return await db.find_one("v_user", where={"id": id})
