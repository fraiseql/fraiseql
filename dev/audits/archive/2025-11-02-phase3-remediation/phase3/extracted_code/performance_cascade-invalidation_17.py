# Extracted from: docs/performance/cascade-invalidation.md
# Block number: 17
# Get CASCADE statistics
stats = await cache.get_cascade_stats()

print(stats)
# {
#     "total_invalidations_24h": 15234,
#     "cascade_triggered": 8521,
#     "avg_cascade_depth": 1.8,
#     "avg_cascade_time_ms": 4.2,
#     "most_frequent_cascades": [
#         {"pattern": "user -> post", "count": 4521},
#         {"pattern": "post -> comment", "count": 2134}
#     ]
# }
