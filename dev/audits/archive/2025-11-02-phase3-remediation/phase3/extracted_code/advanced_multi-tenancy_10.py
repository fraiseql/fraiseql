# Extracted from: docs/advanced/multi-tenancy.md
# Block number: 10
from fraiseql.db import DatabasePool
from fraiseql.fastapi.config import FraiseQLConfig

config = FraiseQLConfig(
    database_url="postgresql://user:pass@localhost/app",
    database_pool_size=20,
    database_max_overflow=10,
)

# Single pool shared by all tenants
pool = DatabasePool(
    config.database_url,
    min_size=config.database_pool_size,
    max_size=config.database_pool_size + config.database_max_overflow,
)

# Use set_tenant_context before queries
async with pool.connection() as conn:
    await conn.execute("SET LOCAL app.current_tenant_id = $1", tenant_id)
    # All queries now filtered by tenant_id via RLS
