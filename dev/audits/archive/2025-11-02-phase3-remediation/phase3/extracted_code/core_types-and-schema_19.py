# Extracted from: docs/core/types-and-schema.md
# Block number: 19
from fraiseql import query
from fraiseql.types import create_connection


@query
async def users_connection(info, first: int = 20) -> Connection[User]:
    db = info.context["db"]
    result = await db.paginate("v_user", first=first)
    return create_connection(result, User)
