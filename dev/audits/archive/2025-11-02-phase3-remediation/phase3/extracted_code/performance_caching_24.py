# Extracted from: docs/performance/caching.md
# Block number: 24
# Get cache statistics
stats = await result_cache.get_stats()
print(f"Hit rate: {stats['hit_rate']:.1%}")
print(f"Hits: {stats['hits']}, Misses: {stats['misses']}")
print(f"Total entries: {stats['total_entries']}")
print(f"Expired entries: {stats['expired_entries']}")
print(f"Table size: {stats['table_size_bytes'] / 1024 / 1024:.2f} MB")
