# Extracted from: docs/reference/database.md
# Block number: 26
from fraiseql import query


@query
async def orders(info) -> list[Order]:
    db = info.context["db"]
    # Automatically filtered by tenant_id from context!
    return await db.find("v_order")
