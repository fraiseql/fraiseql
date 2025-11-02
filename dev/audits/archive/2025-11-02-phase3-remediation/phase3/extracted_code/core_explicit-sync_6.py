# Extracted from: docs/core/explicit-sync.md
# Block number: 6
async def update_post(post_id: UUID, data: dict, background_tasks: BackgroundTasks):
    """Update post and defer sync to background."""
    # 1. Write to command side
    await db.execute("UPDATE tb_post SET ... WHERE id = $1", post_id)

    # 2. DEFERRED SYNC (non-blocking)
    background_tasks.add_task(sync.sync_post, [post_id])

    # 3. Return immediately (sync happens in background)
    return {"status": "updated", "id": str(post_id)}
