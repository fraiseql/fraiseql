# Extracted from: docs/core/queries-and-mutations.md
# Block number: 16
from fraiseql import query


@connection(
    node_type=Post,
    view_name="v_published_posts",
    default_page_size=25,
    max_page_size=50,
    cursor_field="created_at",
    jsonb_extraction=True,
    jsonb_column="data",
)
@query
async def posts_connection(
    info, first: int | None = None, after: str | None = None, where: dict[str, Any] | None = None
) -> Connection[Post]:
    pass
