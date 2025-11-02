# Extracted from: docs/core/types-and-schema.md
# Block number: 17
from fraiseql import type


@type
class PageInfo:
    has_next_page: bool
    has_previous_page: bool
    start_cursor: str | None = None
    end_cursor: str | None = None
    total_count: int | None = None


@type
class Edge[T]:
    node: T
    cursor: str


@type
class Connection[T]:
    edges: list[Edge[T]]
    page_info: PageInfo
    total_count: int | None = None
