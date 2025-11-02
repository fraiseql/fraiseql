# Extracted from: docs/tutorials/production-deployment.md
# Block number: 1
# src/app.py
import os

from psycopg_pool import AsyncConnectionPool

from fraiseql import FraiseQL, FraiseQLConfig
from fraiseql.monitoring import setup_prometheus, setup_sentry

# Load environment
ENV = os.getenv("ENV", "development")
DATABASE_URL = os.getenv("DATABASE_URL")

# Configuration
config = FraiseQLConfig(
    database_url=DATABASE_URL,
    # Performance
    rust_enabled=os.getenv("RUST_ENABLED", "true").lower() == "true",
    apq_enabled=os.getenv("APQ_ENABLED", "true").lower() == "true",
    apq_storage_backend=os.getenv("APQ_STORAGE_BACKEND", "postgresql"),
    enable_turbo_router=True,
    json_passthrough_enabled=True,
    # Security
    enable_playground=(ENV != "production"),
    complexity_enabled=True,
    complexity_max_score=1000,
    query_depth_limit=10,
    # Monitoring
    enable_logging=True,
    log_level=os.getenv("LOG_LEVEL", "info"),
)

# Initialize app
app = FraiseQL(config=config)

# Connection pool
pool = AsyncConnectionPool(conninfo=DATABASE_URL, min_size=5, max_size=20, timeout=5.0)

# Monitoring setup
if ENV == "production":
    setup_sentry(dsn=os.getenv("SENTRY_DSN"), environment=ENV, traces_sample_rate=0.1)

    setup_prometheus(app)


# Health check endpoint
@app.get("/health")
async def health_check():
    """Health check for load balancer."""
    async with pool.connection() as conn:
        await conn.execute("SELECT 1")
    return {"status": "healthy"}


# Graceful shutdown
@app.on_event("shutdown")
async def shutdown():
    await pool.close()
