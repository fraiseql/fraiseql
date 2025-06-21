#!/usr/bin/env python3
"""
FraiseQL vs Strawberry Performance Benchmark Runner

Uses Unix socket connections for accurate framework performance measurement
"""

import json
import os
import statistics
import subprocess
import time
from datetime import datetime, timezone
from pathlib import Path

# Configuration from environment
ITERATIONS = int(os.environ.get("BENCHMARK_ITERATIONS", 30))
WARMUP = int(os.environ.get("BENCHMARK_WARMUP", 10))

# Test queries - covering different complexity levels
QUERIES = {
    "simple_users": {
        "query": "{ users(limit: 10) { id username email } }",
        "description": "Simple user list query",
    },
    "simple_products": {
        "query": "{ products(limit: 10) { id name price } }",
        "description": "Simple product list query",
    },
    "users_with_metadata": {
        "query": "{ users(limit: 20) { id username email fullName createdAt } }",
        "description": "Users with computed fields",
    },
    "products_with_details": {
        "query": "{ products(limit: 20) { id name price stockQuantity categoryId } }",
        "description": "Products with additional fields",
    },
    "filtered_users": {
        "query": '{ users(where: {email: {endsWith: "@gmail.com"}}, limit: 10) { id username email } }',
        "description": "Filtered user query",
    },
    "sorted_products": {
        "query": "{ products(orderBy: {price: DESC}, limit: 10) { id name price } }",
        "description": "Sorted product query",
    },
}

SERVICES = {
    "FraiseQL": {
        "url": "http://localhost:8001/graphql",
        "color": "\033[0;36m",  # Cyan
    },
    "Strawberry": {
        "url": "http://localhost:8002/graphql",
        "color": "\033[0;35m",  # Magenta
    },
}

# Colors
GREEN = "\033[0;32m"
YELLOW = "\033[1;33m"
RED = "\033[0;31m"
BLUE = "\033[0;34m"
NC = "\033[0m"  # No Color


def measure_request_time(url: str, query: str) -> float:
    """Measure response time using curl with time measurement."""
    cmd = [
        "curl",
        "-s",
        "-w",
        "%{time_total}",
        "-X",
        "POST",
        "-H",
        "Content-Type: application/json",
        "-d",
        json.dumps({"query": query}),
        "-o",
        "/dev/null",
        url,
    ]

    try:
        result = subprocess.run(cmd, capture_output=True, text=True, timeout=10)

        if result.returncode == 0:
            return float(result.stdout.strip()) * 1000  # Convert to ms
    except subprocess.TimeoutExpired:
        return -1.0
    except Exception:
        return -1.0

    return -1.0


def print_progress_bar(current: int, total: int, width: int = 30):
    """Print a simple progress bar."""
    percent = current / total
    filled = int(width * percent)
    bar = "█" * filled + "░" * (width - filled)
    print(f"\r  [{bar}] {current}/{total} ({percent * 100:.0f}%)", end="", flush=True)


