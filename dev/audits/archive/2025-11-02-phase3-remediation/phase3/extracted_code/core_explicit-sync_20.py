# Extracted from: docs/core/explicit-sync.md
# Block number: 20
async def sync_with_validation(self, post_ids: list[UUID]):
    """Sync with validation to ensure data integrity."""
    for post_id in post_ids:
        # Fetch from tb_post
        post_data = await conn.fetchrow("SELECT * FROM tb_post WHERE id = $1", post_id)

        if not post_data:
            logger.warning(f"Post {post_id} not found in tb_post, skipping sync")
            continue

        # Validate author exists
        author = await conn.fetchrow("SELECT * FROM tb_user WHERE id = $1", post_data["author_id"])
        if not author:
            logger.error(f"Author {post_data['author_id']} not found for post {post_id}")
            continue

        # Proceed with sync
        await self._do_sync(post_id, post_data, author)
