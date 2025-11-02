# Extracted from: docs/case-studies/template.md
# Block number: 1
# [Include actual caching pattern they use]
await cache.set(f"user:{user_id}", user_data, ttl=3600)
