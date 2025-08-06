"""Manual benchmark test for FraiseQL without GraphQL."""

import os
import time
from statistics import mean, quantiles

import asyncpg
from fastapi import FastAPI

app = FastAPI()

# Database configuration
DATABASE_URL = os.environ.get(
    "DATABASE_URL", "postgresql://benchmark:benchmark@postgres/benchmark_db"
)


@app.get("/health")
async def health():
    return {"status": "healthy"}


@app.get("/benchmark/users")
async def benchmark_users(limit: int = 100):
    """Benchmark user queries."""
    conn = await asyncpg.connect(DATABASE_URL)

    try:
        # Warm up
        await conn.fetch("SELECT data FROM v_users LIMIT $1", limit)

        # Run benchmark
        times = []
        for _ in range(10):
            start = time.time()
            await conn.fetch("SELECT data FROM v_users LIMIT $1", limit)
            times.append((time.time() - start) * 1000)  # Convert to ms

        return {
            "query": "users",
            "limit": limit,
            "mean_ms": mean(times),
            "p95_ms": quantiles(times, n=20)[18] if len(times) >= 20 else max(times),
            "min_ms": min(times),
            "max_ms": max(times),
        }
    finally:
        await conn.close()


@app.get("/benchmark/products")
async def benchmark_products(limit: int = 100):
    """Benchmark product queries."""
    conn = await asyncpg.connect(DATABASE_URL)

    try:
        # Warm up
        await conn.fetch("SELECT data FROM v_products LIMIT $1", limit)

        # Run benchmark
        times = []
        for _ in range(10):
            start = time.time()
            await conn.fetch("SELECT data FROM v_products LIMIT $1", limit)
            times.append((time.time() - start) * 1000)  # Convert to ms

        return {
            "query": "products",
            "limit": limit,
            "mean_ms": mean(times),
            "p95_ms": quantiles(times, n=20)[18] if len(times) >= 20 else max(times),
            "min_ms": min(times),
            "max_ms": max(times),
        }
    finally:
        await conn.close()


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=8000)  # noqa: S104
