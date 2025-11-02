# Extracted from: docs/reference/decorators.md
# Block number: 1
from fraiseql import type, query, mutation, input, field

@type(
    sql_source: str | None = None,
    jsonb_column: str | None = "data",
    implements: list[type] | None = None,
    resolve_nested: bool = False
)
