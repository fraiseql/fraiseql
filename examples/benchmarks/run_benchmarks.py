#!/usr/bin/env python3
"""Comprehensive benchmark runner for FraiseQL vs competitors"""

import argparse
import asyncio
import json
import statistics
import time
from datetime import UTC, datetime
from pathlib import Path
from typing import Any, Dict, List

import aiohttp
import asyncpg


class BenchmarkResult:
    def __init__(self, name: str):
        self.name = name
        self.times: List[float] = []
        self.errors: int = 0

    def add_time(self, duration: float):
        self.times.append(duration)

    def add_error(self):
        self.errors += 1

    def get_stats(self) -> Dict[str, Any]:
        if not self.times:
            return {"error": "No successful requests"}

        return {
            "count": len(self.times),
            "avg": statistics.mean(self.times) * 1000,  # Convert to ms
            "min": min(self.times) * 1000,
            "max": max(self.times) * 1000,
            "median": statistics.median(self.times) * 1000,
            "p95": sorted(self.times)[int(0.95 * len(self.times))] * 1000,
            "p99": sorted(self.times)[int(0.99 * len(self.times))] * 1000,
            "errors": self.errors,
            "rps": len(self.times) / sum(self.times) if sum(self.times) > 0 else 0,
        }


class FraiseQLBenchmark:
    def __init__(self, db_url: str):
        self.db_url = db_url
        self.pool = None

    async def setup(self):
        self.pool = await asyncpg.create_pool(self.db_url, min_size=10, max_size=20)

    async def cleanup(self):
        if self.pool:
            await self.pool.close()

    async def execute_query(self, query: str, variables: Dict | None = None) -> Dict:
        """Simulate FraiseQL query execution (direct SQL)"""
        start_time = time.time()
        try:
            async with self.pool.acquire() as conn:
                # In real FraiseQL, this would be optimized view queries
                if "products" in query.lower():
                    result = await conn.fetch(
                        """
                        SELECT json_agg(row_to_json(t)) as data
                        FROM (
                            SELECT id, name, price, category_name, average_rating
                            FROM product_search
                            WHERE is_active = true
                            LIMIT $1
                        ) t
                    """,
                        variables.get("limit", 100),
                    )
                elif "orders" in query.lower():
                    result = await conn.fetch(
                        """
                        SELECT json_agg(row_to_json(t)) as data
                        FROM (
                            SELECT o.id, o.order_number, o.total_amount, o.status,
                                   json_agg(json_build_object(
                                       'quantity', oi.quantity,
                                       'price', oi.unit_price,
                                       'product', json_build_object('name', p.name)
                                   )) as items
                            FROM orders o
                            JOIN order_items oi ON oi.order_id = o.id
                            JOIN product_variants pv ON oi.variant_id = pv.id
                            JOIN products p ON pv.product_id = p.id
                            WHERE o.customer_id = $1
                            GROUP BY o.id
                            ORDER BY o.created_at DESC
                            LIMIT $2
                        ) t
                    """,
                        variables.get("userId"),
                        variables.get("limit", 50),
                    )
                else:
                    # Simple query
                    result = await conn.fetch("SELECT 1 as result")

            return {"data": result[0]["data"] if result and result[0]["data"] else []}

        except Exception as e:
            raise Exception(f"Query failed: {e}") from e  # noqa: TRY002

        return time.time() - start_time


class HasuraBenchmark:
    def __init__(self, endpoint: str):
        self.endpoint = endpoint
        self.session = None

    async def setup(self):
        self.session = aiohttp.ClientSession()

    async def cleanup(self):
        if self.session:
            await self.session.close()

    async def execute_query(self, query: str, variables: Dict | None = None) -> float:
        """Execute GraphQL query against Hasura"""
        start_time = time.time()
        try:
            payload = {"query": query, "variables": variables or {}}

            async with self.session.post(
                f"{self.endpoint}/v1/graphql",
                json=payload,
                headers={"Content-Type": "application/json"},
            ) as response:
                if response.status != 200:
                    raise Exception(f"HTTP {response.status}")  # noqa: TRY002

                result = await response.json()
                if "errors" in result:
                    raise Exception(f"GraphQL errors: {result['errors']}")  # noqa: TRY002

                return result

        except Exception as e:
            raise Exception(f"Hasura query failed: {e}") from e  # noqa: TRY002

        return time.time() - start_time


class PostGraphileBenchmark:
    def __init__(self, endpoint: str):
        self.endpoint = endpoint
        self.session = None

    async def setup(self):
        self.session = aiohttp.ClientSession()

    async def cleanup(self):
        if self.session:
            await self.session.close()

    async def execute_query(self, query: str, variables: Dict | None = None) -> float:
        """Execute GraphQL query against PostGraphile"""
        start_time = time.time()
        try:
            payload = {"query": query, "variables": variables or {}}

            async with self.session.post(
                f"{self.endpoint}/graphql",
                json=payload,
                headers={"Content-Type": "application/json"},
            ) as response:
                if response.status != 200:
                    raise Exception(f"HTTP {response.status}")  # noqa: TRY002

                result = await response.json()
                if "errors" in result:
                    raise Exception(f"GraphQL errors: {result['errors']}")  # noqa: TRY002

                return result

        except Exception as e:
            raise Exception(f"PostGraphile query failed: {e}") from e  # noqa: TRY002

        return time.time() - start_time


