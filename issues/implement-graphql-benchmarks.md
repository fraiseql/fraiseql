# Implement Standardized GraphQL Benchmarks for FraiseQL

## Summary

Implement industry-standard GraphQL benchmarks using the Chinook database and common test queries to objectively measure FraiseQL's performance against other GraphQL frameworks. This will provide quantifiable metrics for performance claims and help identify optimization opportunities.

## Background

Currently, FraiseQL lacks standardized benchmarks that would allow direct performance comparison with other GraphQL solutions like Hasura, PostGraphile, and Strawberry. The GraphQL community has established common benchmark patterns using:

1. **Chinook Database**: A sample database representing a digital media store
2. **Standard Query Patterns**: Common GraphQL queries that test different performance aspects
3. **Established Metrics**: RPS, latency percentiles, memory usage

## Proposed Implementation

### 1. Database Setup

#### Chinook Schema for PostgreSQL

```sql
-- Core tables from Chinook database
CREATE TABLE artists (
    artist_id SERIAL PRIMARY KEY,
    name VARCHAR(120) NOT NULL
);

CREATE TABLE albums (
    album_id SERIAL PRIMARY KEY,
    title VARCHAR(160) NOT NULL,
    artist_id INTEGER NOT NULL REFERENCES artists(artist_id)
);

CREATE TABLE tracks (
    track_id SERIAL PRIMARY KEY,
    name VARCHAR(200) NOT NULL,
    album_id INTEGER REFERENCES albums(album_id),
    media_type_id INTEGER NOT NULL,
    genre_id INTEGER,
    composer VARCHAR(220),
    milliseconds INTEGER NOT NULL,
    bytes INTEGER,
    unit_price NUMERIC(10,2) NOT NULL
);

CREATE TABLE customers (
    customer_id SERIAL PRIMARY KEY,
    first_name VARCHAR(40) NOT NULL,
    last_name VARCHAR(20) NOT NULL,
    company VARCHAR(80),
    email VARCHAR(60) NOT NULL,
    phone VARCHAR(24),
    country VARCHAR(40)
);

CREATE TABLE invoices (
    invoice_id SERIAL PRIMARY KEY,
    customer_id INTEGER NOT NULL REFERENCES customers(customer_id),
    invoice_date TIMESTAMP NOT NULL,
    total NUMERIC(10,2) NOT NULL
);

CREATE TABLE invoice_items (
    invoice_line_id SERIAL PRIMARY KEY,
    invoice_id INTEGER NOT NULL REFERENCES invoices(invoice_id),
    track_id INTEGER NOT NULL REFERENCES tracks(track_id),
    unit_price NUMERIC(10,2) NOT NULL,
    quantity INTEGER NOT NULL
);
```

#### FraiseQL Views

```sql
-- JSONB views for FraiseQL
CREATE VIEW v_artists AS
SELECT jsonb_build_object(
    'id', artist_id,
    'name', name
) as data
FROM artists;

CREATE VIEW v_albums AS
SELECT jsonb_build_object(
    'id', album_id,
    'title', title,
    'artistId', artist_id,
    'artist', (
        SELECT jsonb_build_object('id', a.artist_id, 'name', a.name)
        FROM artists a
        WHERE a.artist_id = albums.artist_id
    )
) as data
FROM albums;

CREATE VIEW v_tracks AS
SELECT jsonb_build_object(
    'id', track_id,
    'name', name,
    'albumId', album_id,
    'milliseconds', milliseconds,
    'unitPrice', unit_price,
    'album', (
        SELECT jsonb_build_object(
            'id', a.album_id,
            'title', a.title,
            'artist', (
                SELECT jsonb_build_object('id', ar.artist_id, 'name', ar.name)
                FROM artists ar
                WHERE ar.artist_id = a.artist_id
            )
        )
        FROM albums a
        WHERE a.album_id = tracks.album_id
    )
) as data
FROM tracks;
```

### 2. FraiseQL Type Definitions

```python
# benchmarks/chinook/types.py
from uuid import UUID
from decimal import Decimal
from datetime import datetime
from typing import Optional, List
import fraiseql
from fraiseql import fraise_field

@fraiseql.type
class Artist:
    id: int
    name: str
    albums: Optional[List["Album"]] = fraise_field(
        description="Albums by this artist"
    )

@fraiseql.type
class Album:
    id: int
    title: str
    artist_id: int = fraise_field(purpose="Foreign key to artist")
    artist: Optional[Artist] = fraise_field(
        description="The artist who created this album"
    )
    tracks: Optional[List["Track"]] = fraise_field(
        description="Tracks in this album"
    )

@fraiseql.type
class Track:
    id: int
    name: str
    album_id: Optional[int]
    milliseconds: int
    unit_price: Decimal
    album: Optional[Album] = fraise_field(
        description="Album containing this track"
    )

@fraiseql.type
class Customer:
    id: int
    first_name: str
    last_name: str
    email: str
    country: Optional[str]
    invoices: Optional[List["Invoice"]] = fraise_field(
        description="Customer's invoices"
    )

@fraiseql.type
class Invoice:
    id: int
    customer_id: int
    invoice_date: datetime
    total: Decimal
    customer: Optional[Customer]
    items: Optional[List["InvoiceItem"]]
```

