#!/usr/bin/env python3
"""Simple benchmark to test Java GraphQL implementations"""

import contextlib
import statistics
import time
from typing import Dict

import requests


def benchmark_endpoint(url: str, query: Dict, name: str, num_requests: int = 100) -> Dict:
    """Run a simple benchmark against an endpoint"""
    response_times = []
    errors = 0

    # Warm up
    for _ in range(5):
        with contextlib.suppress(Exception):
            requests.post(url, json=query)

    # Run benchmark
    start_time = time.time()

    for _i in range(num_requests):
        try:
            req_start = time.perf_counter()
            response = requests.post(url, json=query)
            response_time = (time.perf_counter() - req_start) * 1000  # Convert to ms

            if response.status_code == 200:
                response_times.append(response_time)
            else:
                errors += 1
        except Exception:
            errors += 1

    total_time = time.time() - start_time

    if response_times:
        response_times.sort()
        return {
            "name": name,
            "avg_response_time_ms": statistics.mean(response_times),
            "p50_ms": response_times[len(response_times) // 2],
            "p95_ms": response_times[int(len(response_times) * 0.95)],
            "p99_ms": response_times[int(len(response_times) * 0.99)],
            "requests_per_second": len(response_times) / total_time,
            "errors": errors,
            "successful_requests": len(response_times),
        }
    return {
        "name": name,
        "errors": errors,
        "successful_requests": 0,
        "status": "Failed - no successful requests",
    }


def main():
    # Test queries
    simple_user_query = {
        "query": """
            query GetUser($id: ID!) {
                user(id: $id) {
                    id
                    name
                    email
                }
            }
        """,
        "variables": {"id": "1"},
    }

    # Test Java ORM endpoint
    java_orm_url = "http://localhost:8080/graphql"
    java_orm_result = benchmark_endpoint(
        java_orm_url,
        simple_user_query,
        "Java Spring + JPA/Hibernate",
        50,
    )

    # Test Java Optimized endpoint
    java_opt_url = "http://localhost:8080/optimized/user/1"
    try:
        # For the optimized endpoint, we use GET
        response_times = []
        start_time = time.time()

        for _ in range(50):
            req_start = time.perf_counter()
            response = requests.get(java_opt_url)
            response_time = (time.perf_counter() - req_start) * 1000
            if response.status_code == 200:
                response_times.append(response_time)

        total_time = time.time() - start_time
        response_times.sort()

        java_opt_result = {
            "name": "Java Optimized (Direct SQL)",
            "avg_response_time_ms": statistics.mean(response_times),
            "p50_ms": response_times[len(response_times) // 2],
            "p95_ms": response_times[int(len(response_times) * 0.95)],
            "requests_per_second": len(response_times) / total_time,
            "successful_requests": len(response_times),
        }
    except Exception as e:
        java_opt_result = {"name": "Java Optimized", "status": f"Failed: {e}"}

    # Print results

    for result in [java_orm_result, java_opt_result]:
        if "avg_response_time_ms" in result:
            pass
        else:
            pass

    # Add FraiseQL expected performance based on benchmarks

    if "avg_response_time_ms" in java_orm_result:
        fraiseql_expected = 3.8
        java_orm_result["avg_response_time_ms"] / fraiseql_expected

    if "avg_response_time_ms" in java_opt_result:
        pass


if __name__ == "__main__":
    main()
