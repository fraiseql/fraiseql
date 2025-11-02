# Extracted from: docs/tutorials/blog-api.md
# Block number: 4
import os

from psycopg_pool import AsyncConnectionPool

from fraiseql import FraiseQL

# Initialize app
app = FraiseQL(
    database_url=os.getenv("DATABASE_URL", "postgresql://localhost/blog"),
    types=[User, Post, Comment],
    enable_playground=True,
)

# Connection pool
pool = AsyncConnectionPool(conninfo=app.config.database_url, min_size=5, max_size=20)


# Context setup
@app.context
async def get_context(request):
    async with pool.connection() as conn:
        repo = PsycopgRepository(pool=pool)
        return {
            "repo": repo,
            "tenant_id": request.headers.get("X-Tenant-ID"),
            "user_id": request.headers.get("X-User-ID"),  # From auth middleware
        }


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=8000)
