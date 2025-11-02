# Extracted from: docs/performance/cascade-invalidation.md
# Block number: 18
# Visualize CASCADE graph
cascade_graph = await cache.get_cascade_graph()

# Output:
# user:123
#  ├─> post:author:123 (12 keys invalidated)
#  ├─> comment:author:123 (45 keys invalidated)
#  └─> follower:following:123 (234 keys invalidated)
