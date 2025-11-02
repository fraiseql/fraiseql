# Extracted from: docs/core/queries-and-mutations.md
# Block number: 15
from fraiseql import connection, query, type
from fraiseql.types import Connection


@type(sql_source="v_user")
class User:
    id: UUID
    name: str
    email: str


@connection(node_type=User)
@query
async def users_connection(info, first: int | None = None) -> Connection[User]:
    pass  # Implementation handled by decorator
