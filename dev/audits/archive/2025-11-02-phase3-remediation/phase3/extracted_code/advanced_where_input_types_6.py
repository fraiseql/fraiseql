# Extracted from: docs/advanced/where_input_types.md
# Block number: 6
@fraiseql.field
async def posts(user: User, info, where: PostWhereInput | None = None) -> list[Post]:
    """Get posts for a user with optional filtering."""
    db = info.context["db"]

    # Combine user filter with relationship constraint
    author_filter = PostWhereInput(author_id={"eq": user.id})
    if where:
        combined_where = PostWhereInput(AND=[author_filter, where])
    else:
        combined_where = author_filter

    return await db.find("posts", where=combined_where)
