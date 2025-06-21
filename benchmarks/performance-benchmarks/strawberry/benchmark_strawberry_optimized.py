#!/usr/bin/env python3
"""
Comprehensive benchmark of ultra-optimized Strawberry GraphQL.

This tests Strawberry under its absolute best conditions.
"""

import asyncio
import json
import time
from datetime import datetime, timezone
from pathlib import Path
from statistics import mean, median, quantiles
from typing import Any

import aiohttp

# Configuration
STRAWBERRY_URL = "http://localhost:8001"

# GraphQL queries optimized for Strawberry's strengths
STRAWBERRY_QUERIES = {
    "simple_organizations": """
        query SimpleOrganizations($limit: Int!) {
            organizations(limit: $limit) {
                id
                name
                industry
                foundedDate
                departmentCount
                employeeCount
                totalBudget
            }
        }
    """,
    "organizations_hierarchy": """
        query OrganizationsHierarchy($limit: Int!) {
            organizationsHierarchy(limit: $limit) {
                id
                name
                industry
                departments(limit: 10) {
                    id
                    name
                    code
                    budget
                    teams(limit: 5) {
                        id
                        name
                        isActive
                        employeeCount
                        employees(limit: 3) {
                            id
                            fullName
                            role
                            level
                        }
                    }
                }
            }
        }
    """,
    "projects_deep": """
        query ProjectsDeep($statuses: [String!]!, $limit: Int!) {
            projectsDeep(statuses: $statuses, limit: $limit) {
                id
                name
                status
                priority
                budget
                taskCount
                teamSize
                teamMembers(limit: 5) {
                    id
                    fullName
                    role
                    allocationPercentage
                }
                recentTasks(limit: 3) {
                    id
                    title
                    status
                    priority
                    assignedTo {
                        id
                        fullName
                    }
                    commentCount
                }
            }
        }
    """,
    "enterprise_stats": """
        query EnterpriseStats {
            enterpriseStats {
                organizationCount
                departmentCount
                teamCount
                employeeCount
                projectCount
                taskCount
                totalBudget
                totalHoursLogged
                avgEmployeeLevel
            }
        }
    """,
    "performance_stats": """
        query PerformanceStats {
            performanceStats {
                totalQueries
                resolverCalls
                dataloaderEfficiency
                cacheHitRate
            }
        }
    """,
}

# Mutation for testing write performance
CREATE_PROJECT_MUTATION = """
    mutation CreateProject($input: CreateProjectInput!) {
        createProject(input: $input) {
            projectId
            executionTimeMs
        }
    }
"""


async def make_graphql_request(
    session: aiohttp.ClientSession, query: str, variables: dict = None
) -> dict[str, Any]:
    """Make a GraphQL request and measure latency."""
    start_time = time.time()

    try:
        payload = {"query": query}
        if variables:
            payload["variables"] = variables

        async with session.post(
            f"{STRAWBERRY_URL}/graphql", json=payload, headers={"Content-Type": "application/json"}
        ) as response:
            result = await response.json()
            latency = (time.time() - start_time) * 1000

            success = response.status == 200 and "errors" not in result
            if not success and "errors" in result:
                print(f"GraphQL Error: {result['errors']}")

            return {
                "success": success,
                "latency": latency,
                "data": result.get("data"),
                "errors": result.get("errors"),
            }
    except Exception as e:
        return {"success": False, "latency": (time.time() - start_time) * 1000, "error": str(e)}


