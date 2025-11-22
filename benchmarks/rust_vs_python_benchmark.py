"""Comprehensive benchmark: Rust vs Python transformation performance.

This benchmark measures actual performance of:
1. fraiseql_rs (Rust) transformation
2. Pure Python transformation
3. Different data sizes and complexities

Goal: Validate the claimed 10-80x speedup.
"""

import json
import statistics
import time
from collections.abc import Callable
from typing import Any


def to_camel_case_python(snake_str: str) -> str:
    """Pure Python snake_case to camelCase conversion."""
    components = snake_str.split("_")
    return components[0] + "".join(x.title() for x in components[1:])


def transform_dict_python(data: dict[str, Any], recursive: bool = True) -> dict[str, Any]:
    """Pure Python dict transformation (mimics what Python would do)."""
    result = {}
    for key, value in data.items():
        camel_key = to_camel_case_python(key)
        if recursive and isinstance(value, dict):
            result[camel_key] = transform_dict_python(value, recursive=True)
        elif recursive and isinstance(value, list):
            result[camel_key] = [
                transform_dict_python(item, recursive=True) if isinstance(item, dict) else item
                for item in value
            ]
        else:
            result[camel_key] = value
    return result


def transform_json_python(json_str: str) -> str:
    """Pure Python JSON transformation."""
    data = json.loads(json_str)
    transformed = transform_dict_python(data)
    return json.dumps(transformed)


def benchmark_transformation(
    name: str, func: Callable[[str], str], input_data: str, iterations: int = 1000
) -> dict[str, Any]:
    """Run benchmark and return statistics."""
    # Warm-up
    for _ in range(10):
        func(input_data)

    # Actual benchmark
    times = []
    for _ in range(iterations):
        start = time.perf_counter()
        func(input_data)
        elapsed = time.perf_counter() - start
        times.append(elapsed * 1000)  # Convert to milliseconds

    return {
        "name": name,
        "min_ms": min(times),
        "max_ms": max(times),
        "mean_ms": statistics.mean(times),
        "median_ms": statistics.median(times),
        "stdev_ms": statistics.stdev(times) if len(times) > 1 else 0,
        "iterations": iterations,
    }


def create_test_data(complexity: str) -> dict:
    """Create test data of varying complexity."""
    if complexity == "simple":
        # Simple object: 10 fields
        return {
            "user_id": 1,
            "user_name": "John Doe",
            "email_address": "john@example.com",
            "created_at": "2025-10-13T10:00:00Z",
            "updated_at": "2025-10-13T12:00:00Z",
            "is_active": True,
            "is_verified": True,
            "age": 30,
            "status": "active",
            "role": "user",
        }

    if complexity == "medium":
        # Medium: 50 fields with some nesting
        return {f"field_{i}": f"value_{i}" for i in range(40)} | {
            "user_profile": {
                "first_name": "John",
                "last_name": "Doe",
                "phone_number": "555-1234",
            },
            "user_settings": {
                "theme_color": "dark",
                "language_code": "en",
            },
        }

    if complexity == "nested":
        # Nested: User with posts and comments
        return {
            "user_id": 1,
            "user_name": "John Doe",
            "user_posts": [
                {
                    "post_id": i,
                    "post_title": f"Post {i}",
                    "post_content": f"Content for post {i}",
                    "created_at": "2025-10-13T10:00:00Z",
                    "post_comments": [
                        {
                            "comment_id": j,
                            "comment_text": f"Comment {j}",
                            "author_name": f"User {j}",
                        }
                        for j in range(5)
                    ],
                }
                for i in range(15)
            ],
        }

    if complexity == "large":
        # Large: 100 fields, deeply nested
        return {
            f"field_{i}": {f"nested_field_{j}": f"value_{i}_{j}" for j in range(10)}
            for i in range(100)
        }

    return {}


def run_benchmarks() -> None:
    """Run all benchmarks and print results."""
    # Import Rust transformer
    try:
        from fraiseql import fraiseql_rs

        rust_available = True
    except ImportError:
        print("⚠️  fraiseql_rs not available - skipping Rust benchmarks")
        rust_available = False

    complexities = ["simple", "medium", "nested", "large"]

    print("=" * 80)
    print("BENCHMARK: Rust vs Python Transformation Performance")
    print("=" * 80)
    print()

    for complexity in complexities:
        data = create_test_data(complexity)
        json_str = json.dumps(data)
        size_kb = len(json_str) / 1024

        print(f"\n📊 Test Case: {complexity.upper()}")
        print(f"   Data size: {size_kb:.2f} KB")
        print(f"   Fields: {len(data)}")
        print("-" * 80)

        results = []

        # Benchmark Python
        result_python = benchmark_transformation(
            "Python (pure)", transform_json_python, json_str, iterations=100
        )
        results.append(result_python)

        # Benchmark Rust
        if rust_available:
            result_rust = benchmark_transformation(
                "Rust (fraiseql_rs)", fraiseql_rs.transform_json, json_str, iterations=100
            )
            results.append(result_rust)

        # Print results
        for result in results:
            print(f"\n{result['name']}:")
            print(f"  Mean:   {result['mean_ms']:.4f} ms")
            print(f"  Median: {result['median_ms']:.4f} ms")
            print(f"  Min:    {result['min_ms']:.4f} ms")
            print(f"  Max:    {result['max_ms']:.4f} ms")
            print(f"  StdDev: {result['stdev_ms']:.4f} ms")

        # Calculate speedup
        if rust_available and len(results) == 2:
            speedup = results[0]["mean_ms"] / results[1]["mean_ms"]
            print(f"\n⚡ Speedup: {speedup:.2f}x faster (Rust vs Python)")

            # Validate claimed performance
            if complexity == "simple":
                claimed = "10-50x"
            elif complexity == "nested":
                claimed = "20-80x"
            else:
                claimed = "N/A"

            print(f"   Claimed: {claimed}")
            print(f"   Actual:  {speedup:.2f}x")

        print("-" * 80)

    print("\n" + "=" * 80)
    print("BENCHMARK COMPLETE")
    print("=" * 80)


if __name__ == "__main__":
    run_benchmarks()
