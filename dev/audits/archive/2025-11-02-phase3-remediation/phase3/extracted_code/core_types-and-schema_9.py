# Extracted from: docs/core/types-and-schema.md
# Block number: 9
from fraiseql import input


@input
class InputName:
    field1: str
    field2: int | None = None
