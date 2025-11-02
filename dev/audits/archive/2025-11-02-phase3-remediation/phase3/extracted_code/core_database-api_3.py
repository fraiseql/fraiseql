# Extracted from: docs/core/database-api.md
# Block number: 3
from psycopg_pool import AsyncConnectionPool

pool = AsyncConnectionPool(conninfo="postgresql://localhost/mydb", min_size=5, max_size=20)

repo = PsycopgRepository(
    pool=pool,
    tenant_id="tenant-123",  # Optional: tenant context
)