### 3. Benchmark Query Suite

```python
# benchmarks/chinook/queries.py

# Standard benchmark queries from hasura/graphql-bench
BENCHMARK_QUERIES = {
    "simple_query": """
        query SimpleArtists {
            artists(limit: 10) {
                id
                name
            }
        }
    """,

    "nested_query": """
        query ArtistsWithAlbums {
            artists(limit: 5) {
                id
                name
                albums {
                    id
                    title
                }
            }
        }
    """,

    "deep_nesting": """
        query DeepNesting {
            artists(limit: 3) {
                id
                name
                albums(limit: 5) {
                    id
                    title
                    tracks(limit: 10) {
                        id
                        name
                        milliseconds
                        unitPrice
                    }
                }
            }
        }
    """,

    "filtering": """
        query FilteredTracks($minPrice: Decimal!, $country: String!) {
            tracks(where: { unitPrice: { gte: $minPrice } }, limit: 100) {
                id
                name
                unitPrice
            }
            customers(where: { country: { eq: $country } }) {
                id
                firstName
                lastName
                email
            }
        }
    """,

    "aggregation": """
        query SalesAnalytics($startDate: DateTime!, $endDate: DateTime!) {
            invoiceStats: invoices(
                where: {
                    invoiceDate: {
                        gte: $startDate,
                        lte: $endDate
                    }
                }
            ) {
                totalCount
                totalRevenue
                averageOrderValue
            }
        }
    """,

    "complex_join": """
        query CustomerPurchaseHistory($customerId: Int!) {
            customer(id: $customerId) {
                id
                firstName
                lastName
                invoices {
                    id
                    invoiceDate
                    total
                    items {
                        unitPrice
                        quantity
                        track {
                            name
                            album {
                                title
                                artist {
                                    name
                                }
                            }
                        }
                    }
                }
            }
        }
    """
}
```

### 4. Benchmark Framework

```python
# benchmarks/framework.py
import asyncio
import time
import statistics
from typing import Dict, List, Any
from dataclasses import dataclass
import aiohttp
import psutil
import json

@dataclass
class BenchmarkResult:
    query_name: str
    requests_per_second: float
    latency_p50: float
    latency_p95: float
    latency_p99: float
    errors: int
    memory_usage_mb: float

class GraphQLBenchmark:
    def __init__(self, endpoint: str, warmup_requests: int = 100):
        self.endpoint = endpoint
        self.warmup_requests = warmup_requests

    async def run_benchmark(
        self,
        query: str,
        variables: Dict[str, Any],
        duration_seconds: int = 60,
        concurrent_requests: int = 10
    ) -> BenchmarkResult:
        """Run a benchmark for a specific query."""

        # Warmup phase
        await self._warmup(query, variables)

        # Benchmark phase
        start_time = time.time()
        end_time = start_time + duration_seconds

        latencies = []
        errors = 0
        requests = 0

        # Track memory
        process = psutil.Process()
        initial_memory = process.memory_info().rss / 1024 / 1024

        async def worker():
            nonlocal errors, requests

            async with aiohttp.ClientSession() as session:
                while time.time() < end_time:
                    request_start = time.time()

                    try:
                        async with session.post(
                            self.endpoint,
                            json={"query": query, "variables": variables},
                            timeout=aiohttp.ClientTimeout(total=30)
                        ) as response:
                            await response.json()

                            if response.status != 200:
                                errors += 1
                            else:
                                latency = (time.time() - request_start) * 1000
                                latencies.append(latency)
                                requests += 1

                    except Exception:
                        errors += 1

        # Run concurrent workers
        workers = [asyncio.create_task(worker()) for _ in range(concurrent_requests)]
        await asyncio.gather(*workers)

        # Calculate results
        duration = time.time() - start_time
        peak_memory = process.memory_info().rss / 1024 / 1024

        return BenchmarkResult(
            query_name=query,
            requests_per_second=requests / duration,
            latency_p50=statistics.median(latencies),
            latency_p95=statistics.quantiles(latencies, n=20)[18],  # 95th percentile
            latency_p99=statistics.quantiles(latencies, n=100)[98],  # 99th percentile
            errors=errors,
            memory_usage_mb=peak_memory - initial_memory
        )

    async def _warmup(self, query: str, variables: Dict[str, Any]):
        """Warmup the server and caches."""
        async with aiohttp.ClientSession() as session:
            tasks = []
            for _ in range(self.warmup_requests):
                task = session.post(
                    self.endpoint,
                    json={"query": query, "variables": variables}
                )
                tasks.append(task)

            await asyncio.gather(*tasks, return_exceptions=True)
```