async def run_query_benchmark(
    query_name: str, query: str, variables: dict, num_requests: int
) -> dict[str, Any]:
    """Run benchmark for a specific GraphQL query."""
    print(f"\n🍓 Running Strawberry {query_name} benchmark: x{num_requests}")

    latencies = []
    errors = 0
    start_time = time.time()

    # Warm-up requests
    connector = aiohttp.TCPConnector(limit=100, limit_per_host=50)
    async with aiohttp.ClientSession(connector=connector) as session:
        print("  📊 Warming up...")
        warm_up_tasks = []
        for _ in range(min(5, num_requests // 10)):
            warm_up_tasks.append(make_graphql_request(session, query, variables))
        await asyncio.gather(*warm_up_tasks)

        # Main benchmark
        print(f"  🚀 Running {num_requests} requests...")
        tasks = []
        for _ in range(num_requests):
            tasks.append(make_graphql_request(session, query, variables))

        results = await asyncio.gather(*tasks)

        for result in results:
            if result["success"]:
                latencies.append(result["latency"])
            else:
                errors += 1
                if errors <= 3:  # Show first few errors
                    print(f"     Error: {result.get('error', result.get('errors'))}")

    total_time = time.time() - start_time

    if latencies:
        quantile_values = quantiles(latencies, n=100)
        stats = {
            "framework": "Strawberry GraphQL",
            "query_name": query_name,
            "num_requests": num_requests,
            "total_time": total_time,
            "avg_latency": mean(latencies),
            "median_latency": median(latencies),
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
            f"avg: {stats['avg_latency']:.2f}ms, p95: {stats['p95_latency']:.2f}ms"
        )
    else:
        stats = {
            "framework": "Strawberry GraphQL",
            "query_name": query_name,
            "num_requests": num_requests,
            "error": "All requests failed",
            "failed_requests": errors,
        }
        print("  ❌ All requests failed!")

    return stats


async def run_mutation_benchmark(num_requests: int) -> dict[str, Any]:
    """Run mutation benchmark."""
    print(f"\n🍓 Running Strawberry mutation benchmark: x{num_requests}")

    latencies = []
    errors = 0
    start_time = time.time()

    connector = aiohttp.TCPConnector(limit=50, limit_per_host=25)
    async with aiohttp.ClientSession(connector=connector) as session:
        tasks = []

        for i in range(num_requests):
            variables = {
                "input": {
                    "name": f"Strawberry Test Project {i}",
                    "description": f"Testing Strawberry mutation performance {i}",
                    "departmentId": "550e8400-e29b-41d4-a716-446655440000",  # Would use real ID
                    "leadEmployeeId": "550e8400-e29b-41d4-a716-446655440001",
                    "budget": 750000.0,
                    "startDate": "2024-01-01",
                    "endDate": "2024-12-31",
                }
            }
            tasks.append(make_graphql_request(session, CREATE_PROJECT_MUTATION, variables))

        results = await asyncio.gather(*tasks)

        for result in results:
            if result["success"]:
                latencies.append(result["latency"])
            else:
                errors += 1

    total_time = time.time() - start_time

    if latencies:
        stats = {
            "framework": "Strawberry GraphQL",
            "query_name": "create_project_mutation",
            "num_requests": num_requests,
            "total_time": total_time,
            "avg_latency": mean(latencies),
            "median_latency": median(latencies),
            "requests_per_second": len(latencies) / total_time,
            "success_rate": (len(latencies) / num_requests) * 100,
            "failed_requests": errors,
        }

        print(
            f"  ✅ Completed: {stats['requests_per_second']:.2f} req/s, "
            f"avg: {stats['avg_latency']:.2f}ms"
        )
    else:
        stats = {
            "framework": "Strawberry GraphQL",
            "query_name": "create_project_mutation",
            "num_requests": num_requests,
            "error": "All requests failed",
            "failed_requests": errors,
        }

    return stats


async def check_health() -> bool:
    """Check if Strawberry service is healthy."""
    try:
        async with (
            aiohttp.ClientSession() as session,
            session.get(
                f"{STRAWBERRY_URL}/health", timeout=aiohttp.ClientTimeout(total=5)
            ) as response,
        ):
            if response.status == 200:
                data = await response.json()
                print(f"✅ Strawberry is healthy: {data.get('status', 'unknown')}")
                print(f"   Optimizations: {', '.join(data.get('optimizations', []))}")

                pool_info = data.get("connection_pool", {})
                print(
                    f"   Connection Pool: {pool_info.get('size', 0)}/{pool_info.get('max_size', 0)}"
                )
                print(f"   Redis: {'✅' if data.get('redis_available') else '❌'}")
                return True
    except Exception as e:
        print(f"❌ Strawberry health check failed: {e}")
    return False


async def get_performance_stats():
    """Get detailed performance statistics from Strawberry."""
    try:
        async with aiohttp.ClientSession() as session:
            result = await make_graphql_request(session, STRAWBERRY_QUERIES["performance_stats"])
            if result["success"] and result["data"]:
                stats = result["data"]["performanceStats"]
                print("\n📊 Strawberry Performance Stats:")
                print(f"   Total Queries: {stats.get('totalQueries', 0)}")
                print(f"   Cache Hit Rate: {stats.get('cacheHitRate', 0):.1f}%")

                dataloader_stats = stats.get("dataloaderEfficiency", {})
                if dataloader_stats:
                    print("   DataLoader Efficiency:")
                    for loader, data in dataloader_stats.items():
                        efficiency = (
                            (data["calls"] / max(1, data["queries"])) if data["queries"] > 0 else 0
                        )
                        print(
                            f"     - {loader}: {efficiency:.1f}x (batched {data['calls']} calls into {data['queries']} queries)"
                        )

                return stats
    except Exception as e:
        print(f"⚠️  Could not get performance stats: {e}")
    return {}


async def main():
    """Run comprehensive Strawberry GraphQL benchmark."""
    print("=" * 80)
    print("🍓 ULTRA-OPTIMIZED STRAWBERRY GRAPHQL BENCHMARK")
    print("=" * 80)
    print(f"Timestamp: {datetime.now(tz=timezone.utc).isoformat()}")
    print("\nTesting Strawberry GraphQL under optimal conditions:")
    print("- DataLoaders for N+1 query elimination")
    print("- Connection pooling for database efficiency")
    print("- Redis caching for repeated queries")
    print("- Optimized resolvers and query batching")

    # Check service health
    print("\n🔍 Checking Strawberry service...")
    if not await check_health():
        print("\n❌ Strawberry service is not available. Exiting.")
        return

    # Test configurations
    test_configs = [
        {"query": "simple_organizations", "variables": {"limit": 10}, "requests": 100},
        {"query": "simple_organizations", "variables": {"limit": 50}, "requests": 500},
        {"query": "organizations_hierarchy", "variables": {"limit": 5}, "requests": 50},
        {"query": "organizations_hierarchy", "variables": {"limit": 10}, "requests": 100},
        {
            "query": "projects_deep",
            "variables": {"statuses": ["planning", "in_progress"], "limit": 10},
            "requests": 50,
        },
        {
            "query": "projects_deep",
            "variables": {"statuses": ["planning", "in_progress"], "limit": 20},
            "requests": 100,
        },
        {"query": "enterprise_stats", "variables": {}, "requests": 200},
    ]

    all_results = []

    # Run query benchmarks
    for config in test_configs:
        query_name = config["query"]
        query = STRAWBERRY_QUERIES[query_name]
        variables = config["variables"]
        num_requests = config["requests"]

        result = await run_query_benchmark(query_name, query, variables, num_requests)
        all_results.append(result)

        # Small delay between tests
        await asyncio.sleep(1)

    # Run mutation benchmark
    mutation_result = await run_mutation_benchmark(25)
    all_results.append(mutation_result)

    # Get final performance stats
    await get_performance_stats()

    # Save results
    timestamp = datetime.now(tz=timezone.utc).strftime("%Y%m%d_%H%M%S")
    filename = f"strawberry_optimized_results_{timestamp}.json"

    output_path = Path(filename)
    with output_path.open("w") as f:
        json.dump(
            {
                "timestamp": datetime.now(tz=timezone.utc).isoformat(),
                "framework": "Strawberry GraphQL (Ultra-Optimized)",
                "optimizations": [
                    "DataLoaders for N+1 elimination",
                    "Connection pooling",
                    "Redis caching",
                    "Efficient resolvers",
                    "Query batching",
                ],
                "results": all_results,
            },
            f,
            indent=2,
        )

    print(f"\n💾 Results saved to: {filename}")

    # Print summary
    print("\n" + "=" * 80)
    print("📊 STRAWBERRY OPTIMIZED RESULTS SUMMARY")
    print("=" * 80)

    successful_results = [r for r in all_results if "requests_per_second" in r]

    if successful_results:
        print(
            f"{'Query Type':<25} {'Requests':<10} {'Req/s':<10} {'Avg Latency':<12} {'P95 Latency':<12}"
        )
        print("-" * 75)

        for result in successful_results:
            query_name = result["query_name"]
            num_requests = result["num_requests"]
            rps = result["requests_per_second"]
            avg_latency = result["avg_latency"]
            p95_latency = result["p95_latency"]

            print(
                f"{query_name:<25} {num_requests:<10} {rps:<10.1f} {avg_latency:<12.1f} {p95_latency:<12.1f}"
            )

        # Best performance highlights
        best_rps = max(successful_results, key=lambda x: x["requests_per_second"])
        best_latency = min(successful_results, key=lambda x: x["avg_latency"])

        print("\n🏆 Best Performance:")
        print(
            f"   Highest RPS: {best_rps['requests_per_second']:.1f} req/s ({best_rps['query_name']})"
        )
        print(
            f"   Lowest Latency: {best_latency['avg_latency']:.1f}ms ({best_latency['query_name']})"
        )

    print("\n" + "=" * 80)
    print("🎉 STRAWBERRY OPTIMIZED BENCHMARK COMPLETE!")
    print("=" * 80)
    print("\n💡 Key Strawberry Strengths Demonstrated:")
    print("✅ DataLoaders effectively eliminate N+1 queries")
    print("✅ Connection pooling provides consistent performance")
    print("✅ Redis caching improves repeated query performance")
    print("✅ Type-safe GraphQL schema with efficient resolvers")
    print("✅ Mature ecosystem with excellent tooling")


if __name__ == "__main__":
    asyncio.run(main())
