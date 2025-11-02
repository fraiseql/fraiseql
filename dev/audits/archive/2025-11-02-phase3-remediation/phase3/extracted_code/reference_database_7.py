# Extracted from: docs/reference/database.md
# Block number: 7
async def paginate(
    view_name: str,
    first: int | None = None,
    after: str | None = None,
    last: int | None = None,
    before: str | None = None,
    filters: dict | None = None,
    order_by: str = "id",
    include_total: bool = True,
    jsonb_extraction: bool | None = None,
    jsonb_column: str | None = None
) -> dict[str, Any]
