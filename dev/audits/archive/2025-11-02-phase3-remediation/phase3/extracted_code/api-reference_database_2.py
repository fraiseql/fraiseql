# Extracted from: docs/api-reference/database.md
# Block number: 2
async def find(
    self,
    view_name: str,
    limit: int | None = None,
    offset: int | None = None,
    order_by: dict | None = None,
    where: dict | None = None,
    **kwargs,
) -> list[dict]:
    """Find records from a view."""
