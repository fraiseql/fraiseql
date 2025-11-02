# Extracted from: docs/core/types-and-schema.md
# Block number: 18
from fraiseql import connection, query, type
from fraiseql.types import Connection


@type(sql_source="v_user")
class User:
    id: UUID
    name: str
    email: str


@connection(node_type=User)
@query
async def users_connection(
    info, first: int | None = None, after: str | None = None
) -> Connection[User]:
    pass  # Implementation handled by decorator
