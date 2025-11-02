# Extracted from: docs/core/database-api.md
# Block number: 48
# Good: Check total count
data, total = await repo.select_from_json_view(
    tenant_id, "v_orders", options=QueryOptions(pagination=PaginationInput(limit=20, offset=0))
)
has_next_page = len(data) + offset < total

# Avoid: Assuming more results exist
