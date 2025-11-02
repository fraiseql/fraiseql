# Extracted from: docs/core/types-and-schema.md
# Block number: 1
from fraiseql import type

@type(
    sql_source: str | None = None,
    jsonb_column: str | None = "data",
    implements: list[type] | None = None,
    resolve_nested: bool = False
)
class TypeName:
    field1: str
    field2: int | None = None
