# Extracted from: docs/advanced/where_input_types.md
# Block number: 3
@fraiseql.query
async def users(info, where: UserWhereInput | None = None) -> list[User]:
    db = info.context["db"]
    return await db.find("users", where=where)
