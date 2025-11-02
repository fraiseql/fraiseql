# Extracted from: docs/core/explicit-sync.md
# Block number: 4
@strawberry.mutation
async def create_post(self, info, title: str, content: str, author_id: str) -> Post:
    """Create a post and sync immediately."""
    pool = info.context["db_pool"]
    sync = info.context["sync"]

    # 1. Write to command side
    post_id = await pool.fetchval(
        "INSERT INTO tb_post (title, content, author_id) VALUES ($1, $2, $3) RETURNING id",
        title,
        content,
        UUID(author_id),
    )

    # 2. EXPLICIT SYNC
    await sync.sync_post([post_id])

    # 3. Also sync author (post count changed)
    await sync.sync_user([UUID(author_id)])

    # 4. Read from query side
    row = await pool.fetchrow("SELECT data FROM tv_post WHERE id = $1", post_id)
    return Post(**row["data"])
