# Extracted from: docs/core/database-api.md
# Block number: 46
# Good: Structured with QueryOptions
options = QueryOptions(
    filters={"status": "active"},
    pagination=PaginationInput(limit=50, offset=0),
    order_by=OrderByInstructions(instructions=[...]),
)
data, total = await repo.select_from_json_view(tenant_id, "v_orders", options=options)

# Avoid: Raw SQL strings
query = "SELECT * FROM v_orders WHERE status = 'active' LIMIT 50"
