# Extracted from: docs/core/explicit-sync.md
# Block number: 12
# Incremental: Sync specific entities (fast)
await sync.sync_post([post_id], mode="incremental")  # ~5ms

# Full: Sync all entities (slow, but thorough)
await sync.sync_all_posts(mode="full")  # ~500ms for 1000 posts

# Use incremental for:
# - After mutations
# - Real-time updates

# Use full for:
# - Initial setup
# - Recovery from errors
# - Scheduled maintenance
