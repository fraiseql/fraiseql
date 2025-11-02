# Extracted from: docs/core/explicit-sync.md
# Block number: 2
from uuid import UUID

import asyncpg


class EntitySync:
    """Handles synchronization from tb_* to tv_* tables."""

    def __init__(self, pool: asyncpg.Pool):
        self.pool = pool

    async def sync_post(self, post_ids: list[UUID], mode: str = "incremental") -> None:
        """Sync posts from tb_post to tv_post.

        Args:
            post_ids: List of post IDs to sync
            mode: 'incremental' (default) or 'full'

        Example:
            await sync.sync_post([post_id], mode='incremental')
        """
        async with self.pool.acquire() as conn:
            for post_id in post_ids:
                # 1. Fetch from command side (tb_post) with joins
                post_data = await conn.fetchrow(
                    """
                    SELECT
                        p.id,
                        p.title,
                        p.content,
                        p.published,
                        p.created_at,
                        jsonb_build_object(
                            'id', u.id,
                            'username', u.username,
                            'fullName', u.full_name
                        ) as author
                    FROM tb_post p
                    JOIN tb_user u ON u.id = p.author_id
                    WHERE p.id = $1
                    """,
                    post_id,
                )

                if not post_data:
                    continue

                # 2. Build denormalized JSONB structure
                jsonb_data = {
                    "id": str(post_data["id"]),
                    "title": post_data["title"],
                    "content": post_data["content"],
                    "published": post_data["published"],
                    "author": post_data["author"],
                    "createdAt": post_data["created_at"].isoformat(),
                }

                # 3. Upsert to query side (tv_post)
                await conn.execute(
                    """
                    INSERT INTO tv_post (id, data, updated_at)
                    VALUES ($1, $2, NOW())
                    ON CONFLICT (id) DO UPDATE
                    SET data = $2, updated_at = NOW()
                    """,
                    post_id,
                    jsonb_data,
                )

                # 4. Log metrics (optional but recommended)
                await self._log_sync("post", post_id, mode, duration_ms=5, success=True)
