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

## Future: Integration Benchmarks

Once Phase 7.2 completes, integration benchmarks will be added to measure:
- Throughput (rows/second) with real Postgres
- Memory usage under load
- Connection setup time
- Large result set streaming

See `BENCHMARKING.md` for full strategy.