def benchmark_service(service_name: str, config: dict) -> dict:
    """Run benchmark on a service."""
    url = config["url"]
    color = config["color"]

    print(f"\n{color}{'=' * 60}{NC}")
    print(f"{color}Benchmarking {service_name}{NC}")
    print(f"{color}{'=' * 60}{NC}")
    print(f"URL: {url}")
    print("Connection: Unix socket (via localhost)")

    results = {}

    for query_name, query_info in QUERIES.items():
        query = query_info["query"]
        description = query_info["description"]

        print(f"\n{YELLOW}[{query_name}]{NC} - {description}")

        # Warmup
        print("  Warming up... ", end="", flush=True)
        warmup_errors = 0
        for _ in range(WARMUP):
            if measure_request_time(url, query) < 0:
                warmup_errors += 1

        if warmup_errors > WARMUP / 2:
            print(f"{RED}FAILED (too many errors in warmup){NC}")
            results[query_name] = {"error": "Warmup failed"}
            continue
        else:
            print(f"{GREEN}done{NC}")

        # Measure
        print(f"  Running {ITERATIONS} requests:")
        times = []
        errors = 0

        for i in range(ITERATIONS):
            print_progress_bar(i + 1, ITERATIONS)
            t = measure_request_time(url, query)
            if t > 0:
                times.append(t)
            else:
                errors += 1

        print()  # New line after progress bar

        if times:
            times.sort()
            total_time = sum(times) / 1000  # Total in seconds

            results[query_name] = {
                "count": len(times),
                "errors": errors,
                "mean_ms": statistics.mean(times),
                "median_ms": statistics.median(times),
                "min_ms": min(times),
                "max_ms": max(times),
                "p90_ms": times[int(len(times) * 0.9)],
                "p95_ms": times[int(len(times) * 0.95)],
                "p99_ms": times[int(len(times) * 0.99)] if len(times) >= 100 else times[-1],
                "stdev_ms": statistics.stdev(times) if len(times) > 1 else 0,
                "throughput_rps": len(times) / total_time,
            }

            # Print summary
            print(
                f"  {GREEN}✓{NC} Mean: {results[query_name]['mean_ms']:.1f}ms, "
                f"P95: {results[query_name]['p95_ms']:.1f}ms, "
                f"Throughput: {results[query_name]['throughput_rps']:.1f} req/s"
            )

            if errors > 0:
                print(f"  {YELLOW}⚠ {errors} errors occurred{NC}")
        else:
            results[query_name] = {"error": "All requests failed"}
            print(f"  {RED}✗ FAILED - All requests failed{NC}")

    return results


def print_comparison(results: dict):
    """Print performance comparison."""
    if len(results) < 2:
        return

    print(f"\n{BLUE}{'=' * 80}{NC}")
    print(f"{BLUE}PERFORMANCE COMPARISON (Unix Socket Connection){NC}")
    print(f"{BLUE}{'=' * 80}{NC}")

    # Table header
    print(f"\n{'Query':<30} {'Framework':<12} {'Mean (ms)':<10} {'P95 (ms)':<10} {'RPS':<8}")
    print("-" * 70)

    # Collect data for summary
    fraiseql_wins = 0
    strawberry_wins = 0

    for query_name in QUERIES:
        first = True
        query_results = []

        for framework, framework_results in results.items():
            if query_name in framework_results and "error" not in framework_results[query_name]:
                stats = framework_results[query_name]
                query_display = query_name if first else ""

                color = SERVICES[framework]["color"] if framework in SERVICES else ""
                print(
                    f"{query_display:<30} {color}{framework:<12}{NC} "
                    f"{stats['mean_ms']:<10.1f} {stats['p95_ms']:<10.1f} "
                    f"{stats['throughput_rps']:<8.1f}"
                )

                query_results.append((framework, stats))
                first = False

        # Determine winner for this query
        if len(query_results) == 2:
            if query_results[0][1]["mean_ms"] < query_results[1][1]["mean_ms"]:
                if query_results[0][0] == "FraiseQL":
                    fraiseql_wins += 1
                else:
                    strawberry_wins += 1
            else:
                if query_results[1][0] == "FraiseQL":
                    fraiseql_wins += 1
                else:
                    strawberry_wins += 1

        if not first:  # Add spacing between queries
            print()

    # Performance ratios
    if "FraiseQL" in results and "Strawberry" in results:
        print(f"\n{YELLOW}Performance Ratios (FraiseQL vs Strawberry):{NC}")
        print("-" * 50)

        total_fraiseql_mean = 0
        total_strawberry_mean = 0
        valid_comparisons = 0

        for query_name in QUERIES:
            if (
                query_name in results["FraiseQL"]
                and query_name in results["Strawberry"]
                and "error" not in results["FraiseQL"][query_name]
                and "error" not in results["Strawberry"][query_name]
            ):
                f_mean = results["FraiseQL"][query_name]["mean_ms"]
                s_mean = results["Strawberry"][query_name]["mean_ms"]
                f_rps = results["FraiseQL"][query_name]["throughput_rps"]
                s_rps = results["Strawberry"][query_name]["throughput_rps"]

                total_fraiseql_mean += f_mean
                total_strawberry_mean += s_mean
                valid_comparisons += 1

                latency_ratio = s_mean / f_mean if f_mean > 0 else 0
                throughput_ratio = f_rps / s_rps if s_rps > 0 else 0

                winner_color = GREEN if latency_ratio > 1 else RED

                print(f"\n{query_name}:")
                print(
                    f"  Latency: {winner_color}{latency_ratio:.2f}x{NC} "
                    f"{'faster' if latency_ratio > 1 else 'slower'}"
                )
                print(
                    f"  Throughput: {winner_color}{throughput_ratio:.2f}x{NC} "
                    f"{'higher' if throughput_ratio > 1 else 'lower'}"
                )

        # Overall summary
        if valid_comparisons > 0:
            avg_fraiseql = total_fraiseql_mean / valid_comparisons
            avg_strawberry = total_strawberry_mean / valid_comparisons
            overall_ratio = avg_strawberry / avg_fraiseql

            print(f"\n{BLUE}{'=' * 50}{NC}")
            print(f"{BLUE}OVERALL PERFORMANCE SUMMARY{NC}")
            print(f"{BLUE}{'=' * 50}{NC}")

            print("\nAverage latency:")
            print(f"  FraiseQL:   {avg_fraiseql:.1f}ms")
            print(f"  Strawberry: {avg_strawberry:.1f}ms")

            winner = "FraiseQL" if overall_ratio > 1 else "Strawberry"
            winner_color = GREEN if winner == "FraiseQL" else RED

            print(
                f"\n{winner_color}🏆 {winner} is {abs(overall_ratio - 1) * 100:.0f}% "
                f"{'faster' if overall_ratio > 1 else 'slower'} overall{NC}"
            )

            print("\nQuery wins:")
            print(f"  FraiseQL:   {fraiseql_wins}")
            print(f"  Strawberry: {strawberry_wins}")


