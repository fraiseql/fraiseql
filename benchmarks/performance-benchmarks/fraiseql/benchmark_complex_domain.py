#!/usr/bin/env python3
"""
Comprehensive benchmark comparing FraiseQL and Strawberry on:

1. Simple queries (baseline)
2. Complex nested queries (where FraiseQL should excel)
3. Mutations (create, update operations)
"""

import asyncio
import json
import time
import uuid
from datetime import date, datetime, timezone
from pathlib import Path
from statistics import mean, median, quantiles, stdev
from typing import Any

import aiohttp

# Configuration
FRAISEQL_URL = "http://localhost:8000"
STRAWBERRY_URL = "http://localhost:8001"

# Test configurations for different complexity levels
TEST_CONFIGS = [
    # Simple queries (baseline)
    {
        "category": "Simple Queries",
        "tests": [
            {"type": "organizations_simple", "requests": 100, "limit": 10},
            {"type": "organizations_simple", "requests": 500, "limit": 50},
        ],
    },
    # Complex nested queries
    {
        "category": "Complex Nested Queries",
        "tests": [
            {"type": "organizations_hierarchy", "requests": 50, "limit": 5},
            {"type": "organizations_hierarchy", "requests": 100, "limit": 10},
            {"type": "projects_deep", "requests": 50, "limit": 10},
            {"type": "projects_full_details", "requests": 25, "limit": 5},
        ],
    },
    # Mutation benchmarks
    {
        "category": "Mutations",
        "tests": [
            {"type": "create_project", "requests": 50},
            {"type": "assign_employee", "requests": 100},
            {"type": "update_task_status", "requests": 200},
            {"type": "batch_create_tasks", "requests": 25, "task_count": 10},
        ],
    },
]


async def make_request(
    session: aiohttp.ClientSession,
    url: str,
    method: str = "GET",
    json_data: dict = None,
    query: str = None,
) -> dict[str, Any]:
    """Make a single request and measure latency."""
    start_time = time.time()

    try:
        if method == "GET":
            async with session.get(url) as response:
                result = await response.json()
                latency = (time.time() - start_time) * 1000
                return {"success": response.status == 200, "latency": latency, "data": result}
        elif method == "POST":
            if query:  # GraphQL
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
            else:  # REST
                async with session.post(url, json=json_data) as response:
                    result = await response.json()
                    latency = (time.time() - start_time) * 1000
                    return {"success": response.status == 200, "latency": latency, "data": result}
    except Exception as e:
        return {"success": False, "latency": (time.time() - start_time) * 1000, "error": str(e)}


async def get_test_data(session: aiohttp.ClientSession, base_url: str):
    """Get IDs for testing."""
    # Get organization IDs
    await make_request(
        session, f"{base_url}/benchmark/stats" if "8000" in base_url else f"{base_url}/stats"
    )

    # For FraiseQL, we can query directly
    if "8000" in base_url:
        return {
            "org_ids": [],  # Will use random selection
            "project_ids": [],
            "employee_ids": [],
            "department_ids": [],
        }

    # For Strawberry, we need to fetch via GraphQL
    # This would require implementing GraphQL queries in Strawberry
    return {"org_ids": [], "project_ids": [], "employee_ids": [], "department_ids": []}


