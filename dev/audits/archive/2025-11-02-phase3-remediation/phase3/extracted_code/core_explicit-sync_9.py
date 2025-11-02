# Extracted from: docs/core/explicit-sync.md
# Block number: 9
# ❌ Slow: Individual syncs
for post_id in post_ids:
    await sync.sync_post([post_id])  # N database queries

# ✅ Fast: Batch sync
await sync.sync_post(post_ids)  # 1 database query
