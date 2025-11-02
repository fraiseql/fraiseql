# Extracted from: docs/core/database-api.md
# Block number: 40
def generate_order_by_clause(
    order_by: OrderByInstructions,
    aggregations: dict[str, str],
    view_name: str,
    alias_mapping: dict[str, str] | None = None,
    dimension_key: str | None = None
) -> tuple[Composed, list[Composed]]
