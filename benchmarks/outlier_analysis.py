"""Analyze outliers and variance in Rust vs Python transformation."""

import asyncio
import json
import statistics
import time
from typing import Any, Callable

import psycopg_pool


async def detailed_benchmark(
    pool: psycopg_pool.AsyncConnectionPool,
    query: str,
    transform_func: Callable[[str], str],
    name: str,
    iterations: int = 100,
) -> dict[str, Any]:
    """Run detailed benchmark with outlier analysis."""
    times = []

    # Warm-up
    for _ in range(10):
        async with pool.connection() as conn:
            async with conn.cursor() as cursor:
                await cursor.execute(query)
                rows = await cursor.fetchall()
                for row in rows:
                    if row and row[0]:
                        transform_func(row[0])

    # Benchmark
    for i in range(iterations):
        start = time.perf_counter()
        async with pool.connection() as conn:
            async with conn.cursor() as cursor:
                await cursor.execute(query)
                rows = await cursor.fetchall()
                for row in rows:
                    if row and row[0]:
                        transform_func(row[0])
        elapsed = time.perf_counter() - start
        times.append(elapsed * 1000)

    # Calculate statistics
    times_sorted = sorted(times)
    mean = statistics.mean(times)
    median = statistics.median(times)
    stdev = statistics.stdev(times)

    # Percentiles
    p95 = times_sorted[int(len(times_sorted) * 0.95)]
    p99 = times_sorted[int(len(times_sorted) * 0.99)]

    # Outliers (values > mean + 2*stdev)
    outlier_threshold = mean + 2 * stdev
    outliers = [t for t in times if t > outlier_threshold]
    outlier_pct = (len(outliers) / len(times)) * 100

    print(f"\n{name}:")
    print(f"  Mean:     {mean:.3f} ms")
    print(f"  Median:   {median:.3f} ms")
    print(f"  StdDev:   {stdev:.3f} ms")
    print(f"  Min:      {min(times):.3f} ms")
    print(f"  Max:      {max(times):.3f} ms")
    print(f"  P95:      {p95:.3f} ms")
    print(f"  P99:      {p99:.3f} ms")
    print(f"  Outliers: {len(outliers)} ({outlier_pct:.1f}%)")
    if outliers:
        print(f"  Outlier values: {[f'{x:.3f}' for x in sorted(outliers)]}")

    return {
        "name": name,
        "mean": mean,
        "median": median,
        "stdev": stdev,
        "min": min(times),
        "max": max(times),
        "p95": p95,
        "p99": p99,
        "outliers": len(outliers),
    }


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
    return json.dumps(transform_dict(data))


async def setup_test_data(pool: psycopg_pool.AsyncConnectionPool) -> None:
    """Setup test table."""
    async with pool.connection() as conn:
        async with conn.cursor() as cursor:
            await cursor.execute("DROP TABLE IF EXISTS outlier_test")
            await cursor.execute("""
                CREATE TABLE outlier_test (
                    id SERIAL PRIMARY KEY,
                    data JSONB NOT NULL
                )
            """)

            # Insert 20 test rows
            for i in range(20):
                data = {
                    "user_id": i,
                    "user_name": f"User {i}",
                    "user_posts": [{"post_id": j, "post_title": f"Post {j}"} for j in range(10)],
                }
                await cursor.execute(
                    "INSERT INTO outlier_test (data) VALUES (%s)", (json.dumps(data),)
                )


async def main() -> None:
    """Run outlier analysis."""
    import os

    db_url = os.getenv("DATABASE_URL", "postgresql://localhost/fraiseql_test")

    pool = psycopg_pool.AsyncConnectionPool(db_url, min_size=1, max_size=5, open=False)
    await pool.open()

    try:
        print("=" * 80)
        print("OUTLIER ANALYSIS: 100 iterations to see variance")
        print("=" * 80)

        await setup_test_data(pool)

        query = "SELECT data::text FROM outlier_test"

        # Import Rust
        try:
            from fraiseql import fraiseql_rs

            rust_available = True
        except ImportError:
            rust_available = False
            print("⚠️  Rust not available")
            return

        # Run benchmarks
        result_python = await detailed_benchmark(
            pool, query, transform_python, "Python", iterations=100
        )

        result_rust = await detailed_benchmark(
            pool, query, fraiseql_rs.transform_json, "Rust", iterations=100
        )

        # Compare
        print("\n" + "=" * 80)
        print("COMPARISON")
        print("=" * 80)
        print(f"\nMedian speedup: {result_python['median'] / result_rust['median']:.2f}x")
        print(f"Mean speedup:   {result_python['mean'] / result_rust['mean']:.2f}x")
        print(
            f"\nPython variance: {result_python['stdev']:.3f}ms (outliers: {result_python['outliers']})"
        )
        print(
            f"Rust variance:   {result_rust['stdev']:.3f}ms (outliers: {result_rust['outliers']})"
        )

        # Conclusion
        print("\n" + "=" * 80)
        print("INTERPRETATION")
        print("=" * 80)

        if result_rust["outliers"] > result_python["outliers"]:
            print("\n⚠️  Rust has MORE outliers than Python")
            print("   This suggests PyO3 FFI boundary or Rust allocator spikes")
            print("   Use MEDIAN for more reliable comparison")
        else:
            print("\n✅ Rust has similar or fewer outliers than Python")
            print("   Performance is consistent")

        if result_rust["median"] < result_python["median"]:
            print(
                f"\n✅ Rust median ({result_rust['median']:.2f}ms) < Python median ({result_python['median']:.2f}ms)"
            )
            print(
                f"   Typical case: Rust is {result_python['median'] / result_rust['median']:.2f}x faster"
            )

        # Cleanup
        async with pool.connection() as conn:
            async with conn.cursor() as cursor:
                await cursor.execute("DROP TABLE outlier_test")

    finally:
        await pool.close()


if __name__ == "__main__":
    asyncio.run(main())
