# Extracted from: docs/core/types-and-schema.md
# Block number: 6
from typing import TYPE_CHECKING
from uuid import UUID

from fraiseql import field, type

if TYPE_CHECKING:
    from .types import Post


@type
class User:
    id: UUID
    first_name: str
    last_name: str

    @field(description="Full display name")
    def display_name(self) -> str:
        return f"{self.first_name} {self.last_name}"

    @field(description="User's posts")
    async def posts(self, info) -> list[Post]:
        db = info.context["db"]
        return await db.find("v_post", where={"user_id": self.id})
