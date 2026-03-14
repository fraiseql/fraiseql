# Performance Testing

**Quick Start:**

```bash
pytest tests/performance/test_performance.py -v -s
```

## Measured Performance (AWS t3.large equivalent)

| Scenario | Latency | Notes |
|----------|---------|-------|
| Single row | 0.83ms | Sub-millisecond |
| 100 rows | 2.59ms | Paginated list |
| 1000 rows | 10.34ms | Linear scaling |
| 20 concurrent | 1.61ms avg | Excellent consistency |

## Component Breakdown

Each test measures time spent in:

- **PostgreSQL**: Query execution (typically 35-89%)
- **Rust Pipeline**: JSON transformation (3-40%, scales with data size)
- **Driver Overhead**: psycopg3 connection pool + result fetching (5-41%)

## Key Insights

1. **PostgreSQL is the bottleneck** - Focus optimization here, not on the driver
2. **Rust pipeline scales linearly** - No performance cliffs with large result sets
3. **Driver overhead is constant** - Switching to asyncpg saves <1ms (not worth the migration cost)
4. **Linear scaling** - 10 rows to 1000 rows = 11.9x time increase (not exponential)

## Details

See [`MEDIUM_VPS_BENCHMARKS.md`](MEDIUM_VPS_BENCHMARKS/) for:

- Complete benchmark results
- Hardware profile specification
- Optimization recommendations
- Comparison with other hardware configurations

## Running Tests Yourself

```bash
# Run all performance tests
pytest tests/performance/test_performance.py -v -s

# Run specific test
pytest tests/performance/test_performance.py::TestRealisticPerformance::test_single_user_lookup -v -s

# Run with verbose output
pytest tests/performance/test_performance.py -v -s --tb=short
```

## Test Coverage

The test suite includes:

- **Single row lookup** - Basic query performance
- **List queries** - Multiple rows (10, 100, 500, 1000)
- **Nested data** - Complex JSONB structures
- **Multi-condition queries** - WHERE clause with multiple conditions
- **Concurrent load** - 20 simultaneous queries
- **Component timing** - Breakdown of where time is spent

All tests use realistic tv_* materialized tables with proper indices.
