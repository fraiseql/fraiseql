# Extracted from: docs/tutorials/blog-api.md
# Block number: 2
from uuid import UUID

from fraiseql import query


@query
def get_post(id: UUID) -> Post | None:
    """Get single post with all nested data."""
    # Implementation handled by framework


@query
def get_posts(is_published: bool | None = None, limit: int = 20, offset: int = 0) -> list[Post]:
    """List posts with filtering and pagination."""
    # Implementation handled by framework
