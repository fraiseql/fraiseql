# Extracted from: docs/core/explicit-sync.md
# Block number: 18
async def create_comment(post_id: UUID, author_id: UUID, content: str):
    """Create comment and sync all affected entities."""
    # 1. Write to command side
    comment_id = await db.execute(
        "INSERT INTO tb_comment (...) VALUES (...) RETURNING id", post_id, author_id, content
    )

    # 2. SYNC ALL AFFECTED ENTITIES
    await asyncio.gather(
        sync.sync_comment([comment_id]),  # New comment
        sync.sync_post([post_id]),  # Post comment count changed
        sync.sync_user([author_id]),  # User comment count changed
    )

    # All entities now consistent!
