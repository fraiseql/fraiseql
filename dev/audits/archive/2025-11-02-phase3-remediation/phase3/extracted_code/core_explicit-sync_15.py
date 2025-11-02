# Extracted from: docs/core/explicit-sync.md
# Block number: 15
# Enable sync logging
import logging

logging.getLogger("fraiseql.sync").setLevel(logging.DEBUG)

# Log output:
# [SYNC] sync_post: Syncing post 123...
# [SYNC] → Fetching from tb_post
# [SYNC] → Building JSONB structure
# [SYNC] → Upserting to tv_post
# [SYNC] ✓ Sync complete in 5.2ms
