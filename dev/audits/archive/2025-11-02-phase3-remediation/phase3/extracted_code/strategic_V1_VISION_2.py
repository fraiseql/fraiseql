# Extracted from: docs/strategic/V1_VISION.md
# Block number: 2
from uuid import UUID

from fraiseql import field, input, type


@type
class User:
    id: UUID
    identifier: str
    name: str
    email: str

    @field
    async def posts(self, info) -> list["Post"]:
        return await QueryRepository(info.context["db"]).find("tv_post", where={"userId": self.id})


@input
class CreateUserInput:
    organisation: str  # Organisation identifier
    identifier: str  # Username
    name: str
    email: str