def generate_strawberry_query(query_type: str, limit: int = 10) -> str:
    """Generate GraphQL query for Strawberry."""
    queries = {
        "organizations_simple": f"""
            query {{
                organizations(limit: {limit}) {{
                    id
                    name
                    description
                    industry
                    foundedDate
                    createdAt
                    departmentCount
                    employeeCount
                }}
            }}
        """,
        "organizations_hierarchy": f"""
            query {{
                organizationsHierarchy(limit: {limit}) {{
                    id
                    name
                    description
                    industry
                    departments {{
                        id
                        name
                        code
                        budget
                        teams {{
                            id
                            name
                            description
                            isActive
                            employeeCount
                            employees(limit: 5) {{
                                id
                                fullName
                                email
                                role
                                level
                                skills
                            }}
                        }}
                    }}
                }}
            }}
        """,
        "projects_deep": f"""
            query {{
                projects(statuses: ["planning", "in_progress"], limit: {limit}) {{
                    id
                    name
                    description
                    status
                    priority
                    budget
                    startDate
                    endDate
                    milestones
                    department {{
                        id
                        name
                        code
                        organization {{
                            id
                            name
                            industry
                        }}
                    }}
                    leadEmployee {{
                        id
                        fullName
                        email
                        role
                    }}
                    taskCount
                    completedTaskCount
                    teamSize
                    totalHoursLogged
                }}
            }}
        """,
        "projects_full_details": f"""
            query {{
                projectsFullDetails(limit: {limit}) {{
                    id
                    name
                    description
                    status
                    priority
                    budget
                    startDate
                    endDate
                    milestones
                    dependencies
                    department {{
                        id
                        name
                        code
                        organization {{
                            id
                            name
                        }}
                    }}
                    leadEmployee {{
                        id
                        fullName
                        email
                        role
                        team {{
                            id
                            name
                        }}
                    }}
                    teamMembers(limit: 10) {{
                        id
                        fullName
                        role
                        allocation
                        startDate
                    }}
                    recentTasks(limit: 5) {{
                        id
                        title
                        status
                        priority
                        dueDate
                        assignedTo {{
                            id
                            fullName
                        }}
                        commentCount
                    }}
                    timeAnalytics {{
                        totalHours
                        billableHours
                        uniqueContributors
                        averageHoursPerTask
                    }}
                    documents(limit: 3) {{
                        id
                        title
                        status
                        version
                        author {{
                            id
                            fullName
                        }}
                        updatedAt
                    }}
                }}
            }}
        """,
    }

    return queries.get(query_type, queries["organizations_simple"])


def generate_strawberry_mutation(mutation_type: str, data: dict) -> str:
    """Generate GraphQL mutation for Strawberry."""
    mutations = {
        "create_project": f"""
            mutation {{
                createProject(input: {{
                    name: "{data.get("name", "Test Project")}"
                    description: "{data.get("description", "Test Description")}"
                    departmentId: "{data.get("department_id", str(uuid.uuid4()))}"
                    leadEmployeeId: "{data.get("lead_employee_id", str(uuid.uuid4()))}"
                    budget: {data.get("budget", 100000)}
                    startDate: "{data.get("start_date", date.today().isoformat())}"
                    endDate: "{data.get("end_date", date.today().isoformat())}"
                }}) {{
                    projectId
                    executionTimeMs
                }}
            }}
        """,
        "assign_employee": f"""
            mutation {{
                assignEmployee(input: {{
                    projectId: "{data.get("project_id", str(uuid.uuid4()))}"
                    employeeId: "{data.get("employee_id", str(uuid.uuid4()))}"
                    role: "{data.get("role", "Developer")}"
                    allocationPercentage: {data.get("allocation_percentage", 100)}
                }}) {{
                    memberId
                    executionTimeMs
                }}
            }}
        """,
        "update_task_status": f"""
            mutation {{
                updateTaskStatus(input: {{
                    taskId: "{data.get("task_id", str(uuid.uuid4()))}"
                    newStatus: "{data.get("new_status", "in_progress")}"
                    actorId: "{data.get("actor_id", str(uuid.uuid4()))}"
                }}) {{
                    success
                    executionTimeMs
                }}
            }}
        """,
    }

    return mutations.get(mutation_type, "")


