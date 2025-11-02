# Extracted from: docs/reference/database.md
# Block number: 5
async def find_one(
    view_name: str,
    where: dict | WhereType | None = None,
    **kwargs
) -> dict[str, Any] | None
