# Extracted from: docs/performance/caching.md
# Block number: 1
from fraiseql.caching import CachedRepository, PostgresCache, ResultCache
from fraiseql.db import DatabasePool

# Initialize database pool
pool = DatabasePool("postgresql://user:pass@localhost/mydb")

# Create cache backend (PostgreSQL UNLOGGED table)
postgres_cache = PostgresCache(
    connection_pool=pool,
    table_name="fraiseql_cache",  # default
    auto_initialize=True,
)

# Wrap with result cache (adds statistics tracking)
result_cache = ResultCache(backend=postgres_cache, default_ttl=300)

# Wrap repository with caching
from fraiseql.db import FraiseQLRepository

base_repo = FraiseQLRepository(
    pool=pool,
    context={"tenant_id": tenant_id},  # CRITICAL for multi-tenant!
)

cached_repo = CachedRepository(base_repository=base_repo, cache=result_cache)

# Use cached repository - automatic caching!
users = await cached_repo.find("users", status="active")
