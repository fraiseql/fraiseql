# Extracted from: docs/performance/caching.md
# Block number: 27
# ✅ CORRECT: tenant_id in context
repo = FraiseQLRepository(pool, context={"tenant_id": tenant_id})

# ❌ WRONG: Missing tenant_id (security issue!)
repo = FraiseQLRepository(pool, context={})
