# Extracted from: docs/core/explicit-sync.md
# Block number: 10
import asyncio

# ✅ Sync multiple entity types in parallel
await asyncio.gather(
    sync.sync_post(post_ids), sync.sync_user(user_ids), sync.sync_comment(comment_ids)
)

# All syncs happen concurrently!
