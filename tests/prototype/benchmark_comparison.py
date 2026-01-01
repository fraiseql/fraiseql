"""
Phase 0 Prototype: Benchmark Comparison

Compares performance between:
1. Python psycopg (baseline)
2. Rust PrototypePool (target)

Metrics:
- Simple query latency (SELECT 1)
- 1000-row query throughput
- Concurrent query performance
- Memory usage

Usage:
    python tests/prototype/benchmark_comparison.py
"""

import asyncio
import time
import sys
from contextlib import asynccontextmanager

# Check if psycopg is available (baseline)
try:
    import psycopg
    from psycopg_pool import AsyncConnectionPool
    HAS_PSYCOPG = True
except ImportError:
    HAS_PSYCOPG = False
    print("⚠️  psycopg not installed - cannot run baseline comparison")
    print("   Install with: pip install psycopg psycopg-pool")

# Check if fraiseql_rs is available (prototype)
try:
    from fraiseql._fraiseql_rs import PrototypePool
    HAS_PROTOTYPE = True
except ImportError:
    HAS_PROTOTYPE = False
    print("⚠️  fraiseql_rs not built - cannot run prototype tests")
    print("   Build with: cd fraiseql_rs && maturin develop")

if not HAS_PSYCOPG and not HAS_PROTOTYPE:
    print("\n❌ Cannot run benchmarks - missing both psycopg and fraiseql_rs")
    sys.exit(1)


# Database configuration
DB_URL = "postgresql://postgres@localhost/postgres"
DB_CONFIG = {
    "database": "postgres",
    "host": "localhost",
    "port": 5432,
    "username": "postgres",
    "password": None,
    "max_connections": 10,
}


class BenchmarkRunner:
    """Helper class to run and report benchmarks"""

    def __init__(self):
        self.results = {}

    @staticmethod
    async def measure(name, coro, iterations=100):
        """Measure execution time of an async function"""
        # Warmup
        await coro()

        # Measure
        start = time.perf_counter()
        for _ in range(iterations):
            await coro()
        end = time.perf_counter()

        avg_ms = ((end - start) / iterations) * 1000
        return avg_ms

    def record(self, category, implementation, metric, value):
        """Record a benchmark result"""
        if category not in self.results:
            self.results[category] = {}
        if implementation not in self.results[category]:
            self.results[category][implementation] = {}
        self.results[category][implementation][metric] = value

    def print_results(self):
        """Print benchmark results with comparison"""
        print("\n" + "=" * 80)
        print("BENCHMARK RESULTS".center(80))
        print("=" * 80)

        for category, implementations in self.results.items():
            print(f"\n{category}")
            print("-" * 80)

            # Check if we have both implementations
            has_baseline = "psycopg" in implementations
            has_prototype = "rust" in implementations

            if has_baseline and has_prototype:
                baseline = implementations["psycopg"]
                prototype = implementations["rust"]

                for metric in baseline.keys():
                    base_val = baseline[metric]
                    proto_val = prototype[metric]
                    speedup = base_val / proto_val if proto_val > 0 else 0

                    print(f"  {metric}:")
                    print(f"    Python (psycopg): {base_val:.3f}ms")
                    print(f"    Rust (prototype): {proto_val:.3f}ms")
                    print(f"    Speedup: {speedup:.2f}x {'✅' if speedup > 1 else '⚠️'}")
            elif has_baseline:
                for metric, value in implementations["psycopg"].items():
                    print(f"  {metric} (psycopg): {value:.3f}ms")
            elif has_prototype:
                for metric, value in implementations["rust"].items():
                    print(f"  {metric} (rust): {value:.3f}ms")

        print("\n" + "=" * 80)


# Baseline: psycopg implementation
@asynccontextmanager
async def psycopg_pool():
    """Create psycopg connection pool"""
    if not HAS_PSYCOPG:
        yield None
        return

    pool = AsyncConnectionPool(
        DB_URL,
        min_size=1,
        max_size=10,
        timeout=30,
    )

    try:
        await pool.wait()
        yield pool
    finally:
        await pool.close()


async def psycopg_simple_query(pool):
    """Execute simple query with psycopg"""
    async with pool.connection() as conn:
        cursor = await conn.execute("SELECT 1")
        await cursor.fetchone()


async def psycopg_1000_rows(pool):
    """Execute 1000-row query with psycopg"""
    async with pool.connection() as conn:
        cursor = await conn.execute(
            "SELECT generate_series(1, 1000) as num"
        )
        rows = await cursor.fetchall()
        return len(rows)


async def psycopg_concurrent_10(pool):
    """Execute 10 concurrent queries with psycopg"""
    async def query():
        async with pool.connection() as conn:
            cursor = await conn.execute("SELECT 1")
            await cursor.fetchone()

    tasks = [query() for _ in range(10)]
    await asyncio.gather(*tasks)


# Prototype: Rust implementation
@asynccontextmanager
async def rust_pool():
    """Create Rust prototype pool"""
    if not HAS_PROTOTYPE:
        yield None
        return

    try:
        pool = PrototypePool(**DB_CONFIG)
        yield pool
    except Exception as e:
        print(f"❌ Cannot create Rust pool: {e}")
        yield None


