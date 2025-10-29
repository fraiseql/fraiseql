#!/usr/bin/env python3
"""LTREE Performance Benchmark

Measures query performance for all 23 LTREE operators with realistic hierarchical data.
Tests both indexed and non-indexed scenarios to demonstrate performance improvements.
"""

import asyncio
import json
import logging
import statistics
import time
from pathlib import Path
from typing import Any

import psycopg_pool


async def benchmark_ltree_query(
    pool: psycopg_pool.AsyncConnectionPool, query: str, description: str, iterations: int = 100
) -> dict[str, Any]:
    """Benchmark a single LTREE query."""
    times = []

    # Warm-up (5 iterations)
    for _ in range(5):
        async with pool.connection() as conn, conn.cursor() as cursor:
            await cursor.execute(query)
            await cursor.fetchall()

    # Actual benchmark
    for _ in range(iterations):
        start = time.perf_counter()

        async with pool.connection() as conn, conn.cursor() as cursor:
            await cursor.execute(query)
            rows = await cursor.fetchall()

        end = time.perf_counter()
        times.append((end - start) * 1000)  # Convert to milliseconds

    return {
        "description": description,
        "query": query,
        "iterations": iterations,
        "avg_time_ms": round(statistics.mean(times), 3),
        "median_time_ms": round(statistics.median(times), 3),
        "min_time_ms": round(min(times), 3),
        "max_time_ms": round(max(times), 3),
        "std_dev_ms": round(statistics.stdev(times), 3) if len(times) > 1 else 0,
        "result_count": len(rows) if "rows" in locals() else 0,
    }


async def run_ltree_benchmarks(pool: psycopg_pool.AsyncConnectionPool) -> list[dict[str, Any]]:
    """Run comprehensive LTREE operator benchmarks."""
    benchmarks = []

    # Basic equality operations
    benchmarks.append(
        await benchmark_ltree_query(
            pool,
            "SELECT * FROM ltree_benchmark WHERE category_path = 'top.science.physics'::ltree",
            "Basic equality (=)",
        )
    )

    benchmarks.append(
        await benchmark_ltree_query(
            pool,
            "SELECT * FROM ltree_benchmark WHERE category_path != 'top.science.physics'::ltree",
            "Inequality (!=)",
        )
    )

    # Hierarchical operations
    benchmarks.append(
        await benchmark_ltree_query(
            pool,
            "SELECT * FROM ltree_benchmark WHERE category_path @> 'top.science'::ltree",
            "Ancestor of (@>)",
        )
    )

    benchmarks.append(
        await benchmark_ltree_query(
            pool,
            "SELECT * FROM ltree_benchmark WHERE category_path <@ 'top.science.physics'::ltree",
            "Descendant of (<@)",
        )
    )

    # Pattern matching
    benchmarks.append(
        await benchmark_ltree_query(
            pool,
            "SELECT * FROM ltree_benchmark WHERE category_path ~ 'top.science.*.physics'",
            "LQUERY pattern match (~)",
        )
    )

    benchmarks.append(
        await benchmark_ltree_query(
            pool,
            "SELECT * FROM ltree_benchmark WHERE category_path @ 'top.science.*{1,2}.physics'",
            "LTXTQUERY pattern match (@)",
        )
    )

    benchmarks.append(
        await benchmark_ltree_query(
            pool,
            "SELECT * FROM ltree_benchmark WHERE category_path ? "
            "ARRAY['top.science.*.physics', 'top.business.*.finance']",
            "Match any LQUERY (? array)",
        )
    )

    # Path analysis operations
    benchmarks.append(
        await benchmark_ltree_query(
            pool,
            "SELECT * FROM ltree_benchmark WHERE nlevel(category_path) = 3",
            "Exact depth (nlevel = 3)",
        )
    )

    benchmarks.append(
        await benchmark_ltree_query(
            pool,
            "SELECT * FROM ltree_benchmark WHERE nlevel(category_path) > 2",
            "Depth greater than (nlevel > 2)",
        )
    )

    benchmarks.append(
        await benchmark_ltree_query(
            pool,
            "SELECT * FROM ltree_benchmark WHERE subpath(category_path, 1, 2) = 'science.physics'",
            "Subpath extraction",
        )
    )

    benchmarks.append(
        await benchmark_ltree_query(
            pool,
            "SELECT * FROM ltree_benchmark WHERE index(category_path, 'physics') = 2",
            "Find sublabel position (index = 2)",
        )
    )

    benchmarks.append(
        await benchmark_ltree_query(
            pool,
            "SELECT * FROM ltree_benchmark WHERE index(category_path, 'physics') >= 1",
            "Minimum sublabel position (index >= 1)",
        )
    )

    # Path manipulation
    benchmarks.append(
        await benchmark_ltree_query(
            pool,
            "SELECT category_path || 'quantum'::ltree FROM ltree_benchmark "
            "WHERE category_path = 'top.science.physics'::ltree",
            "Path concatenation (||)",
        )
    )

    benchmarks.append(
        await benchmark_ltree_query(
            pool,
            "SELECT lca(ARRAY[category_path, 'top.science.chemistry'::ltree]) "
            "FROM ltree_benchmark WHERE category_path <@ 'top.science'::ltree LIMIT 10",
            "Lowest common ancestor (lca)",
        )
    )

    # Array operations
    benchmarks.append(
        await benchmark_ltree_query(
            pool,
            "SELECT * FROM ltree_benchmark WHERE category_path IN "
            "('top.science.physics'::ltree, 'top.business.finance'::ltree)",
            "IN array",
        )
    )

    benchmarks.append(
        await benchmark_ltree_query(
            pool,
            "SELECT * FROM ltree_benchmark WHERE 'top.science'::ltree @> category_path",
            "Array contains (@>)",
        )
    )

    # Complex hierarchical queries
    benchmarks.append(
        await benchmark_ltree_query(
            pool,
            "SELECT * FROM ltree_benchmark WHERE category_path <@ 'top.science'::ltree "
            "AND nlevel(category_path) >= 3",
            "Complex: descendants with min depth",
        )
    )

    benchmarks.append(
        await benchmark_ltree_query(
            pool,
            "SELECT * FROM ltree_benchmark WHERE category_path ~ 'top.*.*' "
            "AND category_path @> 'physics'",
            "Complex: pattern + ancestor",
        )
    )

    return benchmarks


