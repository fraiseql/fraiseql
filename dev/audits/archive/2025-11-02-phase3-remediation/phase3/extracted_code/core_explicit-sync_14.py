# Extracted from: docs/core/explicit-sync.md
# Block number: 14
@pytest.mark.asyncio
async def test_sync_integration(db_pool):
    """Test actual sync operation."""
    sync = EntitySync(db_pool)

    # Create in command side
    post_id = await db_pool.fetchval(
        "INSERT INTO tb_post (...) VALUES (...) RETURNING id", "Test", "...", author_id
    )

    # Sync to query side
    await sync.sync_post([post_id])

    # Verify query side has data
    row = await db_pool.fetchrow("SELECT data FROM tv_post WHERE id = $1", post_id)
    assert row is not None
    assert row["data"]["title"] == "Test"