async def run_query_benchmark(
    framework: str, base_url: str, query_type: str, num_requests: int, limit: int = 10
) -> dict[str, Any]:
    """Run query benchmark for a specific framework."""
    print(f"\n🏃 Running {framework} {query_type} benchmark: x{num_requests}")

    latencies = []
    errors = 0
    start_time = time.time()

    connector = aiohttp.TCPConnector(limit=100, limit_per_host=50)
    async with aiohttp.ClientSession(connector=connector) as session:
        tasks = []

        for _ in range(num_requests):
            if framework == "FraiseQL":
                if query_type == "organizations_simple":
                    url = f"{base_url}/benchmark/organizations/simple?limit={limit}"
                elif query_type == "organizations_hierarchy":
                    url = f"{base_url}/benchmark/organizations/hierarchy?limit={limit}"
                elif query_type == "projects_deep":
                    url = f"{base_url}/benchmark/projects/deep?limit={limit}"
                elif query_type == "projects_full_details":
                    url = f"{base_url}/benchmark/projects/full-details?limit={limit}"
                else:
                    continue

                tasks.append(make_request(session, url))
            else:  # Strawberry
                query = generate_strawberry_query(query_type, limit)
                url = f"{base_url}/graphql"
                tasks.append(make_request(session, url, "POST", query=query))

        results = await asyncio.gather(*tasks)

        for result in results:
            if result["success"]:
                latencies.append(result["latency"])
            else:
                errors += 1

    total_time = time.time() - start_time

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
            f"avg: {stats['avg_latency']:.2f}ms, p95: {stats['p95_latency']:.2f}ms"
        )
    else:
        stats = {
            "framework": framework,
            "query_type": query_type,
            "num_requests": num_requests,
            "error": "All requests failed",
            "failed_requests": errors,
        }
        print("  ❌ All requests failed!")

    return stats


async def run_mutation_benchmark(
    framework: str, base_url: str, mutation_type: str, num_requests: int, **kwargs
) -> dict[str, Any]:
    """Run mutation benchmark."""
    print(f"\n🏃 Running {framework} {mutation_type} mutation benchmark: x{num_requests}")

    latencies = []
    errors = 0
    start_time = time.time()

    connector = aiohttp.TCPConnector(limit=50, limit_per_host=25)
    async with aiohttp.ClientSession(connector=connector) as session:
        tasks = []

        for i in range(num_requests):
            if framework == "FraiseQL":
                if mutation_type == "create_project":
                    url = f"{base_url}/benchmark/mutations/create-project"
                    data = {
                        "name": f"Benchmark Project {i}",
                        "description": f"Created during benchmark run {i}",
                        "department_id": str(uuid.uuid4()),  # Would use real IDs in production
                        "lead_employee_id": str(uuid.uuid4()),
                        "budget": "1000000.00",
                        "start_date": date.today().isoformat(),
                        "end_date": date.today().isoformat(),
                    }
                    tasks.append(make_request(session, url, "POST", json_data=data))

                elif mutation_type == "batch_create_tasks":
                    url = f"{base_url}/benchmark/mutations/batch-create-tasks?project_id={uuid.uuid4()}&count={kwargs.get('task_count', 10)}"
                    tasks.append(make_request(session, url, "POST"))

            else:  # Strawberry
                mutation_data = {
                    "name": f"Benchmark Project {i}",
                    "department_id": str(uuid.uuid4()),
                    "lead_employee_id": str(uuid.uuid4()),
                }
                mutation = generate_strawberry_mutation(mutation_type, mutation_data)
                url = f"{base_url}/graphql"
                tasks.append(make_request(session, url, "POST", query=mutation))

        results = await asyncio.gather(*tasks)

        for result in results:
            if result["success"]:
                latencies.append(result["latency"])
            else:
                errors += 1

    total_time = time.time() - start_time

    if latencies:
        stats = {
            "framework": framework,
            "mutation_type": mutation_type,
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
            "framework": framework,
            "mutation_type": mutation_type,
            "num_requests": num_requests,
            "error": "All requests failed",
            "failed_requests": errors,
        }

    return stats


async def check_health(name: str, url: str) -> bool:
    """Check if service is healthy."""
    try:
        async with (
            aiohttp.ClientSession() as session,
            session.get(f"{url}/health", timeout=aiohttp.ClientTimeout(total=5)) as response,
        ):
            if response.status == 200:
                print(f"✅ {name} is healthy")
                return True
    except Exception as e:
        print(f"❌ {name} health check failed: {e}")
    return False


