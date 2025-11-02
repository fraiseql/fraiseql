# Extracted from: docs/advanced/multi-tenancy.md
# Block number: 12
class HybridPoolManager:
    """Hybrid pool management based on tenant size."""

    def __init__(self, shared_db_url: str):
        self.shared_pool = DatabasePool(shared_db_url, min_size=20, max_size=50)
        self.dedicated_pools: dict[str, DatabasePool] = {}
        self.large_tenants = set()  # Tenants with dedicated pools

    async def get_pool(self, tenant_id: str) -> DatabasePool:
        """Get pool for tenant based on size."""
        if tenant_id in self.large_tenants:
            return self.dedicated_pools[tenant_id]
        return self.shared_pool

    async def promote_to_dedicated(self, tenant_id: str):
        """Promote tenant to dedicated pool."""
        if tenant_id not in self.large_tenants:
            db_url = f"postgresql://user:pass@localhost/tenant_{tenant_id}"
            self.dedicated_pools[tenant_id] = DatabasePool(db_url, min_size=10, max_size=20)
            self.large_tenants.add(tenant_id)
