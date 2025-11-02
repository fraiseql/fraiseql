# Extracted from: docs/reference/quick-reference.md
# Block number: 4
from fraiseql import query
from fraiseql.sql import create_graphql_where_input

# Generate automatic Where input type
UserWhereInput = create_graphql_where_input(User)


@query
async def users(info, where: UserWhereInput | None = None) -> list[User]:
    """Get users with optional filtering."""
    db = info.context["db"]
    return await db.find("users", where=where)
