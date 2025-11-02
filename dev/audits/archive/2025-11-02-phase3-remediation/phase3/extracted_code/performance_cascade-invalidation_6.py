# Extracted from: docs/performance/cascade-invalidation.md
# Block number: 6
# User → Post → Comment (2 levels deep)
user_rule = CacheInvalidationRule(
    entity_type="user",
    cascade_to=[
        "post:author:{id}",  # Direct: User's posts
        "comment:post_author:{id}",  # Indirect: Comments on user's posts
    ],
)

# When user changes:
# 1. Invalidate user's posts
# 2. Invalidate comments on those posts
# Result: Full cascade through 2 levels
