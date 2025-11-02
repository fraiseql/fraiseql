# Extracted from: docs/advanced/database-patterns.md
# Block number: 2
from fraiseql import query


@query
async def get_orders(info, status: str | None = None) -> list[Order]:
    db = info.context["db"]
    tenant_id = info.context["tenant_id"]

    where = {"tenant_id": tenant_id}
    if status:
        where["status"] = status

    return await db.find("v_orders", where=where)
