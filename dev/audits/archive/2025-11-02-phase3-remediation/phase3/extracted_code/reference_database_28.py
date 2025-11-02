# Extracted from: docs/reference/database.md
# Block number: 28
from fraiseql import query


@query
async def products(info) -> list[Product]:
    """Get products for current tenant.

    Automatically filtered by tenant_id from JWT token.
    No need to pass tenant_id explicitly!
    """
    db = info.context["db"]
    return await db.find("v_product")
