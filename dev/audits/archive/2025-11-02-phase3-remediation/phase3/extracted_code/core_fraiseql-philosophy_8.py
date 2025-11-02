# Extracted from: docs/core/fraiseql-philosophy.md
# Block number: 8
from fraiseql import query


@query
async def orders(info) -> list[Order]:
    db = info.context["db"]
    # Automatically filtered by tenant from JWT!
    return await db.find("v_order")