class BenchmarkRunner:
    def __init__(self):
        self.benchmarks = {}

    def add_benchmark(self, name: str, benchmark):
        self.benchmarks[name] = benchmark

    async def run_test(
        self,
        test_name: str,
        query: str,
        variables: Dict | None = None,
        iterations: int = 100,
    ):
        """Run a test against all benchmarks"""
        results = {}

        for name, benchmark in self.benchmarks.items():
            result = BenchmarkResult(f"{name}_{test_name}")

            for i in range(iterations):
                try:
                    duration = await benchmark.execute_query(query, variables)
                    result.add_time(duration)
                except Exception:
                    result.add_error()

                if (i + 1) % 10 == 0:
                    pass

            results[name] = result.get_stats()

        return results

    async def run_all_tests(self, iterations: int = 100):
        """Run comprehensive benchmark suite"""
        # Set up all benchmarks
        for benchmark in self.benchmarks.values():
            await benchmark.setup()

        try:
            all_results = {}

            # Test 1: Simple product query
            simple_query = """
            query GetProducts($limit: Int!) {
                products(limit: $limit) {
                    id
                    name
                    price
                }
            }
            """
            all_results["simple_query"] = await self.run_test(
                "simple_query",
                simple_query,
                {"limit": 100},
                iterations,
            )

            # Test 2: Complex product search
            search_query = """
            query ProductSearch($term: String!, $limit: Int!) {
                productSearch(
                    where: {
                        name: { _ilike: $term }
                        inStock: { _eq: true }
                    }
                    limit: $limit
                ) {
                    id
                    name
                    price
                    categoryName
                    averageRating
                    reviewCount
                    primaryImageUrl
                }
            }
            """
            all_results["search_query"] = await self.run_test(
                "search_query",
                search_query,
                {"term": "%laptop%", "limit": 50},
                iterations,
            )

            # Test 3: Order history with relations
            order_query = """
            query OrderHistory($userId: UUID!, $limit: Int!) {
                orders(
                    where: { customerId: { _eq: $userId } }
                    orderBy: { createdAt: DESC }
                    limit: $limit
                ) {
                    id
                    orderNumber
                    totalAmount
                    status
                    createdAt
                    items {
                        quantity
                        unitPrice
                        product {
                            name
                        }
                    }
                }
            }
            """
            all_results["order_query"] = await self.run_test(
                "order_query",
                order_query,
                {"userId": "d0eebc99-9c0b-4ef8-bb6d-6bb9bd380d11", "limit": 20},
                iterations,
            )

            return all_results

        finally:
            # Clean up all benchmarks
            for benchmark in self.benchmarks.values():
                await benchmark.cleanup()


def print_results(results: Dict[str, Dict[str, Dict]]):
    """Print benchmark results in a formatted table"""
    for test_results in results.values():
        # Sort by average response time
        sorted_results = sorted(
            test_results.items(),
            key=lambda x: x[1].get("avg", float("inf")),
        )

        for _solution, _stats in sorted_results:
            pass


def save_results(results: Dict, filename: str | None = None):
    """Save benchmark results to JSON file"""
    if not filename:
        timestamp = datetime.now(tz=UTC).strftime("%Y%m%d_%H%M%S")
        filename = f"benchmark_results_{timestamp}.json"

    output_path = Path(filename)
    with output_path.open("w") as f:
        json.dump(
            {"timestamp": datetime.now(tz=UTC).isoformat(), "results": results},
            f,
            indent=2,
        )


async def main():
    parser = argparse.ArgumentParser(description="Run FraiseQL benchmarks")
    parser.add_argument(
        "--iterations",
        type=int,
        default=100,
        help="Number of iterations per test",
    )
    parser.add_argument(
        "--fraiseql-db",
        default="postgresql://user:pass@localhost/ecommerce",
        help="FraiseQL database URL",
    )
    parser.add_argument(
        "--hasura-endpoint",
        default="http://localhost:8080",
        help="Hasura endpoint",
    )
    parser.add_argument(
        "--postgraphile-endpoint",
        default="http://localhost:5000",
        help="PostGraphile endpoint",
    )
    parser.add_argument("--output", help="Output file for results")

    args = parser.parse_args()

    # Create benchmark runner
    runner = BenchmarkRunner()

    # Add FraiseQL benchmark
    fraiseql = FraiseQLBenchmark(args.fraiseql_db)
    runner.add_benchmark("FraiseQL", fraiseql)

    # Add competitor benchmarks (if endpoints are available)
    try:
        hasura = HasuraBenchmark(args.hasura_endpoint)
        runner.add_benchmark("Hasura", hasura)
    except Exception:
        pass

    try:
        postgraphile = PostGraphileBenchmark(args.postgraphile_endpoint)
        runner.add_benchmark("PostGraphile", postgraphile)
    except Exception:
        pass

    # Run all tests
    results = await runner.run_all_tests(args.iterations)

    # Print and save results
    print_results(results)
    save_results(results, args.output)


if __name__ == "__main__":
    asyncio.run(main())
