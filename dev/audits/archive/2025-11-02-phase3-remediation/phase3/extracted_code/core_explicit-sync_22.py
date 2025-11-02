# Extracted from: docs/core/explicit-sync.md
# Block number: 22
# ✅ Good: Sync immediately
post_id = await create_post(...)
await sync.sync_post([post_id])

# ❌ Bad: Forget to sync
post_id = await create_post(...)
# Oops! Query side is now stale
