# Extracted from: docs/performance/cascade-invalidation.md
# Block number: 24
# Test CASCADE in your test suite
async def test_user_cascade():
    # Create user and post
    user_id = await create_user(...)
    post_id = await create_post(author_id=user_id, ...)

    # Cache the post
    post = await cache.get(f"post:{post_id}")

    # Update user
    await update_user(user_id, name="New Name")

    # Verify CASCADE invalidated post cache
    assert await cache.get(f"post:{post_id}") is None
