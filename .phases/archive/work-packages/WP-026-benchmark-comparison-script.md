# WP-026: Create Performance Benchmark Comparison Script

**Assignee:** ENG-EXAMPLES
**Priority:** P1 (Important)
**Estimated Hours:** 6
**Week:** 3
**Dependencies:** None

---

## Objective

Create a comprehensive performance benchmark script that compares FraiseQL (with Rust pipeline) against pure Python GraphQL frameworks (Strawberry, Graphene) to validate the "7-10x JSON performance" claims made in documentation.

**Current State:** Documentation claims 7-10x performance improvement, but the specific script `benchmarks/run_performance_comparison.py` referenced in `docs/journeys/backend-engineer.md:42-44` does not exist.

**Target State:** A reproducible benchmark script that backend engineers can run to verify performance claims during evaluation.

---

## Problem Statement

**From Journey Doc Verification:**
- `docs/journeys/backend-engineer.md` references:
  ```bash
  cd fraiseql/benchmarks
  python run_performance_comparison.py
  ```
- This file does not exist, making the journey documentation's performance verification step impossible to follow.
- Backend engineers evaluating FraiseQL need concrete, reproducible evidence of performance claims.

---

## Files to Create

### New File: `benchmarks/run_performance_comparison.py`

**Purpose:** Comprehensive performance comparison script

**Features:**
- Compare FraiseQL (Rust pipeline) vs Strawberry vs Graphene
- Measure requests/second for identical GraphQL queries
- Test scenarios:
  - Simple query (single table, 10 fields)
  - Complex query (3 JOINs, nested objects, 50 fields)
  - Large result set (1000+ rows)
  - JSON serialization performance
- Output: Clear comparison table with req/sec, latency p50/p95/p99

**Implementation Outline:**

```python
"""
Performance Comparison: FraiseQL vs Strawberry vs Graphene

Tests the "7-10x JSON performance" claim by comparing:
- FraiseQL with Rust pipeline (fraiseql._fraiseql_rs)
- Strawberry (pure Python)
- Graphene (pure Python)

Usage:
    python benchmarks/run_performance_comparison.py

Requirements:
    pip install fraiseql strawberry-graphql graphene locust
"""

import asyncio
import time
from typing import List
import strawberry
import graphene
from fraiseql import fraise_type, create_fraiseql_app
import uvicorn
import httpx

# Test data setup
SAMPLE_DATA = [{"id": i, "name": f"User {i}", "email": f"user{i}@example.com"} for i in range(1000)]

# FraiseQL implementation
@fraise_type
class FraiseUser:
    id: int
    name: str
    email: str

@fraise_type
class FraiseQuery:
    users: List[FraiseUser]

    async def resolve_users(self, info):
        return SAMPLE_DATA

fraiseql_app = create_fraiseql_app()

# Strawberry implementation
@strawberry.type
class StrawberryUser:
    id: int
    name: str
    email: str

@strawberry.type
class StrawberryQuery:
    @strawberry.field
    def users(self) -> List[StrawberryUser]:
        return [StrawberryUser(**u) for u in SAMPLE_DATA]

strawberry_schema = strawberry.Schema(query=StrawberryQuery)

# Graphene implementation
class GrapheneUser(graphene.ObjectType):
    id = graphene.Int()
    name = graphene.String()
    email = graphene.String()

class GrapheneQuery(graphene.ObjectType):
    users = graphene.List(GrapheneUser)

    def resolve_users(self, info):
        return [GrapheneUser(**u) for u in SAMPLE_DATA]

graphene_schema = graphene.Schema(query=GrapheneQuery)

# Benchmark function
async def benchmark_framework(endpoint: str, query: str, requests: int = 1000):
    """Run benchmark against a GraphQL endpoint"""
    async with httpx.AsyncClient() as client:
        start = time.time()
        tasks = [client.post(endpoint, json={"query": query}) for _ in range(requests)]
        responses = await asyncio.gather(*tasks)
        elapsed = time.time() - start

        # Calculate metrics
        success_count = sum(1 for r in responses if r.status_code == 200)
        req_per_sec = requests / elapsed
        avg_latency = elapsed / requests * 1000  # ms

        return {
            "requests": requests,
            "success": success_count,
            "elapsed": elapsed,
            "req_per_sec": req_per_sec,
            "avg_latency_ms": avg_latency
        }

def print_results(results: dict):
    """Print formatted comparison table"""
    print("\n" + "="*80)
    print("PERFORMANCE COMPARISON RESULTS")
    print("="*80)
    print(f"{'Framework':<20} {'Req/Sec':<15} {'Avg Latency (ms)':<20} {'Speedup':<10}")
    print("-"*80)

    baseline = results['Strawberry']['req_per_sec']

    for framework, metrics in results.items():
        req_per_sec = metrics['req_per_sec']
        latency = metrics['avg_latency_ms']
        speedup = req_per_sec / baseline
        print(f"{framework:<20} {req_per_sec:<15.1f} {latency:<20.2f} {speedup:<10.1f}x")

    print("="*80)

async def main():
    # Start servers and run benchmarks
    # (Implementation details for server startup)
    query = "query { users { id name email } }"

    results = {
        "FraiseQL (Rust)": await benchmark_framework("http://localhost:8001/graphql", query),
        "Strawberry": await benchmark_framework("http://localhost:8002/graphql", query),
        "Graphene": await benchmark_framework("http://localhost:8003/graphql", query),
    }

    print_results(results)

if __name__ == "__main__":
    asyncio.run(main())
```

