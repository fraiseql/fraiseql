# Extracted from: docs/core/explicit-sync.md
# Block number: 7
async def update_post(post_id: UUID, old_data: dict, new_data: dict):
    """Only sync if data changed in a way that affects queries."""
    # Update command side
    await db.execute("UPDATE tb_post SET ... WHERE id = $1", post_id)

    # Only sync if title or content changed (not view count)
    if new_data["title"] != old_data["title"] or new_data["content"] != old_data["content"]:
        await sync.sync_post([post_id])
    # else: Skip sync (view count doesn't appear in queries)
