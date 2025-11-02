# Extracted from: docs/performance/caching.md
# Block number: 30
# Manual invalidation
await cached_repo.execute_function("create_product", product_data)

# Or explicit
await result_cache.invalidate_pattern(key_builder.build_mutation_pattern("product"))