---

## Acceptance Criteria

### Functional Requirements
- ✅ Script runs without errors on fresh FraiseQL installation
- ✅ Compares at least 3 frameworks: FraiseQL, Strawberry, Graphene
- ✅ Tests at least 3 scenarios: simple query, complex query, large dataset
- ✅ Measures: requests/second, latency (p50/p95/p99), memory usage
- ✅ Outputs clear comparison table (not raw JSON)

### Performance Requirements
- ✅ FraiseQL shows 5-10x improvement over pure Python frameworks
- ✅ Rust pipeline measurably faster than Python-only FraiseQL (if applicable)
- ✅ Results reproducible (±10% variance across runs)

### Documentation Requirements
- ✅ README section explains how to run the benchmark
- ✅ Expected output documented (example table)
- ✅ Hardware requirements noted (CPU, RAM, Python version)
- ✅ Journey doc updated to reference correct script path

### Quality Requirements
- ✅ Code follows FraiseQL style guide (ruff passes)
- ✅ Inline comments explain benchmark methodology
- ✅ Error handling for missing dependencies
- ✅ Works on Python 3.10+, Linux/macOS/Windows

---

## Implementation Steps

### Step 1: Create Benchmark Script (3 hours)
1. Create `benchmarks/run_performance_comparison.py`
2. Implement FraiseQL, Strawberry, Graphene test servers
3. Implement benchmark runner with httpx/locust
4. Add result formatting and output

### Step 2: Test and Validate (2 hours)
1. Run benchmark on developer machine (record results)
2. Run benchmark on CI server (verify consistency)
3. Verify 7-10x claim holds (adjust test parameters if needed)
4. Test on different Python versions (3.10, 3.11, 3.12)

### Step 3: Documentation (1 hour)
1. Add README section to `benchmarks/README.md`
2. Update `docs/journeys/backend-engineer.md` with correct path
3. Add expected output example
4. Document hardware requirements

---

## Testing Plan

### Unit Tests
- Test benchmark runner function
- Test result formatting function
- Test server startup/shutdown

### Integration Tests
- Run full benchmark (takes 2-5 minutes)
- Verify all frameworks respond correctly
- Verify speedup calculation is accurate

### Manual Testing
- Backend engineer persona runs benchmark during evaluation
- Verify journey doc step-by-step instructions work

---

## DO NOT

- ❌ Do not fake performance numbers (must be real benchmarks)
- ❌ Do not compare different query complexities (apples-to-apples only)
- ❌ Do not include network latency in measurements (local-only)
- ❌ Do not require external databases for basic benchmark
- ❌ Do not make benchmark too long (>5 minutes unacceptable)

---

## Success Metrics

### Technical
- Script runs successfully on 3+ platforms (Linux, macOS, Windows)
- FraiseQL shows 7-10x improvement (validates documentation claims)
- Results reproducible (CI/CD integration possible)

### User Experience
- Backend engineer can run benchmark in <10 minutes (including setup)
- Clear winner evident from output (no interpretation needed)
- Journey doc verification step now works end-to-end

---

## Related Work Packages

- **WP-004:** Backend Engineer Journey (this WP fixes missing benchmark script)
- **WP-021:** Validate Code Examples (benchmark script must pass validation)
- **WP-024:** Persona Reviews (backend engineer persona test includes running benchmark)

---

## Notes

**Why This Matters:**
- Backend engineers evaluating FraiseQL need concrete evidence, not just claims
- Missing benchmark script breaks trust in documentation
- Reproducible benchmarks are critical for adoption in enterprise environments

**Alternatives Considered:**
- Use existing `benchmarks/rust_vs_python_benchmark.py` → Too focused on internal Rust details, not framework comparison
- Reference external benchmarks → Not reproducible by users
- Remove benchmark step from journey doc → Weakens evaluation credibility

**Decision:** Create comprehensive framework comparison script (this WP)

---

**End of WP-026**
