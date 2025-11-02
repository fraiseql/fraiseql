# Extracted from: docs/core/queries-and-mutations.md
# Block number: 9
from fraiseql import field, type


@type
class User:
    id: UUID

    @field(description="Posts authored by this user")
    async def posts(self, info) -> list[Post]:
        repo = info.context["repo"]
        return await repo.find_rust("v_post", "posts", info, user_id=self.id)
