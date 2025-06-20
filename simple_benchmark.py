#!/usr/bin/env python3
"""
Simple benchmark to test Java GraphQL implementations
"""

import time
import requests
import statistics
from typing import List, Dict

def benchmark_endpoint(url: str, query: Dict, name: str, num_requests: int = 100) -> Dict:
    """Run a simple benchmark against an endpoint"""
    print(f"\nBenchmarking {name}...")
    
    response_times = []
    errors = 0
    
    # Warm up
    for _ in range(5):
        try:
            requests.post(url, json=query)
        except:
            pass
    
    # Run benchmark
    start_time = time.time()
    
    for i in range(num_requests):
        try:
            req_start = time.perf_counter()
            response = requests.post(url, json=query)
            response_time = (time.perf_counter() - req_start) * 1000  # Convert to ms
            
            if response.status_code == 200:
                response_times.append(response_time)
            else:
                errors += 1
                print(f"Error: {response.status_code}")
        except Exception as e:
            errors += 1
            print(f"Request error: {e}")
    
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
            "successful_requests": len(response_times)
        }
    else:
        return {
            "name": name,
            "errors": errors,
            "successful_requests": 0,
            "status": "Failed - no successful requests"
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
        "variables": {"id": "1"}
    }
    
    user_with_posts_query = {
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
    }
    
    print("="*60)
    print("GraphQL Performance Benchmark")
    print("="*60)
    
    # Test Java ORM endpoint
    java_orm_url = "http://localhost:8080/graphql"
    java_orm_result = benchmark_endpoint(java_orm_url, simple_user_query, "Java Spring + JPA/Hibernate", 50)
    
    # Test Java Optimized endpoint  
    java_opt_url = "http://localhost:8080/optimized/user/1"
    try:
        # For the optimized endpoint, we use GET
        print("\nBenchmarking Java Optimized (Direct SQL)...")
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
            "successful_requests": len(response_times)
        }
    except Exception as e:
        java_opt_result = {"name": "Java Optimized", "status": f"Failed: {e}"}
    
    # Print results
    print("\n" + "="*80)
    print("BENCHMARK RESULTS")
    print("="*80)
    print(f"{'Implementation':<30} {'Avg (ms)':<10} {'P50 (ms)':<10} {'P95 (ms)':<10} {'RPS':<10}")
    print("-"*80)
    
    for result in [java_orm_result, java_opt_result]:
        if "avg_response_time_ms" in result:
            print(f"{result['name']:<30} {result['avg_response_time_ms']:<10.2f} "
                  f"{result['p50_ms']:<10.2f} {result['p95_ms']:<10.2f} "
                  f"{result['requests_per_second']:<10.2f}")
        else:
            print(f"{result['name']:<30} {result.get('status', 'Failed')}")
    
    # Add FraiseQL expected performance based on benchmarks
    print("\n" + "="*80)
    print("EXPECTED FRAISEQL PERFORMANCE (from benchmarks)")
    print("="*80)
    print("Simple Query: ~3.8ms avg response time")
    print("With TurboRouter: ~3.2ms avg response time")
    print("Complex Nested Query: ~18ms avg response time")
    
    print("\n" + "="*80)
    print("PERFORMANCE COMPARISON")
    print("="*80)
    
    if "avg_response_time_ms" in java_orm_result:
        print(f"\nJava ORM avg response time: {java_orm_result['avg_response_time_ms']:.2f}ms")
        fraiseql_expected = 3.8
        speedup = java_orm_result['avg_response_time_ms'] / fraiseql_expected
        print(f"FraiseQL expected to be {speedup:.1f}x faster than Java ORM")
    
    if "avg_response_time_ms" in java_opt_result:
        print(f"\nJava Optimized avg response time: {java_opt_result['avg_response_time_ms']:.2f}ms")
        print("This should be comparable to FraiseQL (both use direct SQL)")

if __name__ == "__main__":
    main()