async def rust_simple_query(pool):
    """Execute simple query with Rust"""
    await pool.execute_query("SELECT 1")


async def rust_1000_rows(pool):
    """Execute 1000-row query with Rust"""
    results = await pool.execute_query(
        "SELECT generate_series(1, 1000) as num"
    )
    return len(results)


async def rust_concurrent_10(pool):
    """Execute 10 concurrent queries with Rust"""
    tasks = [pool.execute_query("SELECT 1") for _ in range(10)]
    await asyncio.gather(*tasks)


# Main benchmark suite
async def run_benchmarks():
    """Run all benchmarks"""
    runner = BenchmarkRunner()

    # Benchmark 1: Simple query latency
    print("Running Benchmark 1: Simple Query Latency (SELECT 1)")
    print("  Testing 100 iterations...")

    if HAS_PSYCOPG:
        async with psycopg_pool() as pool:
            if pool:
                avg_ms = await runner.measure(
                    "psycopg_simple",
                    lambda: psycopg_simple_query(pool),
                    iterations=100,
                )
                runner.record(
                    "1. Simple Query (SELECT 1)",
                    "psycopg",
                    "Average Latency",
                    avg_ms,
                )

    if HAS_PROTOTYPE:
        async with rust_pool() as pool:
            if pool:
                avg_ms = await runner.measure(
                    "rust_simple",
                    lambda: rust_simple_query(pool),
                    iterations=100,
                )
                runner.record(
                    "1. Simple Query (SELECT 1)",
                    "rust",
                    "Average Latency",
                    avg_ms,
                )

    # Benchmark 2: 1000-row query
    print("\nRunning Benchmark 2: 1000-Row Query")
    print("  Testing 50 iterations...")

    if HAS_PSYCOPG:
        async with psycopg_pool() as pool:
            if pool:
                avg_ms = await runner.measure(
                    "psycopg_1000",
                    lambda: psycopg_1000_rows(pool),
                    iterations=50,
                )
                runner.record(
                    "2. 1000-Row Query",
                    "psycopg",
                    "Average Latency",
                    avg_ms,
                )

    if HAS_PROTOTYPE:
        async with rust_pool() as pool:
            if pool:
                avg_ms = await runner.measure(
                    "rust_1000",
                    lambda: rust_1000_rows(pool),
                    iterations=50,
                )
                runner.record(
                    "2. 1000-Row Query",
                    "rust",
                    "Average Latency",
                    avg_ms,
                )

    # Benchmark 3: Concurrent queries
    print("\nRunning Benchmark 3: 10 Concurrent Queries")
    print("  Testing 20 iterations...")

    if HAS_PSYCOPG:
        async with psycopg_pool() as pool:
            if pool:
                avg_ms = await runner.measure(
                    "psycopg_concurrent",
                    lambda: psycopg_concurrent_10(pool),
                    iterations=20,
                )
                runner.record(
                    "3. 10 Concurrent Queries",
                    "psycopg",
                    "Average Latency",
                    avg_ms,
                )

    if HAS_PROTOTYPE:
        async with rust_pool() as pool:
            if pool:
                avg_ms = await runner.measure(
                    "rust_concurrent",
                    lambda: rust_concurrent_10(pool),
                    iterations=20,
                )
                runner.record(
                    "3. 10 Concurrent Queries",
                    "rust",
                    "Average Latency",
                    avg_ms,
                )

    # Print results
    runner.print_results()

    # Summary
    print("\n" + "=" * 80)
    print("SUMMARY".center(80))
    print("=" * 80)

    if HAS_PSYCOPG and HAS_PROTOTYPE:
        print("\n✅ Both implementations tested successfully!")
        print("\nNOTE: These benchmarks measure PyO3 async bridge overhead + database I/O.")
        print("      Real-world performance will vary based on query complexity.")
        print("\nNext steps:")
        print("  1. Review results above")
        print("  2. If speedup < 1x, investigate GIL handling")
        print("  3. If speedup > 2x, proceed to Phase 1 implementation")
    elif HAS_PSYCOPG:
        print("\n⚠️  Only psycopg tested (Rust prototype not available)")
        print("    Build Rust extension to compare: cd fraiseql_rs && maturin develop")
    elif HAS_PROTOTYPE:
        print("\n⚠️  Only Rust prototype tested (psycopg not available)")
        print("    Install psycopg to compare: pip install psycopg psycopg-pool")

    print("\n" + "=" * 80)


if __name__ == "__main__":
    print("Phase 0 Prototype: Benchmark Comparison")
    print("=" * 80)
    print(f"Python (psycopg):  {'✅ Available' if HAS_PSYCOPG else '❌ Not installed'}")
    print(f"Rust (prototype):  {'✅ Available' if HAS_PROTOTYPE else '❌ Not built'}")
    print("=" * 80)

    try:
        asyncio.run(run_benchmarks())
    except KeyboardInterrupt:
        print("\n\n⚠️  Benchmark interrupted by user")
    except Exception as e:
        print(f"\n\n❌ Benchmark failed: {e}")
        import traceback
        traceback.print_exc()
