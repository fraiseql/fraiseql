# Extracted from: docs/performance/cascade-invalidation.md
# Block number: 8
# Only cascade published posts
published_posts_rule = CacheInvalidationRule(
    entity_type="user",
    cascade_to=["post:author:{id}"],
    condition=lambda data: data.get("published") is True,
)

# CASCADE only triggers for published posts
