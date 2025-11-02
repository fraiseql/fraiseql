# Extracted from: docs/core/explicit-sync.md
# Block number: 8
async def delete_user(user_id: UUID):
    """Delete user and cascade sync related entities."""
    # 1. Get user's posts before deleting
    post_ids = await db.fetch("SELECT id FROM tb_post WHERE author_id = $1", user_id)

    # 2. Delete from command side (CASCADE will delete posts too)
    await db.execute("DELETE FROM tb_user WHERE id = $1", user_id)

    # 3. EXPLICIT CASCADE SYNC
    await sync.delete_user([user_id])
    await sync.delete_post([p["id"] for p in post_ids])

    # Query side is now consistent
