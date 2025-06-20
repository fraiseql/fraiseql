#!/usr/bin/env python3
"""
FraiseQL vs Java Performance Benchmark Runner

This script runs comprehensive benchmarks comparing:
1. FraiseQL (Python + PostgreSQL views/functions)
2. Spring Boot + JPA/Hibernate (traditional ORM)
3. Spring Boot + Direct SQL (optimized like FraiseQL)
"""

import asyncio
import time
import statistics
import json
import aiohttp
import psycopg2
from typing import Dict, List, Tuple
from dataclasses import dataclass
from concurrent.futures import ThreadPoolExecutor
import matplotlib.pyplot as plt
import numpy as np

@dataclass
class BenchmarkResult:
    name: str
    avg_response_time_ms: float
    p50_ms: float
    p95_ms: float
    p99_ms: float
    requests_per_second: float
    errors: int
    memory_usage_mb: float

class BenchmarkRunner:
    def __init__(self):
        # When running inside Docker, use service names
        import os
        if os.environ.get('DOCKER_ENV'):
            self.fraiseql_url = "http://fraiseql:8000/graphql"
            self.java_orm_url = "http://java-benchmark:8080/graphql"
            self.java_optimized_url = "http://java-benchmark:8080/optimized/graphql"
        else:
            self.fraiseql_url = "http://localhost:8000/graphql"
            self.java_orm_url = "http://localhost:8080/graphql"
            self.java_optimized_url = "http://localhost:8080/optimized/graphql"
        
        # Test queries
        self.queries = {
            "simple_user": {
                "query": "query GetUser($id: ID!) { user(id: $id) { id name email } }",
                "variables": {"id": "1"}
            },
            "user_with_posts": {
                "query": """
                    query GetUserWithPosts($id: ID!) {
                        user(id: $id) {
                            id
                            name
                            email
                            posts {
                                id
                                title
                                content
                            }
                        }
                    }
                """,
                "variables": {"id": "1"}
            },
            "nested_query": {
                "query": """
                    query GetPostWithComments($id: ID!) {
                        post(id: $id) {
                            id
                            title
                            content
                            author {
                                id
                                name
                                email
                            }
                            comments {
                                id
                                content
                                author {
                                    id
                                    name
                                }
                            }
                        }
                    }
                """,
                "variables": {"id": "1"}
            },
            "list_with_aggregation": {
                "query": """
                    query GetAllUsers {
                        users {
                            id
                            name
                            email
                            posts {
                                id
                                title
                            }
                        }
                    }
                """,
                "variables": {}
            }
        }
    
    async def run_single_request(self, session: aiohttp.ClientSession, url: str, query: dict) -> float:
        """Run a single GraphQL request and return response time in ms"""
        start = time.perf_counter()
        try:
            async with session.post(url, json=query) as response:
                await response.json()
                if response.status != 200:
                    raise Exception(f"HTTP {response.status}")
        except Exception as e:
            print(f"Error: {e}")
            raise
        
        return (time.perf_counter() - start) * 1000
    
    async def benchmark_endpoint(self, url: str, query: dict, name: str, 
                               num_requests: int = 1000, concurrency: int = 10) -> BenchmarkResult:
        """Benchmark a single endpoint with a specific query"""
        print(f"\nBenchmarking {name}...")
        
        response_times = []
        errors = 0
        
        # Warm up
        async with aiohttp.ClientSession() as session:
            for _ in range(10):
                try:
                    await self.run_single_request(session, url, query)
                except:
                    pass
        
        # Run benchmark
        start_time = time.time()
        
        async with aiohttp.ClientSession() as session:
            for batch in range(0, num_requests, concurrency):
                tasks = []
                for _ in range(min(concurrency, num_requests - batch)):
                    tasks.append(self.run_single_request(session, url, query))
                
                results = await asyncio.gather(*tasks, return_exceptions=True)
                
                for result in results:
                    if isinstance(result, Exception):
                        errors += 1
                    else:
                        response_times.append(result)
        
        total_time = time.time() - start_time
        
        # Calculate metrics
        response_times.sort()
        
        return BenchmarkResult(
            name=name,
            avg_response_time_ms=statistics.mean(response_times),
            p50_ms=response_times[len(response_times) // 2],
            p95_ms=response_times[int(len(response_times) * 0.95)],
            p99_ms=response_times[int(len(response_times) * 0.99)],
            requests_per_second=len(response_times) / total_time,
            errors=errors,
            memory_usage_mb=0  # Would need to implement memory monitoring
        )
    
    async def run_benchmarks(self):
        """Run all benchmarks and compare results"""
        results = {}
        
        for query_name, query_data in self.queries.items():
            print(f"\n{'='*60}")
            print(f"Testing: {query_name}")
            print(f"{'='*60}")
            
            results[query_name] = []
            
            # Test FraiseQL
            result = await self.benchmark_endpoint(
                self.fraiseql_url,
                query_data,
                "FraiseQL",
                num_requests=500,
                concurrency=20
            )
            results[query_name].append(result)
            
            # Test Java ORM
            result = await self.benchmark_endpoint(
                self.java_orm_url,
                query_data,
                "Java Spring + JPA",
                num_requests=500,
                concurrency=20
            )
            results[query_name].append(result)
            
            # Test Java Optimized
            result = await self.benchmark_endpoint(
                self.java_optimized_url,
                query_data,
                "Java Optimized (Direct SQL)",
                num_requests=500,
                concurrency=20
            )
            results[query_name].append(result)
        
        return results
    
    def print_results(self, results: Dict[str, List[BenchmarkResult]]):
        """Print benchmark results in a formatted table"""
        print("\n" + "="*100)
        print("BENCHMARK RESULTS SUMMARY")
        print("="*100)
        
        for query_name, query_results in results.items():
            print(f"\n{query_name.upper().replace('_', ' ')}:")
            print("-" * 80)
            print(f"{'Implementation':<30} {'Avg (ms)':<10} {'P50 (ms)':<10} {'P95 (ms)':<10} {'P99 (ms)':<10} {'RPS':<10}")
            print("-" * 80)
            
            for result in query_results:
                print(f"{result.name:<30} {result.avg_response_time_ms:<10.2f} "
                      f"{result.p50_ms:<10.2f} {result.p95_ms:<10.2f} "
                      f"{result.p99_ms:<10.2f} {result.requests_per_second:<10.2f}")
        
        # Performance comparison
        print("\n" + "="*100)
        print("PERFORMANCE COMPARISON (vs Java ORM)")
        print("="*100)
        
        for query_name, query_results in results.items():
            print(f"\n{query_name.upper().replace('_', ' ')}:")
            
            java_orm_time = next(r.avg_response_time_ms for r in query_results if "JPA" in r.name)
            
            for result in query_results:
                if "JPA" not in result.name:
                    speedup = java_orm_time / result.avg_response_time_ms
                    percent_faster = ((java_orm_time - result.avg_response_time_ms) / java_orm_time) * 100
                    print(f"  {result.name}: {speedup:.2f}x faster ({percent_faster:.1f}% improvement)")
    
    def create_visualization(self, results: Dict[str, List[BenchmarkResult]]):
        """Create bar charts comparing performance"""
        query_names = list(results.keys())
        implementations = ["FraiseQL", "Java Spring + JPA", "Java Optimized (Direct SQL)"]
        
        fig, axes = plt.subplots(2, 2, figsize=(12, 10))
        axes = axes.ravel()
        
        for idx, (query_name, query_results) in enumerate(results.items()):
            ax = axes[idx]
            
            response_times = [r.avg_response_time_ms for r in query_results]
            x = np.arange(len(implementations))
            
            bars = ax.bar(x, response_times, color=['#2ecc71', '#e74c3c', '#3498db'])
            ax.set_ylabel('Response Time (ms)')
            ax.set_title(query_name.replace('_', ' ').title())
            ax.set_xticks(x)
            ax.set_xticklabels(implementations, rotation=45, ha='right')
            
            # Add value labels on bars
            for bar in bars:
                height = bar.get_height()
                ax.annotate(f'{height:.1f}',
                           xy=(bar.get_x() + bar.get_width() / 2, height),
                           xytext=(0, 3),
                           textcoords="offset points",
                           ha='center', va='bottom')
        
        plt.tight_layout()
        plt.savefig('benchmark_results.png', dpi=300, bbox_inches='tight')
        print("\nVisualization saved as 'benchmark_results.png'")

async def main():
    runner = BenchmarkRunner()
    
    print("Starting FraiseQL vs Java Performance Benchmark...")
    print("Make sure all services are running:")
    print("- FraiseQL on port 8000")
    print("- Java Spring Boot on port 8080")
    print("- PostgreSQL with test data")
    
    input("\nPress Enter to start benchmark...")
    
    results = await runner.run_benchmarks()
    runner.print_results(results)
    runner.create_visualization(results)

if __name__ == "__main__":
    asyncio.run(main())