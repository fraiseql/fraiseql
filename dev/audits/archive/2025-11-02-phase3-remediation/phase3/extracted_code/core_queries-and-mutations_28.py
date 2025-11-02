# Extracted from: docs/core/queries-and-mutations.md
# Block number: 28
from typing import AsyncGenerator


@subscription
async def on_post_created(info) -> AsyncGenerator[Post]:
    # Subscribe to post creation events
    async for post in post_event_stream():
        yield post