async def main():
    """Run the complete complex domain benchmark."""
    print("=" * 80)
    print("🏆 FRAISEQL COMPLEX DOMAIN BENCHMARK")
    print("=" * 80)
    print(f"Timestamp: {datetime.now(tz=timezone.utc).isoformat()}")
    print("\nThis benchmark tests:")
    print("1. Simple queries (baseline performance)")
    print("2. Complex nested queries (FraiseQL's strength)")
    print("3. Mutations (write operations)")

    # Check service health
    print("\n🔍 Checking services...")
    services_healthy = await asyncio.gather(
        check_health("FraiseQL", FRAISEQL_URL), check_health("Strawberry", STRAWBERRY_URL)
    )

    if not all(services_healthy):
        print("\n⚠️  Not all services are healthy.")
        return

    # Run benchmarks
    all_results = []

    for category in TEST_CONFIGS:
        print(f"\n{'=' * 60}")
        print(f"📋 Category: {category['category']}")
        print(f"{'=' * 60}")

        for test in category["tests"]:
            if test["type"] in [
                "organizations_simple",
                "organizations_hierarchy",
                "projects_deep",
                "projects_full_details",
            ]:
                # Query benchmarks
                for framework, url in [("FraiseQL", FRAISEQL_URL), ("Strawberry", STRAWBERRY_URL)]:
                    result = await run_query_benchmark(
                        framework=framework,
                        base_url=url,
                        query_type=test["type"],
                        num_requests=test["requests"],
                        limit=test.get("limit", 10),
                    )
                    result["category"] = category["category"]
                    all_results.append(result)
                    await asyncio.sleep(1)

            else:
                # Mutation benchmarks
                for framework, url in [("FraiseQL", FRAISEQL_URL), ("Strawberry", STRAWBERRY_URL)]:
                    result = await run_mutation_benchmark(
                        framework=framework,
                        base_url=url,
                        mutation_type=test["type"],
                        num_requests=test["requests"],
                        task_count=test.get("task_count", 10),
                    )
                    result["category"] = category["category"]
                    all_results.append(result)
                    await asyncio.sleep(1)

    # Save results
    timestamp = datetime.now(tz=timezone.utc).strftime("%Y%m%d_%H%M%S")
    filename = f"benchmark_complex_results_{timestamp}.json"

    output_path = Path(filename)
    with output_path.open("w") as f:
        json.dump(
            {
                "timestamp": datetime.now(tz=timezone.utc).isoformat(),
                "description": "Complex domain benchmark comparing FraiseQL and Strawberry",
                "results": all_results,
            },
            f,
            indent=2,
        )

    print(f"\n💾 Results saved to: {filename}")

    # Print summary by category
    print("\n" + "=" * 80)
    print("📊 RESULTS SUMMARY BY CATEGORY")
    print("=" * 80)

    for category in TEST_CONFIGS:
        print(f"\n🎯 {category['category']}")
        print("-" * 60)

        category_results = [
            r
            for r in all_results
            if r.get("category") == category["category"] and "requests_per_second" in r
        ]

        if category_results:
            # Group by test type
            test_types = {r.get("query_type", r.get("mutation_type")) for r in category_results}

            for test_type in test_types:
                print(f"\n  📌 {test_type}")

                test_results = [
                    r
                    for r in category_results
                    if r.get("query_type", r.get("mutation_type")) == test_type
                ]

                # Sort by performance
                test_results.sort(key=lambda x: x["requests_per_second"], reverse=True)

                if len(test_results) >= 2:
                    fraiseql_result = next(
                        (r for r in test_results if r["framework"] == "FraiseQL"), None
                    )
                    strawberry_result = next(
                        (r for r in test_results if r["framework"] == "Strawberry"), None
                    )

                    if fraiseql_result and strawberry_result:
                        improvement = (
                            (
                                fraiseql_result["requests_per_second"]
                                / strawberry_result["requests_per_second"]
                            )
                            - 1
                        ) * 100

                        print(
                            f"    FraiseQL:   {fraiseql_result['requests_per_second']:7.1f} req/s | "
                            f"Avg: {fraiseql_result['avg_latency']:6.1f}ms"
                        )
                        print(
                            f"    Strawberry: {strawberry_result['requests_per_second']:7.1f} req/s | "
                            f"Avg: {strawberry_result['avg_latency']:6.1f}ms"
                        )

                        if improvement > 0:
                            print(f"    🚀 FraiseQL is {improvement:.1f}% faster")
                        else:
                            print(f"    📉 FraiseQL is {abs(improvement):.1f}% slower")

    print("\n" + "=" * 80)
    print("🎉 COMPLEX DOMAIN BENCHMARK COMPLETE!")
    print("=" * 80)


if __name__ == "__main__":
    asyncio.run(main())
