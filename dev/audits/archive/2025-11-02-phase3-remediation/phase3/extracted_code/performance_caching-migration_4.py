# Extracted from: docs/performance/caching-migration.md
# Block number: 4
# ✅ CORRECT: tenant_id in context
context = {"tenant_id": request.state.tenant_id}

# ❌ WRONG: Missing tenant_id (security risk!)
context = {}
