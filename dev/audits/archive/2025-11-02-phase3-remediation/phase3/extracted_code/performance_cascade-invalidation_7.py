# Extracted from: docs/performance/cascade-invalidation.md
# Block number: 7
# User ↔ Post (both directions)

# Forward: User → Post
user_to_post = CacheInvalidationRule(entity_type="user", cascade_to=["post:author:{id}"])

# Backward: Post → User
post_to_user = CacheInvalidationRule(
    entity_type="post",
    cascade_to=["user:{author_id}"],  # Invalidate author's cache
)

# When post changes, author's cache is invalidated
# When user changes, their posts are invalidated
