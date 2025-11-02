# Extracted from: docs/core/explicit-sync.md
# Block number: 24
import time


async def sync_post(self, post_ids: list[UUID]):
    start = time.time()

    # Do sync...

    duration_ms = (time.time() - start) * 1000
    await self._log_sync("post", post_ids, duration_ms)

    if duration_ms > 50:
        logger.warning(f"Slow sync: {duration_ms}ms for {len(post_ids)} posts")
