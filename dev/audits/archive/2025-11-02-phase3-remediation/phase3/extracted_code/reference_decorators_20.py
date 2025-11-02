# Extracted from: docs/reference/decorators.md
# Block number: 20
from typing import AsyncGenerator


@subscription
async def on_post_created(info) -> AsyncGenerator[Post]:
    async for post in post_event_stream():
        yield post


@subscription
async def on_user_posts(info, user_id: UUID) -> AsyncGenerator[Post]:
    async for post in post_event_stream():
        if post.user_id == user_id:
            yield post
