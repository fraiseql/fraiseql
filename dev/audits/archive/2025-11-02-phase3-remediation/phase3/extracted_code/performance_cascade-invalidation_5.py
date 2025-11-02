# Extracted from: docs/performance/cascade-invalidation.md
# Block number: 5
from fraiseql.caching import CacheInvalidationRule

# Define custom CASCADE rule
rule = CacheInvalidationRule(
    entity_type="user",
    cascade_to=[
        "post:author:{id}",  # Invalidate all posts by this user
        "user:{id}:followers",  # Invalidate follower list
        "feed:follower:*",  # Invalidate feeds for all followers
    ],
)

# Register the rule
await cache.register_cascade_rule(rule)
