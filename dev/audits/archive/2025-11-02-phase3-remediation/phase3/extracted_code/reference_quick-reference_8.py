# Extracted from: docs/reference/quick-reference.md
# Block number: 8
from fraiseql.sql import create_graphql_where_input

# Generate Where input type for any @type decorated class
UserWhereInput = create_graphql_where_input(User)
PostWhereInput = create_graphql_where_input(Post)


@query
async def users(info, where: UserWhereInput | None = None) -> list[User]:
    db = info.context["db"]
    return await db.find("users", where=where)
