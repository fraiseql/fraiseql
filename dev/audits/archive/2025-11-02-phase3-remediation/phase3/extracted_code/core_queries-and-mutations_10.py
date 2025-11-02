# Extracted from: docs/core/queries-and-mutations.md
# Block number: 10
from fraiseql import field, type


async def fetch_user_posts_optimized(root, info):
    """Custom resolver with optimized batch loading."""
    db = info.context["db"]
    # Use DataLoader or batch loading here
    return await batch_load_posts([root.id])


@type
class User:
    id: UUID

    @field(resolver=fetch_user_posts_optimized, description="Posts with optimized loading")
    async def posts(self) -> list[Post]:
        # This signature defines GraphQL schema
        # but fetch_user_posts_optimized handles actual resolution
        pass
