# Extracted from: docs/architecture/type-operator-architecture.md
# Block number: 16
def _looks_like_daterange_value(self, val: Any, op: str) -> bool:
    """Detect PostgreSQL daterange format."""
    # Pattern: [2024-01-01,2024-12-31] or (2024-01-01,2024-12-31)

    pattern = r"^\[?\(?(\d{4}-\d{2}-\d{2}),\s*(\d{4}-\d{2}-\d{2})\)?\]?$"

    return bool(re.match(pattern, val))
