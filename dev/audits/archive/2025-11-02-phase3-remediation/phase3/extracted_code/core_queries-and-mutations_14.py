# Extracted from: docs/core/queries-and-mutations.md
# Block number: 14
from fraiseql import type, query, mutation, input, field

@connection(
    node_type: type,
    view_name: str | None = None,
    default_page_size: int = 20,
    max_page_size: int = 100,
    include_total_count: bool = True,
    cursor_field: str = "id",
    jsonb_extraction: bool | None = None,
    jsonb_column: str | None = None
)
@query
async def query_name(
    info,
    first: int | None = None,
    after: str | None = None,
    where: dict | None = None
) -> Connection[NodeType]:
    pass  # Implementation handled by decorator
