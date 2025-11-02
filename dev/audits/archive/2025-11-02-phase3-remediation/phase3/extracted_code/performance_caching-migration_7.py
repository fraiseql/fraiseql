# Extracted from: docs/performance/caching-migration.md
# Block number: 7
# Option 1: Use a constant tenant_id
context = {"tenant_id": "single-tenant"}

# Option 2: Don't set tenant_id (cache keys won't include it)
context = {}  # OK for single-tenant apps

# Option 3: Use another identifier (user_id, org_id, etc.)
context = {"tenant_id": request.state.organization_id}