### 5. Benchmark Runner

```python
# benchmarks/run_benchmarks.py
import asyncio
import json
from datetime import datetime
from typing import Dict, List

from framework import GraphQLBenchmark, BenchmarkResult
from chinook.queries import BENCHMARK_QUERIES

class FraiseQLBenchmarkSuite:
    def __init__(self, fraiseql_url: str):
        self.benchmark = GraphQLBenchmark(fraiseql_url)
        self.results: Dict[str, BenchmarkResult] = {}

    async def run_all_benchmarks(self):
        """Run all standard benchmarks."""

        # Test variables
        variables_map = {
            "filtering": {
                "minPrice": 0.99,
                "country": "USA"
            },
            "aggregation": {
                "startDate": "2020-01-01T00:00:00Z",
                "endDate": "2023-12-31T23:59:59Z"
            },
            "complex_join": {
                "customerId": 1
            }
        }

        for query_name, query in BENCHMARK_QUERIES.items():
            print(f"Running benchmark: {query_name}")

            variables = variables_map.get(query_name, {})

            result = await self.benchmark.run_benchmark(
                query=query,
                variables=variables,
                duration_seconds=30,  # Shorter for testing
                concurrent_requests=10
            )

            self.results[query_name] = result

            # Print immediate results
            print(f"  RPS: {result.requests_per_second:.2f}")
            print(f"  P50 Latency: {result.latency_p50:.2f}ms")
            print(f"  P95 Latency: {result.latency_p95:.2f}ms")
            print(f"  P99 Latency: {result.latency_p99:.2f}ms")
            print(f"  Errors: {result.errors}")
            print()

    def generate_report(self) -> str:
        """Generate a markdown report of results."""

        report = f"""# FraiseQL Benchmark Results

Generated: {datetime.now().isoformat()}

## Summary

| Query | RPS | P50 (ms) | P95 (ms) | P99 (ms) | Errors | Memory (MB) |
|-------|-----|----------|----------|----------|--------|-------------|
"""

        for query_name, result in self.results.items():
            report += f"| {query_name} | {result.requests_per_second:.2f} | "
            report += f"{result.latency_p50:.2f} | {result.latency_p95:.2f} | "
            report += f"{result.latency_p99:.2f} | {result.errors} | "
            report += f"{result.memory_usage_mb:.2f} |\n"

        return report

    def export_json(self, filename: str):
        """Export results as JSON for comparison."""

        data = {
            "framework": "FraiseQL",
            "timestamp": datetime.now().isoformat(),
            "results": {
                name: {
                    "rps": result.requests_per_second,
                    "latency_p50": result.latency_p50,
                    "latency_p95": result.latency_p95,
                    "latency_p99": result.latency_p99,
                    "errors": result.errors,
                    "memory_mb": result.memory_usage_mb
                }
                for name, result in self.results.items()
            }
        }

        with open(filename, 'w') as f:
            json.dump(data, f, indent=2)

# CLI runner
if __name__ == "__main__":
    import sys

    if len(sys.argv) != 2:
        print("Usage: python run_benchmarks.py <fraiseql_endpoint>")
        sys.exit(1)

    async def main():
        suite = FraiseQLBenchmarkSuite(sys.argv[1])
        await suite.run_all_benchmarks()

        # Save results
        report = suite.generate_report()
        with open("benchmark_results.md", "w") as f:
            f.write(report)

        suite.export_json("benchmark_results.json")

        print("\nResults saved to benchmark_results.md and benchmark_results.json")

    asyncio.run(main())
```

### 6. Comparison Framework

