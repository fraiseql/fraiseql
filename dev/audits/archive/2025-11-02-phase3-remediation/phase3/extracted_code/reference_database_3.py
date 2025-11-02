# Extracted from: docs/reference/database.md
# Block number: 3
async def find(
    view_name: str,
    where: dict | WhereType | None = None,
    limit: int | None = None,
    offset: int | None = None,
    order_by: str | OrderByType | None = None
) -> list[dict[str, Any]]
