# Extracted from: docs/performance/caching-migration.md
# Block number: 5
# Check that tenant_id is in context
assert base_repo.context.get("tenant_id") is not None, "tenant_id required!"
