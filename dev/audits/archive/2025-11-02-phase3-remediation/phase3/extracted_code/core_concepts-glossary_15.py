# Extracted from: docs/core/concepts-glossary.md
# Block number: 15
from fraiseql.monitoring import apq_metrics

# Check APQ cache statistics
stats = await apq_metrics.get_stats()
print(f"Cache hits: {stats.hits}")
print(f"Cache misses: {stats.misses}")
print(f"Hit rate: {stats.hit_rate:.2%}")
print(f"Cached queries: {stats.total_queries}")
