# Extracted from: docs/performance/cascade-invalidation.md
# Block number: 9
# User changes → cascades to 10 posts
# Cost: 1ms + (10 × 0.5ms) = 6ms total

# Still much faster than cache miss!
# Cache miss would cost: ~50ms database query
