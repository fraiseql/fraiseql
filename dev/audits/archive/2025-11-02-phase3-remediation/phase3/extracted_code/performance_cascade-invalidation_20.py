# Extracted from: docs/performance/cascade-invalidation.md
# Block number: 20
# Command side: Update tb_user
await db.execute("UPDATE tb_user SET name = $1 WHERE id = $2", "Alice Smith", user_id)

# Explicit sync to query side
await sync.sync_user([user_id])

# CASCADE: tv_user changed → invalidate related caches
# - user:{user_id}:posts
# - post:* where author_id = {user_id}

# Next query will re-read from tv_post (which has updated author name)
