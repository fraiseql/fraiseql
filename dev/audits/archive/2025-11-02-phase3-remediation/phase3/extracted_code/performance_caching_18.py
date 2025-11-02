# Extracted from: docs/performance/caching.md
# Block number: 18
from fraiseql import query
from fraiseql.caching import cache_result


@query
@cache_result(ttl=600, key_prefix="top_products")
async def get_top_products(info, category: str, limit: int = 10) -> list[Product]:
    """Get top products by category (cached)."""
    tenant_id = info.context["tenant_id"]
    db = info.context["db"]

    return await db.find(
        "products",
        category=category,
        status="published",
        order_by=[("sales_count", "DESC")],
        limit=limit,
    )
