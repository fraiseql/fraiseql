# Extracted from: docs/performance/caching.md
# Block number: 4
from fraiseql.caching import PostgresCache

cache = PostgresCache(
    connection_pool=pool,
    table_name="fraiseql_cache",  # Cache table name
    auto_initialize=True,  # Auto-create table on first use
)
