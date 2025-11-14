#!/usr/bin/env python3
"""
CI Performance Benchmark for FraiseQL

Lightweight performance regression testing for CI/CD pipelines.
Measures key performance metrics and compares against baselines.
"""

import asyncio
import json
import os
import statistics
import time
from pathlib import Path
from typing import Dict, Any, List
import psycopg


async def benchmark_query(
    conn: psycopg.AsyncConnection, query: str, description: str, iterations: int = 10
) -> Dict[str, Any]:
    """Benchmark a single query."""
    times = []

    # Warm-up (3 iterations)
    for _ in range(3):
        async with conn.cursor() as cursor:
            await cursor.execute(query)
            await cursor.fetchall()

    # Actual benchmark
    for _ in range(iterations):
        start = time.perf_counter()

        async with conn.cursor() as cursor:
            await cursor.execute(query)
            rows = await cursor.fetchall()

        end = time.perf_counter()
        times.append((end - start) * 1000)  # Convert to milliseconds

    return {
        "description": description,
        "query": query,
        "iterations": iterations,
        "row_count": len(rows) if "rows" in locals() else 0,
        "avg_time_ms": round(statistics.mean(times), 3),
        "median_time_ms": round(statistics.median(times), 3),
        "min_time_ms": round(min(times), 3),
        "max_time_ms": round(max(times), 3),
        "std_dev_ms": round(statistics.stdev(times), 3) if len(times) > 1 else 0,
        "p95_time_ms": round(sorted(times)[int(len(times) * 0.95)], 3),
    }


async def run_basic_performance_tests(database_url: str) -> Dict[str, Any]:
    """Run basic performance tests."""
    results = {
        "timestamp": time.time(),
        "database_url": database_url.replace(os.environ.get("DB_PASSWORD", ""), "***"),
        "tests": [],
    }

    async with await psycopg.AsyncConnection.connect(database_url) as conn:
        # Test 1: Simple SELECT
        result = await benchmark_query(
            conn, "SELECT 1 as test_value", "Simple SELECT query", iterations=50
        )
        results["tests"].append(result)

        # Test 2: Count users (if table exists)
        try:
            result = await benchmark_query(
                conn, "SELECT COUNT(*) FROM users", "Count users table", iterations=20
            )
            results["tests"].append(result)
        except psycopg.Error:
            results["tests"].append(
                {"description": "Count users table", "error": "users table not found"}
            )

        # Test 3: JSONB query (if applicable)
        try:
            result = await benchmark_query(
                conn,
                "SELECT COUNT(*) FROM information_schema.tables WHERE table_schema = 'public'",
                "Information schema query",
                iterations=10,
            )
            results["tests"].append(result)
        except psycopg.Error as e:
            results["tests"].append({"description": "Information schema query", "error": str(e)})

        # Test 4: Connection pool test (simulate concurrent connections)
        concurrent_times = []

        async def single_connection_test():
            start = time.perf_counter()
            async with await psycopg.AsyncConnection.connect(database_url) as test_conn:
                async with test_conn.cursor() as cursor:
                    await cursor.execute("SELECT pg_sleep(0.01)")  # 10ms sleep
                    await cursor.fetchall()
            end = time.perf_counter()
            concurrent_times.append((end - start) * 1000)

        # Run 5 concurrent connections
        await asyncio.gather(*[single_connection_test() for _ in range(5)])

        results["tests"].append(
            {
                "description": "Concurrent connections (5)",
                "iterations": 5,
                "avg_time_ms": round(statistics.mean(concurrent_times), 3),
                "median_time_ms": round(statistics.median(concurrent_times), 3),
                "min_time_ms": round(min(concurrent_times), 3),
                "max_time_ms": round(max(concurrent_times), 3),
            }
        )

    return results


def save_results(results: Dict[str, Any], output_file: str):
    """Save benchmark results to JSON file."""
    with open(output_file, "w") as f:
        json.dump(results, f, indent=2)


def load_baseline(baseline_file: str) -> Dict[str, Any]:
    """Load baseline results for comparison."""
    if os.path.exists(baseline_file):
        with open(baseline_file, "r") as f:
            return json.load(f)
    return {}


