# Extracted from: docs/advanced/multi-tenancy.md
# Block number: 1
from fraiseql.db import DatabasePool


class TenantDatabaseManager:
    """Manage separate database per tenant."""

    def __init__(self, base_url: str):
        self.base_url = base_url
        self.pools: dict[str, DatabasePool] = {}

    async def get_pool(self, tenant_id: str) -> DatabasePool:
        """Get database pool for specific tenant."""
        if tenant_id not in self.pools:
            # Create tenant-specific connection
            db_url = f"{self.base_url.rsplit('/', 1)[0]}/tenant_{tenant_id}"
            self.pools[tenant_id] = DatabasePool(db_url)

        return self.pools[tenant_id]

    async def close_all(self):
        """Close all tenant database pools."""
        for pool in self.pools.values():
            await pool.close()
