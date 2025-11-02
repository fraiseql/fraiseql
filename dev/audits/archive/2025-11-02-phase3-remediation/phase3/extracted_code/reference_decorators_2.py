# Extracted from: docs/reference/decorators.md
# Block number: 2
from fraiseql import input


@input
class InputName:
    field1: str
    field2: int | None = None