def main():
    """Run the benchmark."""
    print(f"{BLUE}FraiseQL Performance Benchmark{NC}")
    print(f"{BLUE}{'=' * 30}{NC}")
    print("Configuration:")
    print(f"  Iterations: {ITERATIONS}")
    print(f"  Warmup:     {WARMUP}")
    print("  Connection: Unix socket (no network overhead)")

    results = {}
    total_start = time.time()

    # Test both services
    for service_name, config in SERVICES.items():
        try:
            results[service_name] = benchmark_service(service_name, config)
        except Exception as e:
            print(f"{RED}Failed to benchmark {service_name}: {e}{NC}")
            results[service_name] = {"error": str(e)}

    # Save results
    timestamp = datetime.now(tz=timezone.utc).isoformat()
    filename = f"benchmark_results_{timestamp.replace(':', '-')}.json"

    # Load profile info if available
    profile_info = {}
    profile_path = Path("benchmark_profile.json")
    if profile_path.exists():
        with profile_path.open() as f:
            profile_info = json.load(f)

    output_path = Path(filename)
    with output_path.open("w") as f:
        json.dump(
            {
                "timestamp": timestamp,
                "connection_type": "unix_socket",
                "profile": profile_info,
                "configuration": {
                    "iterations": ITERATIONS,
                    "warmup": WARMUP,
                    "queries": len(QUERIES),
                },
                "results": results,
            },
            f,
            indent=2,
        )

    print(f"\n{GREEN}Results saved to: {filename}{NC}")

    # Print comparison
    print_comparison(results)

    total_time = time.time() - total_start
    print(f"\n{GREEN}✅ Total benchmark time: {total_time:.1f} seconds{NC}")


if __name__ == "__main__":
    main()
