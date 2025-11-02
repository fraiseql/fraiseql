# Extracted from: docs/core/database-api.md
# Block number: 36
repo = PsycopgRepository(pool, tenant_id="tenant-123")

# This query:
data, total = await repo.select_from_json_view(tenant_id=tenant_id, view_name="v_orders")

# Automatically adds: WHERE tenant_id = $1