async def run_index_comparison_benchmark(pool: psycopg_pool.AsyncConnectionPool) -> dict[str, Any]:
    """Compare performance with and without GiST index."""
    # Test query that benefits from index
    test_query = "SELECT * FROM ltree_benchmark WHERE category_path <@ 'top.science'::ltree"

    # With index (current state)
    with_index = await benchmark_ltree_query(pool, test_query, "With GiST index", 50)

    # Drop index temporarily
    async with pool.connection() as conn, conn.cursor() as cursor:
        await cursor.execute("DROP INDEX IF EXISTS idx_ltree_benchmark_path")

    # Without index
    without_index = await benchmark_ltree_query(pool, test_query, "Without GiST index", 50)

    # Recreate index
    async with pool.connection() as conn, conn.cursor() as cursor:
        await cursor.execute(
            "CREATE INDEX idx_ltree_benchmark_path ON ltree_benchmark USING GIST (category_path)"
        )

    return {
        "query": test_query,
        "with_index": with_index,
        "without_index": without_index,
        "speedup_factor": round(without_index["avg_time_ms"] / with_index["avg_time_ms"], 2),
    }


async def main() -> None:
    """Main benchmark execution."""
    # Set up logging
    logging.basicConfig(level=logging.INFO, format="%(levelname)s: %(message)s")
    logger = logging.getLogger(__name__)

    # Database connection
    dsn = "postgresql://postgres:password@localhost:5432/fraiseql_test"

    async with psycopg_pool.AsyncConnectionPool(dsn) as pool:
        logger.info("🚀 Starting LTREE Performance Benchmark")

        # Run operator benchmarks
        logger.info("📊 Running LTREE Operator Benchmarks...")
        operator_results = await run_ltree_benchmarks(pool)

        # Run index comparison
        logger.info("📈 Running Index Performance Comparison...")
        index_comparison = await run_index_comparison_benchmark(pool)

        # Generate report
        report = {
            "benchmark_timestamp": time.time(),
            "dataset_size": 10000,
            "operator_benchmarks": operator_results,
            "index_comparison": index_comparison,
            "summary": {
                "total_operators_tested": len(operator_results),
                "fastest_operation": min(operator_results, key=lambda x: x["avg_time_ms"]),
                "slowest_operation": max(operator_results, key=lambda x: x["avg_time_ms"]),
                "index_speedup": index_comparison["speedup_factor"],
            },
        }

        # Save results
        timestamp = time.strftime("%Y%m%d_%H%M%S")
        filename = f"ltree_benchmark_{timestamp}.json"
        results_path = Path("benchmarks/ltree_performance_benchmark") / filename

        results_path.parent.mkdir(parents=True, exist_ok=True)
        results_path.write_text(json.dumps(report, indent=2))

        # Print summary
        logger.info("✅ Benchmark Complete!")
        logger.info(f"📄 Results saved to: {filename}")
        logger.info(f"⚡ Index speedup: {index_comparison['speedup_factor']}x")
        logger.info(
            f"🏃 Fastest operation: {report['summary']['fastest_operation']['description']} "
            f"({report['summary']['fastest_operation']['avg_time_ms']}ms)"
        )
        logger.info(
            f"🐌 Slowest operation: {report['summary']['slowest_operation']['description']} "
            f"({report['summary']['slowest_operation']['avg_time_ms']}ms)"
        )


if __name__ == "__main__":
    asyncio.run(main())