```python
# benchmarks/compare.py
import json
import matplotlib.pyplot as plt
from typing import Dict, List

class BenchmarkComparison:
    def __init__(self):
        self.results: Dict[str, Dict] = {}

    def load_results(self, framework: str, filename: str):
        """Load benchmark results for a framework."""
        with open(filename) as f:
            data = json.load(f)
            self.results[framework] = data['results']

    def generate_comparison_chart(self, metric: str = "rps"):
        """Generate comparison charts."""
        frameworks = list(self.results.keys())
        queries = list(next(iter(self.results.values())).keys())

        # Create grouped bar chart
        fig, ax = plt.subplots(figsize=(12, 6))

        x = range(len(queries))
        width = 0.8 / len(frameworks)

        for i, framework in enumerate(frameworks):
            values = [self.results[framework][q][metric] for q in queries]
            offset = (i - len(frameworks)/2 + 0.5) * width
            ax.bar([xi + offset for xi in x], values, width, label=framework)

        ax.set_xlabel('Query')
        ax.set_ylabel(metric.upper())
        ax.set_title(f'GraphQL Framework Comparison - {metric.upper()}')
        ax.set_xticks(x)
        ax.set_xticklabels(queries, rotation=45, ha='right')
        ax.legend()

        plt.tight_layout()
        plt.savefig(f'comparison_{metric}.png')

    def generate_report(self) -> str:
        """Generate a comparison report."""
        report = "# GraphQL Framework Benchmark Comparison\n\n"

        # RPS comparison
        report += "## Requests Per Second (Higher is Better)\n\n"
        report += "| Query | " + " | ".join(self.results.keys()) + " |\n"
        report += "|-------|" + "|".join(["-------"] * len(self.results)) + "|\n"

        queries = list(next(iter(self.results.values())).keys())
        for query in queries:
            row = f"| {query} |"
            for framework in self.results:
                rps = self.results[framework][query]['rps']
                row += f" {rps:.2f} |"
            report += row + "\n"

        return report
```

### 7. Docker Compose for Testing

```yaml
# benchmarks/docker-compose.yml
version: '3.8'

services:
  postgres:
    image: postgres:15
    environment:
      POSTGRES_DB: chinook
      POSTGRES_USER: benchmark
      POSTGRES_PASSWORD: benchmark
    volumes:
      - ./chinook/schema.sql:/docker-entrypoint-initdb.d/01-schema.sql
      - ./chinook/data.sql:/docker-entrypoint-initdb.d/02-data.sql
      - ./chinook/views.sql:/docker-entrypoint-initdb.d/03-views.sql
    ports:
      - "5432:5432"

  fraiseql:
    build: ..
    environment:
      DATABASE_URL: postgresql://benchmark:benchmark@postgres/chinook
      FRAISEQL_MODE: production
    depends_on:
      - postgres
    ports:
      - "8000:8000"

  hasura:
    image: hasura/graphql-engine:latest
    environment:
      HASURA_GRAPHQL_DATABASE_URL: postgresql://benchmark:benchmark@postgres/chinook
      HASURA_GRAPHQL_ENABLE_CONSOLE: "false"
    depends_on:
      - postgres
    ports:
      - "8001:8080"

  postgraphile:
    image: graphile/postgraphile:latest
    command:
      - --connection
      - postgresql://benchmark:benchmark@postgres/chinook
      - --schema
      - public
      - --disable-graphiql
    depends_on:
      - postgres
    ports:
      - "8002:5000"
```

## Implementation Plan

### Phase 1: Basic Setup (Week 1)
- [ ] Create Chinook database schema and seed data
- [ ] Implement FraiseQL types for Chinook
- [ ] Create JSONB views
- [ ] Basic query execution tests

### Phase 2: Benchmark Framework (Week 2)
- [ ] Implement benchmark runner
- [ ] Add standard queries from graphql-bench
- [ ] Create result collection and reporting
- [ ] Docker compose for reproducible testing

### Phase 3: Comparison Tools (Week 3)
- [ ] Setup comparison frameworks (Hasura, PostGraphile)
- [ ] Create unified benchmark runner
- [ ] Generate comparison charts
- [ ] Document methodology

### Phase 4: CI Integration (Week 4)
- [ ] GitHub Actions workflow for benchmarks
- [ ] Performance regression detection
- [ ] Automated reports on PRs
- [ ] Public dashboard

## Expected Outcomes

1. **Objective Performance Metrics**
   - RPS for standard queries
   - Latency percentiles (P50, P95, P99)
   - Memory usage patterns
   - Scalability characteristics

2. **Framework Comparisons**
   - Side-by-side performance data
   - Strengths and weaknesses analysis
   - Use case recommendations

3. **Performance Tracking**
   - Regression detection
   - Optimization validation
   - Version-to-version improvements

## Success Criteria

- Benchmarks run reliably and reproducibly
- Results align with theoretical performance expectations
- Clear documentation of methodology
- Easy to run for contributors
- Automated regression detection

## Future Enhancements

1. **Extended Query Patterns**
   - Mutations and complex writes
   - Subscription simulation
   - Batch query handling

2. **Load Patterns**
   - Burst traffic simulation
   - Gradual load increase
   - Mixed workload scenarios

3. **Database Variations**
   - Different PostgreSQL versions
   - Various connection pool sizes
   - Read replica configurations

4. **TurboRouter Testing**
   - Before/after TurboRouter comparison
   - Cache hit rate analysis
   - Overhead reduction validation

This standardized benchmark suite will provide objective performance data and enable continuous performance monitoring as FraiseQL evolves.
