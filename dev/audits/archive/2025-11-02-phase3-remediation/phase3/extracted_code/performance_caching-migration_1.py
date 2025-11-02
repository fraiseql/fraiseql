# Extracted from: docs/performance/caching-migration.md
# Block number: 1
from fastapi import FastAPI

from fraiseql.caching import PostgresCache, ResultCache

app = FastAPI()


@app.on_event("startup")
async def startup():
    # Reuse existing database pool
    pool = app.state.db_pool

    # Initialize cache backend (auto-creates UNLOGGED table)
    postgres_cache = PostgresCache(
        connection_pool=pool, table_name="fraiseql_cache", auto_initialize=True
    )

    # Wrap with result cache for statistics
    app.state.result_cache = ResultCache(
        backend=postgres_cache,
        default_ttl=300,  # 5 minutes default
    )
