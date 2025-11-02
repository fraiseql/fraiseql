# Extracted from: docs/core/database-api.md
# Block number: 4
async def select_from_json_view(
    self,
    tenant_id: uuid.UUID,
    view_name: str,
    *,
    options: QueryOptions | None = None,
) -> tuple[Sequence[dict[str, object]], int | None]
