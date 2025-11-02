# Extracted from: docs/core/queries-and-mutations.md
# Block number: 11
from fraiseql import field, type


@type
class User:
    id: UUID

    @field(description="User's posts with optional filtering")
    async def posts(self, info, published_only: bool = False, limit: int = 10) -> list[Post]:
        repo = info.context["repo"]
        filters = {"user_id": self.id}
        if published_only:
            filters["status"] = "published"
        return await repo.find_rust("v_post", "posts", info, **filters, limit=limit)
