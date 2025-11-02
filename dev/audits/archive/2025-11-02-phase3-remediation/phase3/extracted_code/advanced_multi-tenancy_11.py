# Extracted from: docs/advanced/multi-tenancy.md
# Block number: 11
class TenantPoolManager:
    """Manage connection pool per tenant."""

    def __init__(self, base_db_url: str, pool_size: int = 5):
        self.base_db_url = base_db_url
        self.pool_size = pool_size
        self.pools: dict[str, DatabasePool] = {}

    async def get_pool(self, tenant_id: str) -> DatabasePool:
        """Get or create pool for tenant."""
        if tenant_id not in self.pools:
            # Option 1: Different database per tenant
            db_url = f"{self.base_db_url.rsplit('/', 1)[0]}/tenant_{tenant_id}"

            # Option 2: Same database, different schema
            # db_url = self.base_db_url
            # Set search_path after connection

            self.pools[tenant_id] = DatabasePool(
                db_url, min_size=self.pool_size, max_size=self.pool_size * 2
            )

        return self.pools[tenant_id]

    async def close_pool(self, tenant_id: str):
        """Close pool for inactive tenant."""
        if tenant_id in self.pools:
            await self.pools[tenant_id].close()
            del self.pools[tenant_id]

    async def close_all(self):
        """Close all tenant pools."""
        for pool in self.pools.values():
            await pool.close()
        self.pools.clear()


# Usage
pool_manager = TenantPoolManager("postgresql://user:pass@localhost/app")


@app.middleware("http")
async def tenant_pool_middleware(request: Request, call_next):
    tenant_id = await resolve_tenant_id(request)
    request.state.db_pool = await pool_manager.get_pool(tenant_id)
    response = await call_next(request)
    return response
