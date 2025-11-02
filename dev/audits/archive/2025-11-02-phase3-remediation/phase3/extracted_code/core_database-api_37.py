# Extracted from: docs/core/database-api.md
# Block number: 37
options = QueryOptions(ignore_tenant_column=True)

data, total = await repo.select_from_json_view(
    tenant_id=tenant_id, view_name="v_orders", options=options
)
# No tenant_id filter applied
