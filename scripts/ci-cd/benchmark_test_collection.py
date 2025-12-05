#!/usr/bin/env python3
"""
Benchmark Test Collection Performance for FraiseQL CI/CD Optimization.

This script measures test collection times for different test categories
to validate the impact of the CI/CD optimization strategy.
"""

import argparse
import json
import os
import re
import subprocess
import sys
import time


def collect_tests(marker_filter: str, skip_rust: bool = False) -> dict:
    """Run pytest --collect-only and measure timing.

    Args:
        marker_filter: pytest marker expression (e.g., "not database")
        skip_rust: If True, set FRAISEQL_SKIP_RUST=1

    Returns:
        Dict with test_count, collection_time_seconds, marker_filter
    """
    env = os.environ.copy()
    if skip_rust:
        env["FRAISEQL_SKIP_RUST"] = "1"

    cmd = ["uv", "run", "pytest", "--collect-only", "-q", "tests/"]
    if marker_filter:
        cmd.extend(["-m", marker_filter])

    start = time.perf_counter()
    result = subprocess.run(cmd, capture_output=True, text=True, env=env, timeout=120)
    collection_time = time.perf_counter() - start

    # Parse test count from output like "4352/4655 tests collected" or "4655 tests collected"
    test_count = 0
    for line in result.stdout.splitlines():
        match = re.search(r"(\d+)(?:/\d+)?\s+tests?\s+collected", line)
        if match:
            test_count = int(match.group(1))
            break

    return {
        "marker_filter": marker_filter or "(all)",
        "test_count": test_count,
        "collection_time_seconds": round(collection_time, 2),
        "skip_rust": skip_rust,
    }


def main() -> int:
    """Run benchmark and output results."""
    parser = argparse.ArgumentParser(description="Benchmark test collection performance")
    parser.add_argument("--json", action="store_true", help="Output JSON format")
    args = parser.parse_args()

    print("Benchmarking test collection performance...\n")

    # Unit tests (skip Rust, skip isolation markers)
    unit_filter = (
        "not database and not integration and not e2e "
        "and not forked and not slow and not enterprise"
    )
    unit_results = collect_tests(unit_filter, skip_rust=True)

    # Integration tests (need Rust and isolation markers)
    integration_filter = (
        "database or integration or e2e or forked or slow or enterprise"
    )
    integration_results = collect_tests(integration_filter, skip_rust=False)

    # All tests
    all_results = collect_tests("", skip_rust=False)

    results = {
        "unit_tests": unit_results,
        "integration_tests": integration_results,
        "all_tests": all_results,
    }

    if args.json:
        print(json.dumps(results, indent=2))
    else:
        print("=" * 60)
        print("Test Collection Performance Summary")
        print("=" * 60)
        print(
            f"\nUnit tests (SKIP_RUST=1):\n"
            f"  Tests: {unit_results['test_count']}\n"
            f"  Collection time: {unit_results['collection_time_seconds']}s"
        )
        print(
            f"\nIntegration tests:\n"
            f"  Tests: {integration_results['test_count']}\n"
            f"  Collection time: {integration_results['collection_time_seconds']}s"
        )
        print(
            f"\nAll tests:\n"
            f"  Tests: {all_results['test_count']}\n"
            f"  Collection time: {all_results['collection_time_seconds']}s"
        )

        # Calculate estimated speedup
        total_tests = all_results["test_count"]
        unit_tests = unit_results["test_count"]
        if total_tests > 0 and unit_tests > 0:
            unit_ratio = unit_tests / total_tests
            print(f"\n{'=' * 60}")
            print(f"Unit tests represent {unit_ratio:.1%} of all tests")
            print(
                "With optimized CI, unit tests run first for fast feedback,\n"
                "integration tests only run if unit tests pass."
            )

    return 0


if __name__ == "__main__":
    sys.exit(main())
