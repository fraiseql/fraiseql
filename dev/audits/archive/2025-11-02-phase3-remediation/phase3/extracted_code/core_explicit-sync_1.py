# Extracted from: docs/core/explicit-sync.md
# Block number: 1
# ✅ Explicit sync (visible in your code)
async def create_post(title: str, author_id: UUID) -> Post:
    # 1. Write to command side
    post_id = await db.execute(
        "INSERT INTO tb_post (title, author_id) VALUES ($1, $2) RETURNING id", title, author_id
    )

    # 2. EXPLICIT SYNC 👈 THIS IS IN YOUR CODE!
    await sync.sync_post([post_id], mode="incremental")

    # 3. Read from query side
    return await db.fetchrow("SELECT data FROM tv_post WHERE id = $1", post_id)
