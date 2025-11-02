# Extracted from: docs/core/database-api.md
# Block number: 9
query = SQL("SELECT json_data FROM {} WHERE tenant_id = {}").format(
    Identifier("v_orders"), Placeholder()
)

orders = await repo.fetch_all(query, (tenant_id,))
