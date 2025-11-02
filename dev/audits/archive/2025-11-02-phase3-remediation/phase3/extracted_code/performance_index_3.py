# Extracted from: docs/performance/index.md
# Block number: 3
from fraiseql import dataloader


@field
@dataloader
async def posts(user: User, info: Info) -> list[Post]:
    # Automatically batched
    return await info.context.repo.find("posts_view", user_id=user.id)
