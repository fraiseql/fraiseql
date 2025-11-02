# Extracted from: docs/core/explicit-sync.md
# Block number: 16
async def sync_post_with_ivm(self, post_ids: list[UUID]):
    """Sync with IVM extension (faster!)."""
    # IVM automatically maintains tv_post when tb_post changes
    # Just trigger a refresh
    await self.pool.execute("REFRESH MATERIALIZED VIEW CONCURRENTLY tv_post")
