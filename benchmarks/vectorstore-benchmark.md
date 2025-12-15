# FraiseQL Vector Store Integration Benchmarks

Comprehensive performance analysis and comparison of LangChain and LlamaIndex integrations with FraiseQL.

## Overview

This benchmark suite evaluates the performance characteristics of FraiseQL's vector store integrations across multiple dimensions:

- **Insertion Performance**: Document storage throughput with various batch sizes
- **Search Performance**: Vector similarity search query throughput
- **Concurrent Load**: Multi-user concurrent operations simulation
- **Memory Usage**: Memory efficiency and profiling during operations
- **Framework Comparison**: Direct performance comparison between LangChain and LlamaIndex

## Prerequisites

### Database Setup
```bash
# Ensure PostgreSQL with pgvector extension is available
createdb fraiseql_benchmark
psql fraiseql_benchmark -c "CREATE EXTENSION IF NOT EXISTS vector;"
```

### Python Dependencies
```bash
# Install FraiseQL with optional dependencies
pip install fraiseql[langchain,llamaindex]

# Or install separately
pip install langchain llama-index
```

## Running Benchmarks

### Basic Usage
```python
import asyncio
import psycopg_pool
from benchmarks.vectorstore_integration_benchmark import VectorStoreBenchmark, BenchmarkConfig

async def run_benchmarks():
    # Database connection
    conninfo = "postgresql://user:password@localhost:5432/fraiseql_benchmark"
    db_pool = psycopg_pool.AsyncConnectionPool(conninfo)

    # Configure benchmark
    config = BenchmarkConfig(
        document_count=5000,      # Number of documents to test with
        embedding_dimension=1536, # OpenAI ada-002 dimension
        concurrent_users=20,      # Concurrent users for load testing
        batch_sizes=[1, 10, 50, 100]  # Batch sizes to test
    )

    # Run benchmarks
    benchmark = VectorStoreBenchmark(db_pool, config)
    results = await benchmark.run_full_benchmark()

    # Results are saved to benchmark_results.json
    print("Benchmark completed! Results saved to benchmark_results.json")

asyncio.run(run_benchmarks())
```

### Command Line Usage
```bash
# Run with default configuration
python -c "
import asyncio
from benchmarks.vectorstore_integration_benchmark import main
asyncio.run(main())
"

# Run with custom configuration
python -c "
import asyncio
import psycopg_pool
from benchmarks.vectorstore_integration_benchmark import VectorStoreBenchmark, BenchmarkConfig

async def custom_benchmark():
    db_pool = psycopg_pool.AsyncConnectionPool('postgresql://localhost/fraiseql_benchmark')
    config = BenchmarkConfig(document_count=10000, concurrent_users=50)
    benchmark = VectorStoreBenchmark(db_pool, config)
    await benchmark.run_full_benchmark()

asyncio.run(custom_benchmark())
"
```

## Benchmark Configuration

### BenchmarkConfig Parameters

| Parameter | Default | Description |
|-----------|---------|-------------|
| `document_count` | 1000 | Number of documents to generate for testing |
| `embedding_dimension` | 384 | Dimension of embedding vectors |
| `batch_sizes` | [1, 10, 50, 100] | Batch sizes for insertion testing |
| `concurrent_users` | 10 | Number of concurrent users for load testing |
| `query_count` | 100 | Number of search queries to perform |
| `warmup_iterations` | 5 | Number of warmup iterations before benchmarking |

### Example Configurations

```python
# Quick smoke test
config = BenchmarkConfig(
    document_count=100,
    concurrent_users=1,
    query_count=10
)

# Production-scale test
config = BenchmarkConfig(
    document_count=50000,
    embedding_dimension=1536,  # OpenAI ada-002
    concurrent_users=100,
    batch_sizes=[10, 50, 100, 500]
)

# Memory profiling focus
config = BenchmarkConfig(
    document_count=10000,
    concurrent_users=1,  # Minimize concurrency for memory analysis
    batch_sizes=[1]  # Single document operations
)
```

## Benchmark Results

### Output Format

Results are saved as JSON with the following structure:

```json
{
  "timestamp": 1731600000.0,
  "config": {
    "document_count": 5000,
    "embedding_dimension": 384,
    "batch_sizes": [1, 10, 50, 100],
    "concurrent_users": 20,
    "query_count": 100
  },
  "frameworks": {
    "langchain": {
      "operations": {
        "insertion": {
          "count": 4,
          "avg_duration": 45.2,
          "avg_throughput": 110.5,
          "avg_memory": 125000000,
          "error_rate": 0.0,
          "results": [...]
        },
        "search": {...},
        "concurrent_load": {...}
      },
      "summary": {
        "avg_throughput": 85.3,
        "avg_memory_usage": 98000000,
        "total_errors": 0,
        "memory_efficiency": 0.85
      }
    },
    "llamaindex": {...}
  },
  "comparisons": {
    "performance_ratio": {
      "langchain_insertion": 1.0,
      "llamaindex_insertion": 0.95,
      "langchain_search": 1.0,
      "llamaindex_search": 1.12
    },
    "memory_efficiency": {...},
    "error_rates": {...}
  }
}
```

### Key Metrics

- **Throughput**: Operations per second (higher is better)
- **Memory Usage**: Peak memory consumption in bytes
- **Memory Efficiency**: Score from 0-1 (higher is better)
- **Error Rate**: Percentage of failed operations (lower is better)
- **Performance Ratio**: Relative performance compared to best framework

## Interpreting Results

### Performance Analysis

1. **Insertion Throughput**: Measures how quickly documents can be stored
   - Batch size impact: Larger batches generally improve throughput
   - Memory vs Speed trade-off: Larger batches use more memory but are faster

2. **Search Performance**: Evaluates query processing speed
   - Vector similarity search efficiency
   - Index utilization effectiveness

3. **Concurrent Load**: Tests real-world multi-user scenarios
   - Database connection pool efficiency
   - Lock contention and deadlock handling

4. **Memory Efficiency**: Analyzes resource utilization
   - Peak memory usage during operations
   - Memory cleanup and garbage collection effectiveness

### Framework Comparison

The benchmark provides direct comparisons between LangChain and LlamaIndex:

- **LangChain**: Generally better for bulk operations and complex workflows
- **LlamaIndex**: Often better for individual document processing and advanced indexing

### Production Readiness Indicators

- **Error Rate < 1%**: Indicates stable operation
- **Memory Efficiency > 0.8**: Good resource utilization
- **Throughput Consistency**: Stable performance across batch sizes
- **Concurrent Scaling**: Performance scales with user count

## Troubleshooting

### Common Issues

1. **pgvector Extension Missing**
   ```sql
   -- Connect to your database
   CREATE EXTENSION IF NOT EXISTS vector;
   ```

2. **Memory Issues**
   - Reduce `document_count` for memory-constrained environments
   - Use smaller `batch_sizes` to reduce peak memory usage
   - Monitor system memory during benchmark runs

3. **Timeout Issues**
   - Increase database connection timeouts
   - Reduce `concurrent_users` for slower systems
   - Use smaller batches to avoid long-running transactions

4. **Import Errors**
   - Install optional dependencies: `pip install langchain llama-index`
   - Ensure compatible versions of all packages

### Performance Tuning

Based on benchmark results, consider these optimizations:

1. **Database Tuning**
   ```sql
   -- Increase connection pool size
   ALTER SYSTEM SET max_connections = 200;

   -- Optimize vector index
   SET ivfflat.probes = 10;  -- Increase for better search quality
   ```

2. **Application Tuning**
   - Use appropriate batch sizes based on benchmark results
   - Implement connection pooling
   - Consider async/await patterns for better concurrency

3. **Memory Optimization**
   - Monitor and tune garbage collection
   - Use streaming for large datasets
   - Implement memory-efficient data structures

## Contributing

When adding new benchmark tests:

1. Follow the existing `BenchmarkResult` structure
2. Include proper error handling and cleanup
3. Add memory profiling for resource-intensive operations
4. Update this README with new configuration options
5. Provide example results and interpretation guidance

## Related Benchmarks

- `jsonb_generation_benchmark/`: JSONB query performance
- `ltree_performance_benchmark/`: Hierarchical data performance
- `cascade_performance_benchmark.py`: GraphQL cascade operations
