# Extracted from: docs/core/explicit-sync.md
# Block number: 3
async def sync_post_with_comments(self, post_ids: list[UUID]) -> None:
    """Sync posts with embedded comments (denormalized)."""
    async with self.pool.acquire() as conn:
        for post_id in post_ids:
            # Fetch post
            post_data = await conn.fetchrow("SELECT * FROM tb_post WHERE id = $1", post_id)

            # Fetch comments for this post
            comments = await conn.fetch(
                """
                SELECT
                    c.id,
                    c.content,
                    c.created_at,
                    jsonb_build_object(
                        'id', u.id,
                        'username', u.username
                    ) as author
                FROM tb_comment c
                JOIN tb_user u ON u.id = c.author_id
                WHERE c.post_id = $1
                ORDER BY c.created_at DESC
                """,
                post_id,
            )

            # Build denormalized structure with embedded comments
            jsonb_data = {
                "id": str(post_data["id"]),
                "title": post_data["title"],
                "author": {...},
                "comments": [
                    {
                        "id": str(c["id"]),
                        "content": c["content"],
                        "author": c["author"],
                        "createdAt": c["created_at"].isoformat(),
                    }
                    for c in comments
                ],
            }

            # Upsert to tv_post
            await conn.execute(
                "INSERT INTO tv_post (id, data) VALUES ($1, $2) ON CONFLICT (id) DO UPDATE SET data = $2",
                post_id,
                jsonb_data,
            )
