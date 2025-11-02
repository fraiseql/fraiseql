# Extracted from: docs/core/types-and-schema.md
# Block number: 20
from fraiseql import query


@query
async def users_paginated(info, page: int = 1, limit: int = 20) -> Connection[User]:
    db = info.context["db"]
    offset = (page - 1) * limit
    users = await db.find("v_user", limit=limit, offset=offset)
    total = await db.count("v_user")

    # Manual construction
    from fraiseql.types import Connection, Edge, PageInfo

    edges = [Edge(node=user, cursor=str(i)) for i, user in enumerate(users)]
    page_info = PageInfo(
        has_next_page=offset + limit < total, has_previous_page=page > 1, total_count=total
    )

    return Connection(edges=edges, page_info=page_info, total_count=total)
