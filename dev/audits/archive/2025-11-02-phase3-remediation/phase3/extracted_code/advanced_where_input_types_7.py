# Extracted from: docs/advanced/where_input_types.md
# Block number: 7
# Before: Manual filtering
@fraiseql.query
async def users_by_status(info, status: str) -> list[User]:
    db = info.context["db"]
    query = "SELECT * FROM users WHERE status = %s"
    result = await db.run(DatabaseQuery(query, [status]))
    return [User(**row) for row in result]

# After: Where input filtering
@fraiseql.query
async def users(info, where: UserWhereInput | None = None) -> list[User]:
    db = info.context["db"]
    return await db.find("users", where=where)

# Usage remains the same, but now supports complex filtering
query {
  users(where: { status: { eq: "active" } }) { id name status }
}
