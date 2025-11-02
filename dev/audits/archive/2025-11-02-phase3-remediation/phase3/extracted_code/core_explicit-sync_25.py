# Extracted from: docs/core/explicit-sync.md
# Block number: 25
async def sync_post(self, post_ids: list[UUID]):
    for post_id in post_ids:
        try:
            await self._do_sync(post_id)
        except Exception as e:
            logger.error(f"Sync failed for post {post_id}: {e}")
            await self._log_sync_error("post", post_id, str(e))
            # Continue with next post (don't fail entire batch)
