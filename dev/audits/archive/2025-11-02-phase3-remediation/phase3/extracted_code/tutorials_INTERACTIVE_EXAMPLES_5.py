# Extracted from: docs/tutorials/INTERACTIVE_EXAMPLES.md
# Block number: 5
from fraiseql import type


@type(sql_source="tv_post_stats")
class PostStats:
    post_id: UUID
    title: str
    comment_count: int
    avg_rating: float | None
    last_comment_at: datetime | None
