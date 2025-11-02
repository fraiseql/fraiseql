# Extracted from: docs/reference/decorators.md
# Block number: 5
from fraiseql import query


@query
async def query_name(info, param1: Type1, param2: Type2 = default) -> ReturnType:
    pass
