# Extracted from: docs/core/explicit-sync.md
# Block number: 19
async def like_post(post_id: UUID, user_id: UUID):
    """Optimistic sync: update cache immediately, sync later."""
    # 1. Update cache optimistically (fast!)
    cached_post = await cache.get(f"post:{post_id}")
    cached_post["likes"] += 1
    await cache.set(f"post:{post_id}", cached_post)

    # 2. Write to command side
    await db.execute(
        "INSERT INTO tb_post_like (post_id, user_id) VALUES ($1, $2)", post_id, user_id
    )

    # 3. Sync in background (eventual consistency)
    background_tasks.add_task(sync.sync_post, [post_id])

    # User sees immediate update!
