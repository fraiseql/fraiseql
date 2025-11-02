# Extracted from: docs/core/queries-and-mutations.md
# Block number: 17
from fraiseql import query


@connection(node_type=User, cursor_field="created_at")
@query
async def recent_users_connection(
    info, first: int | None = None, after: str | None = None, where: dict[str, Any] | None = None
) -> Connection[User]:
    pass
