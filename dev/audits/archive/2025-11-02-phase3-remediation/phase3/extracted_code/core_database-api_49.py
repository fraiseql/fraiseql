# Extracted from: docs/core/database-api.md
# Block number: 49
# Good: Automatic tenant isolation
data, total = await repo.select_from_json_view(tenant_id, "v_orders")

# Avoid: Manual tenant filtering in WHERE clauses
