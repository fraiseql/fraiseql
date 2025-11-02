# Extracted from: docs/core/fraiseql-philosophy.md
# Block number: 14
from fraiseql import query


@query
async def order_totals(info, id: UUID) -> OrderTotals:
    db = info.context["db"]
    # Database does the heavy lifting
    return await db.execute_function("calculate_order_totals", {"order_id": id})
