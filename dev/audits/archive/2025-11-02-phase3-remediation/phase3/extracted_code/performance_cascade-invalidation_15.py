# Extracted from: docs/performance/cascade-invalidation.md
# Block number: 15
# Invalidate entire post
await cache.invalidate("post:123")

# Or: Invalidate only post title
await cache.invalidate_field("post:123", field="title")

# Author name changed? Only invalidate author field
await cache.invalidate_field("post:*", field="author.name")
