# JSONB Generation Method Benchmark

Comprehensive benchmark comparing different PostgreSQL methods for generating JSONB in FraiseQL views and Trinity tables.

## Methods Tested

### View-Based Generation (Query Side)

1. **`jsonb_build_object()`** - Current FraiseQL pattern
   ```sql
   jsonb_build_object('key1', col1, 'key2', col2, ...)
   ```

2. **`row_to_json()` with LATERAL** - Manual field selection with LATERAL join
   ```sql
   row_to_json(t)::jsonb FROM ... CROSS JOIN LATERAL (SELECT ...) t
   ```

3. **`row_to_json()` with subquery** - Manual field selection with subquery
   ```sql
   row_to_json((SELECT t FROM (SELECT ...) t))::jsonb
   ```

4. **`to_jsonb()`** - Simplest approach, converts entire row
   ```sql
   to_jsonb(table_name) - 'pk_field'
   ```

### Trinity Table GENERATED Columns (Write Side)

5. **GENERATED with `jsonb_build_object()`**
   ```sql
   data JSONB GENERATED ALWAYS AS (jsonb_build_object(...)) STORED
   ```

6. **GENERATED with `to_jsonb()`**
   ```sql
   data JSONB GENERATED ALWAYS AS (to_jsonb(table_name) - 'data' - 'pk') STORED
   ```

## Test Scenarios

1. **Single Row Lookup** - UUID-based single record retrieval (most common GraphQL pattern)
2. **Paginated Query** - 100 rows with OFFSET (typical GraphQL pagination)
3. **Filtered Query** - WHERE clause with moderate result set
4. **Full Scan** - Complete table scan (10,000 rows)
5. **Trinity Write** - INSERT performance with GENERATED column overhead

## Prerequisites

- PostgreSQL 12+ (for GENERATED columns)
- `pgbench` utility (included with PostgreSQL)
- Database with sufficient resources

## Quick Start

```bash
# 1. Run the benchmark (uses default 'postgres' database)
./run_benchmark.sh

# 2. Or specify your database
./run_benchmark.sh your_database_name

# 3. View results
cat results/benchmark_TIMESTAMP.md
```

## Manual Setup

If you want to run tests individually:

```bash
# 1. Setup schema and data
psql -d your_db -f 00_setup.sql

# 2. Run individual test
pgbench -d your_db -f 01_test_single_row.sql -c 10 -j 4 -T 30

# 3. Test specific view by editing SQL file
sed 's/v_user_jsonb_build/v_user_to_jsonb/g' 01_test_single_row.sql > test_to_jsonb.sql
pgbench -d your_db -f test_to_jsonb.sql -c 10 -j 4 -T 30
```

## Benchmark Configuration

Edit `run_benchmark.sh` to adjust:

```bash
DURATION=30      # Seconds per test
CLIENTS=10       # Concurrent clients
JOBS=4           # Worker threads
```

## Test Data

- **10,000 rows** in base table
- Realistic field types: UUID, TEXT, BOOLEAN, TEXT[], JSONB, TIMESTAMPTZ
- Nested JSONB metadata field
- 90% active users (for filtered tests)

## Expected Results

Based on PostgreSQL performance characteristics, we expect:

1. **`to_jsonb()`** - Fastest for views (C implementation, entire row)
2. **`row_to_json()`** - Good balance (2-3x faster than jsonb_build_object)
3. **`jsonb_build_object()`** - Slowest but most flexible (current approach)

For **Trinity tables with GENERATED columns**:
- Similar performance characteristics
- Additional storage overhead for precomputed JSONB
- Faster SELECT queries (no generation cost)
- Slower INSERT/UPDATE (generation overhead)

## Interpreting Results

### Key Metrics

- **TPS (transactions per second)** - Higher is better
- **Latency avg** - Lower is better (milliseconds)
- **Latency stddev** - Lower is better (more consistent)

### Trade-offs

**`to_jsonb()` Advantages:**
- ✅ Fastest generation
- ✅ Simplest SQL
- ❌ Snake_case keys (requires Rust transformation)
- ❌ Less control over field selection

**`row_to_json()` Advantages:**
- ✅ 2-3x faster than jsonb_build_object
- ✅ Full control over fields
- ✅ Can use camelCase in SQL
- ⚠️ Slightly more complex SQL

**`jsonb_build_object()` Advantages:**
- ✅ Maximum flexibility
- ✅ Current FraiseQL pattern (no changes needed)
- ❌ Slowest method
- ❌ Verbose for many fields

## Real-World Considerations

Beyond raw performance, consider:

1. **Field Count** - `to_jsonb()` advantage grows with more fields
2. **Nested Objects** - May still need `jsonb_build_object()` for composition
3. **CamelCase** - Your Rust transformer already handles this
4. **Maintainability** - Simpler SQL = easier to maintain
5. **Trinity Pattern** - GENERATED columns trade write performance for read performance

## Storage Analysis

The benchmark includes storage comparison:

- **Base table size** - Without JSONB column
- **Trinity with jsonb_build_object** - Generated column overhead
- **Trinity with to_jsonb** - Generated column overhead

Generated columns typically add **30-50% storage overhead** but eliminate query-time generation cost.

## Recommendations

Based on benchmark results, you should see:

**For Views:**
1. Use `to_jsonb()` for simple views (let Rust handle camelCase)
2. Use `row_to_json()` when you need field selection in SQL
3. Reserve `jsonb_build_object()` for complex nested compositions

**For Trinity Tables:**
1. Use GENERATED columns when read:write ratio > 10:1
2. Choose `to_jsonb()` for GENERATED columns (simpler, faster)
3. Consider materialized views for expensive aggregations

## Cleanup

```bash
# Remove test data
psql -d your_db -c "DROP TABLE IF EXISTS tb_user_bench CASCADE;"
psql -d your_db -c "DROP TABLE IF EXISTS tv_user_jsonb_build CASCADE;"
psql -d your_db -c "DROP TABLE IF EXISTS tv_user_to_jsonb CASCADE;"
```

## Contributing

Found interesting results? Please share:
- Your hardware specs (CPU, RAM, disk type)
- PostgreSQL version and configuration
- Benchmark results summary
- Any unexpected findings

## References

- [PostgreSQL JSON Functions](https://www.postgresql.org/docs/current/functions-json.html)
- [pgbench Documentation](https://www.postgresql.org/docs/current/pgbench.html)
