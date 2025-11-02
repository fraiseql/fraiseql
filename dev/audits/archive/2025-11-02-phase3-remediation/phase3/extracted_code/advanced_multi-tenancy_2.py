# Extracted from: docs/advanced/multi-tenancy.md
# Block number: 2
from fraiseql.db import DatabasePool


class SchemaPerTenantManager:
    """Manage schema-per-tenant pattern."""

    def __init__(self, db_pool: DatabasePool):
        self.db_pool = db_pool

    async def set_search_path(self, tenant_id: str):
        """Set PostgreSQL search_path to tenant schema."""
        async with self.db_pool.connection() as conn:
            await conn.execute(f"SET search_path TO tenant_{tenant_id}, public")
