# Extracted from: docs/performance/cascade-invalidation.md
# Block number: 11
# ❌ Bad: Cascade everything
rule = CacheInvalidationRule(
    entity_type="user",
    cascade_to=["*"],  # Invalidates EVERYTHING!
)

# ✅ Good: Cascade specific patterns
rule = CacheInvalidationRule(
    entity_type="user",
    cascade_to=["post:author:{id}", "comment:author:{id}"],  # Only what's needed
)
