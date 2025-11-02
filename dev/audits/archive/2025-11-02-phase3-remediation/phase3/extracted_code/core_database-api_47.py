# Extracted from: docs/core/database-api.md
# Block number: 47
# Good: Shared connection pool
pool = AsyncConnectionPool(conninfo=DATABASE_URL, min_size=5, max_size=20)
repo = PsycopgRepository(pool)

# Avoid: Creating connections per request
