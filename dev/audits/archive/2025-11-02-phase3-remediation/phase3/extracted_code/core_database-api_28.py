# Extracted from: docs/core/database-api.md
# Block number: 28
@dataclass
class PaginationInput:
    limit: int | None = None
    offset: int | None = None
