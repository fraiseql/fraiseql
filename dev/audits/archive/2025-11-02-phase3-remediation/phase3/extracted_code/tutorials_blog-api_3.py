# Extracted from: docs/tutorials/blog-api.md
# Block number: 3
from fraiseql import input, mutation


@input
class CreatePostInput:
    title: str
    content: str
    excerpt: str | None = None
    tags: list[str] | None = None
    is_published: bool = False


@input
class CreateCommentInput:
    post_id: UUID
    content: str
    parent_id: UUID | None = None


@mutation
def create_post(input: CreatePostInput) -> Post:
    """Create new blog post."""
    # Implementation handled by framework


@mutation
def create_comment(input: CreateCommentInput) -> Comment:
    """Add comment to post."""
    # Implementation handled by framework
