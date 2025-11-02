# Extracted from: docs/advanced/multi-tenancy.md
# Block number: 15
from fraiseql import query
from fraiseql.caching import Cache


class TenantCache:
    """Tenant-aware caching wrapper."""

    def __init__(self, cache: Cache):
        self.cache = cache

    def _tenant_key(self, tenant_id: str, key: str) -> str:
        """Generate tenant-scoped cache key."""
        return f"tenant:{tenant_id}:{key}"

    async def get(self, tenant_id: str, key: str):
        """Get cached value for tenant."""
        return await self.cache.get(self._tenant_key(tenant_id, key))

    async def set(self, tenant_id: str, key: str, value, ttl: int = 300):
        """Set cached value for tenant."""
        return await self.cache.set(self._tenant_key(tenant_id, key), value, ttl=ttl)

    async def delete(self, tenant_id: str, key: str):
        """Delete cached value for tenant."""
        return await self.cache.delete(self._tenant_key(tenant_id, key))

    async def clear_tenant(self, tenant_id: str):
        """Clear all cache for tenant."""
        pattern = f"tenant:{tenant_id}:*"
        await self.cache.delete_pattern(pattern)


# Usage
tenant_cache = TenantCache(cache)


@query
async def get_products(info) -> list[Product]:
    """Get products with tenant-aware caching."""
    tenant_id = info.context["tenant_id"]

    # Check cache
    cached = await tenant_cache.get(tenant_id, "products")
    if cached:
        return cached

    # Fetch from database
    async with db.connection() as conn:
        result = await conn.execute("SELECT * FROM products WHERE tenant_id = $1", tenant_id)
        products = [Product(**row) for row in await result.fetchall()]

    # Cache result
    await tenant_cache.set(tenant_id, "products", products, ttl=600)
    return products
