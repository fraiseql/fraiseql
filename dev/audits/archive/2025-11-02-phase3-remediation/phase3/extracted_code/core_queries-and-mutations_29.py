# Extracted from: docs/core/queries-and-mutations.md
# Block number: 29
@subscription
async def on_user_posts(info, user_id: UUID) -> AsyncGenerator[Post, None]:
    # Only yield posts from specific user
    async for post in post_event_stream():
        if post.user_id == user_id:
            yield post
