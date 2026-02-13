# fraiseql-wire Testing Guide

## Overview

This guide explains how to run the fraiseql-wire test suite, including unit tests, integration tests, load tests, and stress tests.

---

## Test Structure

### Test Types

1. **Unit Tests** (`.rs` files in `src/`)
   - Protocol encoding/decoding
   - JSON validation
   - SQL generation
   - Error handling
   - **Run**: `cargo test --lib`

2. **Integration Tests** (`tests/` directory)
   - End-to-end client functionality
   - Connection lifecycle
   - Query execution
   - **Run**: `cargo test --test integration -- --ignored`

3. **Load Tests** (`tests/load_tests.rs`)
   - Throughput benchmarks
   - Memory stability
   - Concurrent connections
   - Chunk size optimization
   - **Run**: `cargo test --test load_tests -- --ignored --nocapture`

4. **Stress Tests** (`tests/stress_tests.rs`)
   - Failure scenarios
   - Error handling
   - Recovery mechanisms
   - Edge cases
   - **Run**: `cargo test --test stress_tests -- --ignored --nocapture`

---

## Setting Up the Test Environment

### Prerequisites

1. **Postgres 17** (or compatible)

   ```bash
   # Check if Postgres is running
   pg_isready

   # Start Postgres if needed
   sudo systemctl start postgresql
   ```

2. **Test Database**

   ```bash
   # Create test database
   sudo -u postgres createdb fraiseql_test

   # Or with psql
   psql -U postgres -c "CREATE DATABASE fraiseql_test;"
   ```

3. **Load Test Schema** (required for load/stress tests)

   ```bash
   # Initialize staging schema and tables
   psql -U postgres -d fraiseql_test -f tests/fixtures/schema.sql

   # Seed with test data
   psql -U postgres -d fraiseql_test -f tests/fixtures/seed_data.sql
   ```

### Using Docker (Recommended)

For isolated testing, use Docker Compose:

```bash
# Start Postgres in Docker
docker-compose up -d postgres

# The database is ready when:
docker-compose exec postgres pg_isready

# Initialize schema and seed data
docker-compose exec postgres psql -U postgres -d fraiseql_test -f tests/fixtures/schema.sql
docker-compose exec postgres psql -U postgres -d fraiseql_test -f tests/fixtures/seed_data.sql
```

---

## Running Tests

### All Unit Tests

```bash
cargo test --lib
```

**Expected Output**:

```
running 47 tests

test result: ok. 47 passed; 0 failed; 0 ignored
```

### All Integration Tests (Requires Postgres)

```bash
# Set environment variables
export POSTGRES_HOST=localhost
export POSTGRES_USER=postgres
export POSTGRES_PASSWORD=postgres
export POSTGRES_DB=fraiseql_test

# Run tests
cargo test --test integration -- --ignored --nocapture
```

### Load Tests (Requires Postgres + Schema)

```bash
# Ensure schema is loaded first
psql -U postgres -d fraiseql_test -f tests/fixtures/schema.sql
psql -U postgres -d fraiseql_test -f tests/fixtures/seed_data.sql

# Run load tests
export POSTGRES_HOST=localhost
export POSTGRES_USER=postgres
export POSTGRES_PASSWORD=postgres
export POSTGRES_DB=fraiseql_test

cargo test --test load_tests -- --ignored --nocapture
```

**Load Tests Include**:

- Moderate volume streaming
- Large volume with custom chunk size
- SQL predicate filtering
- Rust predicate filtering
- Large JSON object handling
- ORDER BY verification
- Concurrent connections
- Sustained streaming
- Chunk size comparison
- Partial stream consumption

### Stress Tests (Requires Postgres + Schema)

```bash
export POSTGRES_HOST=localhost
export POSTGRES_USER=postgres
export POSTGRES_PASSWORD=postgres
export POSTGRES_DB=fraiseql_test

cargo test --test stress_tests -- --ignored --nocapture
```

**Stress Tests Include**:

- Early stream drop (client disconnect)
- Invalid connection strings
- Connection refused (unreachable host)
- Missing tables
- Invalid WHERE clauses
- Empty result sets
- Large WHERE clauses
- Rapid connection cycling
- Single connection, multiple queries
- Tiny chunk size
- Huge chunk size
- Wrong credentials
- Partial consumption
- Zero chunk size
- Invalid ORDER BY
- Combined predicates
- JSON validity verification
- Complex ORDER BY expressions

### Run All Tests

```bash
# Unit tests (always runs)
cargo test --lib

# Integration, load, and stress tests (requires Postgres)
cargo test -- --ignored --nocapture
```

---

## Test Database Verification

### Check Schema

```bash
# List all tables in test_staging schema
psql -U postgres -d fraiseql_test -c "\dt test_staging.*"

# Expected output:
#                   List of relations
#  Schema        | Name      | Type  | Owner
# ───────────────┼───────────┼───────┼────────
#  test_staging  | documents | table | postgres
#  test_staging  | projects  | table | postgres
#  test_staging  | tasks     | table | postgres
#  test_staging  | users     | table | postgres
```

### Check Views

```bash
psql -U postgres -d fraiseql_test -c "\dv test_staging.*"
```

### Check Data

```bash
# Count rows in each table
psql -U postgres -d fraiseql_test -c "SELECT * FROM test_staging.row_counts();"

# Sample data
psql -U postgres -d fraiseql_test -c "SELECT jsonb_pretty(data) FROM test_staging.v_projects LIMIT 1;"
```

### Truncate Test Data (Reset)

```bash
psql -U postgres -d fraiseql_test -c "SELECT test_staging.truncate_all();"
```

---

## Interpreting Test Results

