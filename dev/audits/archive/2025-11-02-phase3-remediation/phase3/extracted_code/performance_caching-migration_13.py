# Extracted from: docs/performance/caching-migration.md
# Block number: 13
stats = await app.state.result_cache.get_stats()
print(f"Cache hit rate: {stats['hit_rate']:.1%}")
print(f"Total entries: {stats['total_entries']}")
print(f"Hits: {stats['hits']}, Misses: {stats['misses']}")
