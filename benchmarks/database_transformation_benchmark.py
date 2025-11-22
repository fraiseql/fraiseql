"""End-to-end benchmark: Database query + transformation.

Measures actual end-to-end performance including:
1. PostgreSQL query execution
2. Data transfer (PostgreSQL -> Python)
3. Transformation (Rust vs Python)

This gives us the real-world impact.
"""

import asyncio
import json
import statistics
import time

import psycopg_pool


async def benchmark_query_with_transformation(
    pool: psycopg_pool.AsyncConnectionPool,
    query: str,
    transform_func: callable,
    iterations: int = 50,
) -> dict[str, float]:
    """Benchmark query execution + transformation."""
    times = []

    # Warm-up
    for _ in range(5):
        async with pool.connection() as conn:
            async with conn.cursor() as cursor:
                await cursor.execute(query)
                rows = await cursor.fetchall()
                for row in rows:
                    if row and row[0]:
                        transform_func(row[0])

    # Actual benchmark
    for _ in range(iterations):
        start = time.perf_counter()

        async with pool.connection() as conn:
            async with conn.cursor() as cursor:
                await cursor.execute(query)
                rows = await cursor.fetchall()

                # Transform each result
                for row in rows:
                    if row and row[0]:
                        transform_func(row[0])

        elapsed = time.perf_counter() - start
        times.append(elapsed * 1000)  # ms

    return {
        "min_ms": min(times),
        "max_ms": max(times),
        "mean_ms": statistics.mean(times),
        "median_ms": statistics.median(times),
        "stdev_ms": statistics.stdev(times) if len(times) > 1 else 0,
    }


async def setup_test_table(pool: psycopg_pool.AsyncConnectionPool) -> None:
    """Create test table with sample data."""
    async with pool.connection() as conn:
        async with conn.cursor() as cursor:
            # Drop and recreate test table
            await cursor.execute("""
                DROP TABLE IF EXISTS benchmark_test_users CASCADE
            """)

            await cursor.execute("""
                CREATE TABLE benchmark_test_users (
                    id SERIAL PRIMARY KEY,
                    data JSONB NOT NULL
                )
            """)

            # Insert test data with various complexities
            test_cases = [
                # Simple
                {
                    "user_id": i,
                    "user_name": f"User {i}",
                    "email_address": f"user{i}@example.com",
                    "created_at": "2025-10-13T10:00:00Z",
                    "is_active": True,
                }
                for i in range(10)
            ] + [
                # Nested (User with posts)
                {
                    "user_id": i,
                    "user_name": f"User {i}",
                    "user_posts": [
                        {
                            "post_id": j,
                            "post_title": f"Post {j}",
                            "post_content": f"Content for post {j}",
                        }
                        for j in range(10)
                    ],
                }
                for i in range(10, 20)
            ]

            for data in test_cases:
                await cursor.execute(
                    "INSERT INTO benchmark_test_users (data) VALUES (%s)", (json.dumps(data),)
                )


def transform_python(json_str: str) -> str:
    """Pure Python transformation."""

    def to_camel(s: str) -> str:
        parts = s.split("_")
        return parts[0] + "".join(p.title() for p in parts[1:])

    def transform_dict(d: dict) -> dict:
        result = {}
        for k, v in d.items():
            ck = to_camel(k)
            if isinstance(v, dict):
                result[ck] = transform_dict(v)
            elif isinstance(v, list):
                result[ck] = [transform_dict(i) if isinstance(i, dict) else i for i in v]
            else:
                result[ck] = v
        return result

    data = json.loads(json_str)
    transformed = transform_dict(data)
    return json.dumps(transformed)


async def run_database_benchmark() -> None:
    """Run end-to-end database benchmarks."""
    # Check database availability
    try:
        import os

        db_url = os.getenv("DATABASE_URL", "postgresql://localhost/fraiseql_test")

        pool = psycopg_pool.AsyncConnectionPool(db_url, min_size=1, max_size=5, open=False)
        await pool.open()
    except Exception as e:
        print(f"⚠️  Database not available: {e}")
        print("Skipping database benchmarks")
        return

    print("=" * 80)
    print("END-TO-END BENCHMARK: Query + Transformation")
    print("=" * 80)
    print()

    try:
        # Setup test data
        print("Setting up test data...")
        await setup_test_table(pool)
        print("✅ Test data ready\n")

        # Import Rust transformer
        try:
            from fraiseql import fraiseql_rs

            rust_available = True
        except ImportError:
            print("⚠️  fraiseql_rs not available")
            rust_available = False

        # Test queries
        queries = [
            ("Simple query (10 rows)", "SELECT data::text FROM benchmark_test_users LIMIT 10"),
            (
                "Nested query (10 rows)",
                "SELECT data::text FROM benchmark_test_users OFFSET 10 LIMIT 10",
            ),
            ("All rows (20 rows)", "SELECT data::text FROM benchmark_test_users"),
        ]

        for name, query in queries:
            print(f"\n📊 {name}")
            print("-" * 80)

            # Benchmark with Python transformation
            result_python = await benchmark_query_with_transformation(
                pool, query, transform_python, iterations=30
            )

            print("\nQuery + Python transformation:")
            print(f"  Mean:   {result_python['mean_ms']:.2f} ms")
            print(f"  Median: {result_python['median_ms']:.2f} ms")
            print(f"  Min:    {result_python['min_ms']:.2f} ms")
            print(f"  Max:    {result_python['max_ms']:.2f} ms")

            # Benchmark with Rust transformation
            if rust_available:
                result_rust = await benchmark_query_with_transformation(
                    pool, query, fraiseql_rs.transform_json, iterations=30
                )

                print("\nQuery + Rust transformation:")
                print(f"  Mean:   {result_rust['mean_ms']:.2f} ms")
                print(f"  Median: {result_rust['median_ms']:.2f} ms")
                print(f"  Min:    {result_rust['min_ms']:.2f} ms")
                print(f"  Max:    {result_rust['max_ms']:.2f} ms")

                # Calculate impact
                speedup = result_python["mean_ms"] / result_rust["mean_ms"]
                time_saved = result_python["mean_ms"] - result_rust["mean_ms"]

                print("\n⚡ Impact:")
                print(f"   Speedup: {speedup:.2f}x")
                print(
                    f"   Time saved: {time_saved:.2f} ms ({time_saved / result_python['mean_ms'] * 100:.1f}%)"
                )

            print("-" * 80)

        # Cleanup
        async with pool.connection() as conn:
            async with conn.cursor() as cursor:
                await cursor.execute("DROP TABLE benchmark_test_users")

    finally:
        await pool.close()

    print("\n" + "=" * 80)
    print("BENCHMARK COMPLETE")
    print("=" * 80)


if __name__ == "__main__":
    asyncio.run(run_database_benchmark())
