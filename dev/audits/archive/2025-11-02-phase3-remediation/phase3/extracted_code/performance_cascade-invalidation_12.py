# Extracted from: docs/performance/cascade-invalidation.md
# Block number: 12
# ✅ Batch invalidations
user_ids = [user1, user2, user3]

# Single CASCADE operation for all users
await cache.invalidate_batch([f"user:{uid}" for uid in user_ids])

# CASCADE propagates efficiently
