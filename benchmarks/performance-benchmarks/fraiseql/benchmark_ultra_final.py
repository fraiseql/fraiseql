#!/usr/bin/env python3
"""
Final benchmark comparing:

1. Ultra-optimized FraiseQL (original)
2. Ultra-optimized FraiseQL with read replicas + Nginx
3. Strawberry GraphQL (baseline)
"""

import asyncio
import json
import time
from datetime import datetime, timezone
from pathlib import Path
from statistics import mean, median, quantiles, stdev
from typing import Any

import aiohttp

# Configuration
FRAISEQL_ULTRA_URL = "http://localhost:8000"
FRAISEQL_REPLICAS_URL = "http://localhost:8080"  # Nginx load balancer
STRAWBERRY_URL = "http://localhost:8001"

# Test configurations
TEST_CONFIGS = [
    {"query_type": "users", "requests": 100, "limit": 100},
    {"query_type": "products", "requests": 100, "limit": 100},
    {"query_type": "users", "requests": 500, "limit": 50},
    {"query_type": "products", "requests": 500, "limit": 50},
    {"query_type": "users", "requests": 1000, "limit": 100},
    {"query_type": "products", "requests": 1000, "limit": 100},
]


async def make_request(session: aiohttp.ClientSession, url: str, query: str) -> dict[str, Any]:
    """Make a single GraphQL request and measure latency."""
    start_time = time.time()

    try:
        if "/benchmark/" in url:
            # Direct REST API call for FraiseQL
            async with session.get(url) as response:
                result = await response.json()
                latency = (time.time() - start_time) * 1000
                return {"success": response.status == 200, "latency": latency, "data": result}
        else:
            # GraphQL API call for Strawberry
            async with session.post(
                url, json={"query": query}, headers={"Content-Type": "application/json"}
            ) as response:
                result = await response.json()
                latency = (time.time() - start_time) * 1000
                return {
                    "success": response.status == 200 and "errors" not in result,
                    "latency": latency,
                    "data": result,
                }
    except Exception as e:
        return {"success": False, "latency": (time.time() - start_time) * 1000, "error": str(e)}


