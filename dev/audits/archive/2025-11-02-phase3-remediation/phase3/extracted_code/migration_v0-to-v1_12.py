# Extracted from: docs/migration/v0-to-v1.md
# Block number: 12
from fraiseql import dataloader, field


@field
@dataloader
async def posts(user: User, info: Info) -> list[Post]:
    return await info.context.repo.find("posts_view", user_id=user.id)
