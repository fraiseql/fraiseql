# Extracted from: docs/core/explicit-sync.md
# Block number: 5
async def create_many_posts(posts: list[dict]) -> list[UUID]:
    """Create multiple posts and batch sync."""
    post_ids = []

    # 1. Create all posts (command side)
    for post_data in posts:
        post_id = await db.execute(
            "INSERT INTO tb_post (...) VALUES (...) RETURNING id",
            post_data["title"],
            post_data["content"],
            post_data["author_id"],
        )
        post_ids.append(post_id)

    # 2. BATCH SYNC (much faster than individual syncs!)
    await sync.sync_post(post_ids, mode="incremental")

    return post_ids
