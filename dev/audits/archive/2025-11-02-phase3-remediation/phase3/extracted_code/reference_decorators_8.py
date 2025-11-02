# Extracted from: docs/reference/decorators.md
# Block number: 8
from fraiseql import connection, query, type
from fraiseql.types import Connection


@type(sql_source="v_user")
class User:
    id: UUID
    name: str


@connection(node_type=User)
@query
async def users_connection(info, first: int | None = None) -> Connection[User]:
    pass  # Implementation handled by decorator


@connection(
    node_type=Post,
    view_name="v_published_posts",
    default_page_size=25,
    max_page_size=50,
    cursor_field="created_at",
)
@query
async def posts_connection(
    info, first: int | None = None, after: str | None = None
) -> Connection[Post]:
    pass
