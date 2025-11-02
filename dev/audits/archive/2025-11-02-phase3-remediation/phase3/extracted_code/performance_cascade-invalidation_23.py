# Extracted from: docs/performance/cascade-invalidation.md
# Block number: 23
# ✅ CASCADE only what's needed
user_rule = CacheInvalidationRule("user", cascade_to=["post:author:{id}", "comment:author:{id}"])

# ❌ Don't cascade everything
user_rule = CacheInvalidationRule("user", cascade_to=["*"])