async def run_benchmark(
    framework: str, base_url: str, query_type: str, num_requests: int, limit: int = 100
) -> dict[str, Any]:
    """Run benchmark for a specific framework and query."""
    print(f"\n🏃 Running {framework} benchmark: {query_type} x{num_requests} (limit={limit})")

    # Prepare query
    if framework in ["FraiseQL-Ultra", "FraiseQL-Replicas"]:
        url = f"{base_url}/benchmark/{query_type}?limit={limit}"
        query = None
    else:  # Strawberry
        url = f"{base_url}/graphql"
        if query_type == "users":
            query = f"""
            query {{
                users(limit: {limit}) {{
                    id
                    email
                    username
                    fullName
                    createdAt
                    updatedAt
                    orderCount
                    totalSpent
                }}
            }}
            """
        else:  # products
            query = f"""
            query {{
                products(limit: {limit}) {{
                    id
                    name
                    description
                    price
                    stock
                    categoryId
                    createdAt
                    updatedAt
                    category {{
                        id
                        name
                    }}
                }}
            }}
            """

    # Warm-up requests
    print("  📊 Warming up...")
    async with aiohttp.ClientSession() as session:
        warm_up_tasks = []
        for _ in range(min(10, num_requests // 10)):
            if query:
                warm_up_tasks.append(make_request(session, url, query))
            else:
                warm_up_tasks.append(make_request(session, url, None))
        await asyncio.gather(*warm_up_tasks)

    # Run actual benchmark
    print(f"  🚀 Running {num_requests} requests...")
    latencies = []
    errors = 0
    start_time = time.time()

    # Use connection pooling
    connector = aiohttp.TCPConnector(limit=100, limit_per_host=50)
    async with aiohttp.ClientSession(connector=connector) as session:
        tasks = []
        for _ in range(num_requests):
            if query:
                tasks.append(make_request(session, url, query))
            else:
                tasks.append(make_request(session, url, None))

        results = await asyncio.gather(*tasks)

        for result in results:
            if result["success"]:
                latencies.append(result["latency"])
            else:
                errors += 1

    total_time = time.time() - start_time

    # Calculate statistics
    if latencies:
        quantile_values = quantiles(latencies, n=100)
        stats = {
            "framework": framework,
            "query_type": query_type,
            "num_requests": num_requests,
            "limit": limit,
            "total_time": total_time,
            "avg_latency": mean(latencies),
            "median_latency": median(latencies),
            "std_dev": stdev(latencies) if len(latencies) > 1 else 0,
            "p95_latency": quantile_values[94] if len(quantile_values) > 94 else max(latencies),
            "p99_latency": quantile_values[98] if len(quantile_values) > 98 else max(latencies),
            "min_latency": min(latencies),
            "max_latency": max(latencies),
            "requests_per_second": len(latencies) / total_time,
            "success_rate": (len(latencies) / num_requests) * 100,
            "failed_requests": errors,
        }

        print(
            f"  ✅ Completed: {stats['requests_per_second']:.2f} req/s, "
            f"avg latency: {stats['avg_latency']:.2f}ms"
        )
    else:
        stats = {
            "framework": framework,
            "query_type": query_type,
            "num_requests": num_requests,
            "limit": limit,
            "error": "All requests failed",
            "failed_requests": errors,
        }
        print("  ❌ All requests failed!")

    return stats


async def check_health(name: str, url: str) -> bool:
    """Check if service is healthy."""
    try:
        async with (
            aiohttp.ClientSession() as session,
            session.get(f"{url}/health", timeout=aiohttp.ClientTimeout(total=5)) as response,
        ):
            if response.status == 200:
                data = await response.json()
                print(f"✅ {name} is healthy: {data.get('status', 'unknown')}")
                if "optimizations" in data:
                    print(f"   Optimizations: {', '.join(data['optimizations'])}")
                return True
    except Exception as e:
        print(f"❌ {name} health check failed: {e}")
    return False


async def main():
    """Run the complete benchmark comparison."""
    print("=" * 80)
    print("🏆 FINAL ULTRA-OPTIMIZED FRAISEQL BENCHMARK")
    print("=" * 80)
    print(f"Timestamp: {datetime.now(tz=timezone.utc).isoformat()}")

    # Check service health
    print("\n🔍 Checking services...")
    services_healthy = await asyncio.gather(
        check_health("FraiseQL Ultra", FRAISEQL_ULTRA_URL),
        check_health("FraiseQL Replicas+Nginx", FRAISEQL_REPLICAS_URL),
        check_health("Strawberry", STRAWBERRY_URL),
    )

    if not all(services_healthy):
        print("\n⚠️  Not all services are healthy. Results may be affected.")
        await asyncio.sleep(2)

    # Run benchmarks
    all_results = []

    for config in TEST_CONFIGS:
        print(f"\n{'=' * 60}")
        print(
            f"📋 Test: {config['query_type']} query, {config['requests']} requests, limit={config['limit']}"
        )
        print(f"{'=' * 60}")

        # Run benchmarks for each framework
        for framework, url in [
            ("FraiseQL-Ultra", FRAISEQL_ULTRA_URL),
            ("FraiseQL-Replicas", FRAISEQL_REPLICAS_URL),
            ("Strawberry", STRAWBERRY_URL),
        ]:
            result = await run_benchmark(
                framework=framework,
                base_url=url,
                query_type=config["query_type"],
                num_requests=config["requests"],
                limit=config["limit"],
            )
            all_results.append(result)

            # Small delay between frameworks
            await asyncio.sleep(1)

    # Save results
    timestamp = datetime.now(tz=timezone.utc).strftime("%Y%m%d_%H%M%S")
    filename = f"benchmark_final_results_{timestamp}.json"

    output_path = Path(filename)
    with output_path.open("w") as f:
        json.dump(
            {
                "timestamp": datetime.now(tz=timezone.utc).isoformat(),
                "configurations": {
                    "fraiseql_ultra": "Multi-tier pools + Multi-level cache + Projection tables",
                    "fraiseql_replicas": "Ultra + Read replicas + Nginx load balancing",
                    "strawberry": "Default configuration",
                },
                "results": all_results,
            },
            f,
            indent=2,
        )

    print(f"\n💾 Results saved to: {filename}")

    # Print summary
    print("\n" + "=" * 80)
    print("📊 FINAL RESULTS SUMMARY")
    print("=" * 80)

    # Group results by test configuration
    for config in TEST_CONFIGS:
        print(
            f"\n🎯 {config['query_type'].upper()} - {config['requests']} requests (limit={config['limit']})"
        )
        print("-" * 60)

        config_results = [
            r
            for r in all_results
            if r.get("query_type") == config["query_type"]
            and r.get("num_requests") == config["requests"]
            and r.get("limit") == config["limit"]
            and "requests_per_second" in r
        ]

        if config_results:
            # Sort by performance
            config_results.sort(key=lambda x: x["requests_per_second"], reverse=True)

            config_results[0]["requests_per_second"]

            for i, result in enumerate(config_results):
                rps = result["requests_per_second"]
                avg_latency = result["avg_latency"]
                p95_latency = result["p95_latency"]

                if i == 0:
                    improvement = "🥇 WINNER"
                else:
                    improvement_pct = ((rps / config_results[-1]["requests_per_second"]) - 1) * 100
                    improvement = f"+{improvement_pct:.1f}% vs baseline"

                print(
                    f"{result['framework']:20} | {rps:7.1f} req/s | "
                    f"Avg: {avg_latency:6.1f}ms | P95: {p95_latency:6.1f}ms | {improvement}"
                )

    print("\n" + "=" * 80)
    print("🎉 BENCHMARK COMPLETE!")
    print("=" * 80)


if __name__ == "__main__":
    asyncio.run(main())
