# Extracted from: docs/advanced/nested-array-filtering.md
# Block number: 4
# Filtering happens after data is loaded
async def _apply_where_filter_to_array(items: list, where_filter: Any) -> list:
    """Apply where filtering to an array of items."""
    filtered_items = []
    for item in items:  # ← Iterates through each item in memory
        if await _item_matches_where_criteria(item, where_filter):
            filtered_items.append(item)
    return filtered_items
