# Extracted from: docs/core/explicit-sync.md
# Block number: 21
# In your mutation code
async def create_post(...):
    post_id = await db.execute("INSERT INTO tb_post ...")
    await sync.sync_post([post_id])  # Explicit!
