# Extracted from: docs/core/database-api.md
# Block number: 41
def generate_pagination_query(
    base_query: Composable,
    order_by_clause: Composable,
    aggregated_columns: Sequence[Composed],
    pagination: PaginationInput | None
) -> tuple[Composed, tuple[int, int]]
