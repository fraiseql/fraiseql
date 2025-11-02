# Extracted from: docs/reference/decorators.md
# Block number: 16
from fraiseql import field, type


@type
class User:
    first_name: str
    last_name: str

    @field(description="Full display name")
    def display_name(self) -> str:
        return f"{self.first_name} {self.last_name}"

    @field(description="User's posts")
    async def posts(self, info) -> list[Post]:
        db = info.context["db"]
        return await db.find("v_post", where={"user_id": self.id})

    @field(description="Posts with parameters")
    async def recent_posts(self, info, limit: int = 10) -> list[Post]:
        db = info.context["db"]
        return await db.find(
            "v_post", where={"user_id": self.id}, order_by="created_at DESC", limit=limit
        )
