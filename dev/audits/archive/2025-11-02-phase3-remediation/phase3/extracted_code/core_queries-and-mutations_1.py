# Extracted from: docs/core/queries-and-mutations.md
# Block number: 1
from fraiseql import query


@query
async def query_name(info, param1: Type1, param2: Type2 = default) -> ReturnType:
    pass
