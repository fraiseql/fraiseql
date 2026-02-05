# fraiseql-wire Benchmarks

This directory contains performance benchmarks for fraiseql-wire core operations.

## Running Benchmarks

### Micro-benchmarks (Always Run)

Fast benchmarks of core operations (~30 seconds):

```bash
cargo bench --bench micro_benchmarks
```

Results are stored in `target/criterion/` for trend analysis.

### Benchmark Groups

**json_parsing**: JSON parsing performance

- `small` - Small JSON object (~200 bytes)
- `large` - Large JSON with nested structures (~2KB)
- `deeply_nested` - Deeply nested JSON to test recursion depth

**connection_parsing**: Connection string parsing

- `parse_0` - Simple localhost connection
- `parse_1` - Connection with credentials
- `parse_2` - Connection with query parameters
- `parse_3` - Unix socket default

**chunking**: BytesMut chunking overhead

- `64` - Small chunk size
- `256` - Medium chunk size (default)
- `1024` - Large chunk size

**error_handling**: Error type overhead

- `error_construction` - Creating an io::Error
- `error_conversion_to_string` - Converting error to string

**string_matching**: SQL predicate string operations

- `contains_check` - Checking if string contains substring
- `split_operation` - Splitting string by delimiter

**hashmap_ops**: Connection parameter lookups

- `insert_5_items` - Creating HashMap with 5 items
- `lookup_existing` - Looking up existing key
- `lookup_missing` - Looking up missing key

## Interpreting Results

Criterion produces detailed statistical analysis for each benchmark:

```
json_parsing/small         time:   [125.34 µs 126.12 µs 127.05 µs]
                          change: [-2.1% -0.5% +1.3%] (within noise)
```

- **time**: Point estimate, lower bound, upper bound (95% confidence interval)
- **change**: Percent change vs. baseline (if comparing to previous run)
- **outliers**: Statistical analysis of variance

## Regression Detection

If you see results like:

```
json_parsing/large        time:   [850.21 µs 862.34 µs 876.45 µs]
                          change: [+15.2% +22.1% +31.8%] (regression)
```

This indicates a potential performance regression. Investigate by:

1. Reviewing recent code changes to JSON parsing
2. Checking if changes were intentional
3. Running benchmarks on a clean branch for comparison
4. Using profiling tools (flamegraph, perf) to identify hot paths

## Integration Benchmarks

Real-world performance benchmarks against Postgres 17:

```bash
cargo bench --bench integration_benchmarks --features bench-with-postgres
```

### Integration Benchmark Groups

**throughput**: Streaming performance (rows/second)

- `1000_rows` - Small result set
- `10000_rows` - Medium result set
- `100000_rows` - Large result set

**latency**: Time-to-first-row under different result sizes

- `ttfr_1k` - 1,000 row set
- `ttfr_100k` - 100,000 row set
- `ttfr_1m` - 1,000,000 row set

**connection_setup**: Connection overhead

- `tcp_connection` - Network connection (localhost)
- `unix_socket_connection` - Unix socket (faster)

**memory_usage**: Memory consumption by chunk size

- `chunk_64` - 64-byte chunks
- `chunk_256` - 256-byte chunks (default)
- `chunk_1024` - 1024-byte chunks

**chunking_strategy**: Chunking efficiency impact

- `chunk_64` through `chunk_1024` - Processing with different chunk sizes

**predicate_effectiveness**: SQL predicate filtering impact

- `no_filter` - All 100,000 rows
- `sql_1percent` - Filtered to 1% (1,000 rows)
- `sql_10percent` - Filtered to 10% (10,000 rows)
- `sql_50percent` - Filtered to 50% (50,000 rows)

**streaming_stability**: Long-running stability benchmarks

- `large_result_set_1m_rows` - 1M row streaming memory stability
- `high_throughput_small_chunks` - High throughput with small chunks

**json_parsing_load**: JSON parsing under realistic loads

- `small_200b` - Small JSON payloads
- `medium_2kb` - Medium JSON payloads
- `large_10kb` - Large JSON payloads
- `huge_100kb` - Very large JSON payloads

### Setup

Before running integration benchmarks, create the test database:

```bash
# Create test database
psql -U postgres -c "CREATE DATABASE fraiseql_bench"

# Load test data views
psql -U postgres fraiseql_bench < benches/setup.sql

# Verify data loaded
psql -U postgres fraiseql_bench -c "SELECT COUNT(*) FROM v_test_100k"
```

### Cleanup

```bash
psql -U postgres -c "DROP DATABASE fraiseql_bench"
```

See `BENCHMARKING.md` for full strategy.