def compare_with_baseline(current: Dict[str, Any], baseline: Dict[str, Any]) -> Dict[str, Any]:
    """Compare current results with baseline."""
    comparison = {
        "comparison_timestamp": time.time(),
        "regressions": [],
        "improvements": [],
        "summary": {},
    }

    if not baseline.get("tests"):
        comparison["summary"]["message"] = "No baseline available for comparison"
        return comparison

    # Compare each test
    for current_test in current["tests"]:
        if "error" in current_test:
            continue

        baseline_test = None
        for b_test in baseline["tests"]:
            if b_test.get("description") == current_test["description"]:
                baseline_test = b_test
                break

        if not baseline_test or "error" in baseline_test:
            continue

        desc = current_test["description"]
        current_avg = current_test["avg_time_ms"]
        baseline_avg = baseline_test["avg_time_ms"]

        # Check for regression (10% slower)
        if current_avg > baseline_avg * 1.10:
            regression_pct = ((current_avg - baseline_avg) / baseline_avg) * 100
            comparison["regressions"].append(
                {
                    "test": desc,
                    "regression_percent": round(regression_pct, 1),
                    "current_ms": current_avg,
                    "baseline_ms": baseline_avg,
                }
            )

        # Check for improvement (10% faster)
        elif current_avg < baseline_avg * 0.90:
            improvement_pct = ((baseline_avg - current_avg) / baseline_avg) * 100
            comparison["improvements"].append(
                {
                    "test": desc,
                    "improvement_percent": round(improvement_pct, 1),
                    "current_ms": current_avg,
                    "baseline_ms": baseline_avg,
                }
            )

    # Summary
    total_regressions = len(comparison["regressions"])
    total_improvements = len(comparison["improvements"])

    if total_regressions > 0:
        comparison["summary"]["status"] = "REGRESSION"
        comparison["summary"]["message"] = f"Found {total_regressions} performance regression(s)"
    elif total_improvements > 0:
        comparison["summary"]["status"] = "IMPROVEMENT"
        comparison["summary"]["message"] = f"Found {total_improvements} performance improvement(s)"
    else:
        comparison["summary"]["status"] = "STABLE"
        comparison["summary"]["message"] = "Performance is stable"

    return comparison


async def main():
    """Main entry point."""
    # Get database URL from environment
    database_url = os.environ.get("DATABASE_URL")
    if not database_url:
        print("❌ DATABASE_URL environment variable not set")
        return 1

    # Output file
    output_file = os.environ.get("BENCHMARK_OUTPUT", "benchmark-results.json")
    baseline_file = os.environ.get("BENCHMARK_BASELINE", "performance-baseline.json")

    print("🚀 Running CI Performance Benchmarks...")
    print(f"Database: {database_url.replace(os.environ.get('DB_PASSWORD', ''), '***')}")
    print(f"Output: {output_file}")
    print(f"Baseline: {baseline_file}")
    print()

    try:
        # Run benchmarks
        results = await run_basic_performance_tests(database_url)

        # Save results
        save_results(results, output_file)
        print(f"✅ Benchmark results saved to {output_file}")

        # Compare with baseline
        baseline = load_baseline(baseline_file)
        if baseline:
            comparison = compare_with_baseline(results, baseline)
            print(f"📊 Comparison: {comparison['summary']['message']}")

            # Print regressions
            if comparison["regressions"]:
                print("\n🚨 Performance Regressions:")
                for reg in comparison["regressions"]:
                    print(f"  • {reg['test']}: {reg['regression_percent']}% slower")

            # Print improvements
            if comparison["improvements"]:
                print("\n✅ Performance Improvements:")
                for imp in comparison["improvements"]:
                    print(f"  • {imp['test']}: {imp['improvement_percent']}% faster")

            # Exit with error if regressions found
            if comparison["regressions"]:
                print("\n❌ Performance regression detected!")
                return 1
        else:
            print("ℹ️  No baseline found - establishing new baseline")

        print("✅ Performance benchmarks completed successfully")
        return 0

    except Exception as e:
        print(f"❌ Benchmark failed: {e}")
        return 1


if __name__ == "__main__":
    exit_code = asyncio.run(main())
    exit(exit_code)
