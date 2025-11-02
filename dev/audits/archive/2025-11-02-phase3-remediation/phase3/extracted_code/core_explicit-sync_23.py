# Extracted from: docs/core/explicit-sync.md
# Block number: 23
# ✅ Good: Batch sync
post_ids = await create_many_posts(...)
await sync.sync_post(post_ids)  # One call

# ❌ Bad: Individual syncs
for post_id in post_ids:
    await sync.sync_post([post_id])  # N calls