### Load Test Output

```
Test: Moderate data volume streaming
  Rows: 5
  Time: 15ms
  Throughput: 333 rows/sec
```

**What This Means**:

- 5 rows were streamed from the projects table
- Took 15 milliseconds
- Throughput is 333 rows/sec

### Stress Test Output

```
Test: Early stream drop (client disconnect)
  Received first row, dropping stream...
  Stream dropped: ✓
  Reconnection successful: ✓
```

**What This Means**:

- Stream was successfully dropped after receiving one row
- Client can reconnect and use the connection again
- Error handling and resource cleanup work correctly

---

## Performance Expectations

### Baseline Metrics

After running load tests, you should see:

| Metric | Expected | Notes |
|--------|----------|-------|
| Throughput | > 100 rows/sec | Depends on JSON size |
| Latency (first row) | < 10ms | For local Postgres |
| Memory stability | < 100MB | With default chunk size |
| Connection setup | < 50ms | TCP to localhost |

### Factors Affecting Performance

1. **JSON Size**: Larger objects = lower throughput
2. **Chunk Size**: Affects memory usage and latency
3. **Predicates**: SQL predicates reduce data volume
4. **Network**: TCP slower than Unix socket
5. **Postgres Load**: Other queries affect performance

---

## Debugging Failed Tests

### Connection Issues

```bash
# Verify Postgres is running
pg_isready -h localhost -p 5432

# Check database exists
psql -U postgres -l | grep fraiseql_test

# Check schema is loaded
psql -U postgres -d fraiseql_test -c "SELECT * FROM information_schema.tables WHERE table_schema = 'test_staging';"
```

### Schema Issues

```bash
# Reload schema
psql -U postgres -d fraiseql_test -f tests/fixtures/schema.sql

# Clear and reseed data
psql -U postgres -d fraiseql_test -c "SELECT test_staging.truncate_all();"
psql -U postgres -d fraiseql_test -f tests/fixtures/seed_data.sql
```

### Test Output

Run with `--nocapture` to see print statements:

```bash
cargo test --test load_tests -- --ignored --nocapture
```

### Verbose Output

```bash
# Show test names as they run
cargo test --test stress_tests -- --ignored --nocapture --test-threads=1
```

### Single Test

Run a specific test:

```bash
cargo test --test load_tests test_load_moderate_volume -- --ignored --nocapture
```

---

## CI/CD Testing

### GitHub Actions

The project includes automated testing in `.github/workflows/ci.yml`:

1. **Unit Tests**: Run on every push
2. **Integration Tests**: Run with Postgres service
3. **Load Tests**: Optional, can be run manually or nightly

### Local CI Simulation

To test locally before pushing:

```bash
# Run everything the CI does
cargo fmt -- --check
cargo clippy -- -D warnings
cargo test --lib
cargo test --test integration -- --ignored
```

---

## Best Practices

### When Writing Tests

1. **Use `#[ignore]` for tests requiring Postgres**

   ```rust
   #[tokio::test]
   #[ignore] // Requires Postgres running
   async fn test_database_operation() { }
   ```

2. **Provide clear test names**
   - `test_load_moderate_volume` not `test_load_1`
   - Describes what is being tested

3. **Include println! for diagnostics**

   ```rust
   println!("  Rows: {}", count);
   println!("  Time: {:?}", elapsed);
   println!("  Throughput: {:.0} rows/sec", throughput);
   ```

4. **Test both success and failure paths**
   - Happy path: correct input
   - Error path: invalid input, missing resources

5. **Clean up resources**
   - Drop streams explicitly
   - Close connections
   - Test reconnection

### When Running Tests

1. **Run unit tests frequently** (no dependencies)
2. **Run integration tests before committing** (with Postgres)
3. **Run load tests periodically** (time-consuming)
4. **Run stress tests when adding error handling** (validates robustness)

---

## Troubleshooting

### "connection refused" Error

```
Error: connection error: failed to connect to localhost:5432: connection refused
```

**Solution**:

1. Verify Postgres is running: `pg_isready`
2. Check hostname/port environment variables
3. Start Postgres: `sudo systemctl start postgresql`

### "database does not exist" Error

```
Error: connection error: database "fraiseql_test" does not exist
```

**Solution**:

1. Create database: `createdb fraiseql_test`
2. Load schema: `psql -U postgres -d fraiseql_test -f tests/fixtures/schema.sql`

### "relation does not exist" Error

```
Error: sql error: relation "test_staging.projects" does not exist
```

**Solution**:

1. Schema not loaded: `psql -U postgres -d fraiseql_test -f tests/fixtures/schema.sql`
2. Wrong schema: Verify connection string uses correct database

### Authentication Failed

```
Error: authentication failed: role "test" does not exist
```

**Solution**:

1. Check `POSTGRES_USER` environment variable
2. Verify user exists: `psql -U postgres -c "\du"`
3. Use correct credentials

### Tests Hang/Timeout

```bash
# Run with timeout and single thread
timeout 30 cargo test --test load_tests -- --ignored --nocapture --test-threads=1
```

---

## Next Steps

1. **Read the implementation plan**: `.claude/phases/phase-7-3-7-6-stabilization.md`
2. **Review acceptance criteria**: Success metrics for each phase
3. **Run baseline tests**: Establish performance baseline
4. **Monitor over time**: Track throughput and memory

---

## Related Documentation

- **Load Testing**: Throughput, memory, concurrency
- **Stress Testing**: Failure scenarios, recovery
- **Performance Tuning**: `PERFORMANCE_TUNING.md`
- **Architecture**: `.claude/CLAUDE.md`

---

**Ready to test fraiseql-wire!** 🧪
