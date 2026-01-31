# FraiseQL Baseline Benchmarks - Setup Report

**Generated**: 2026-01-31  
**Status**: Ready to Run  
**Rust Toolchain**: cargo 1.95.0-nightly, rustc 1.95.0-nightly

---

## 1. Benchmark Files Summary

### fraiseql-core Benchmarks (5 files)

Located: `/home/lionel/code/fraiseql/crates/fraiseql-core/benches/`

| File | Purpose | Status |
|------|---------|--------|
| `adapter_comparison.rs` | PostgresAdapter vs FraiseWireAdapter performance comparison | ✅ Ready |
| `federation_bench.rs` | Federation query performance | ✅ Ready |
| `full_pipeline_comparison.rs` | Complete GraphQL execution pipeline | ✅ Ready |
| `saga_performance_bench.rs` | Saga transaction performance | ✅ Ready |
| `sql_projection_benchmark.rs` | Field projection efficiency | ✅ Ready |
| **fixtures/** | Test data setup scripts | ✅ Ready |

### fraiseql-wire Benchmarks (4 files)

Located: `/home/lionel/code/fraiseql/crates/fraiseql-wire/benches/`

| File | Purpose | Status |
|------|---------|--------|
| `micro_benchmarks.rs` | JSON parsing, chunking, error handling | ✅ Ready |
| `comparison_benchmarks.rs` | Wire protocol comparisons | ✅ Ready |
| `integration_benchmarks.rs` | Postgres integration performance | ✅ Ready |
| `phase6_validation.rs` | Phase 6 validation benchmarks | ✅ Ready |
| `setup.sql` | Database setup for Postgres tests | ✅ Ready |

### fraiseql-server Benchmarks (1 file)

Located: `/home/lionel/code/fraiseql/crates/fraiseql-server/benches/`

| File | Purpose | Status |
|------|---------|--------|
| `performance_benchmarks.rs` | HTTP server performance | ✅ Ready |

### Workspace Benchmarks (1 file)

Located: `/home/lionel/code/fraiseql/benches/`

| File | Purpose | Status |
|------|---------|--------|
| `arrow_flight_benchmarks.rs` | Arrow Flight performance | ✅ Ready |

**Total**: 11 benchmark files + fixtures + setup scripts

---

## 2. Benchmark Infrastructure

### Criterion Configuration

**Status**: ✅ Configured in all crates

```toml
[dev-dependencies]
criterion = { version = "0.5", features = ["async_tokio"] }
tokio-test = "0.4"
```

**Features Enabled**:
- `async_tokio` - Async benchmark support
- Statistical analysis
- HTML report generation to `target/criterion/`
- Regression detection
- Trend analysis

### Rust Toolchain

```
cargo:  1.95.0-nightly (efcd9f586 2026-01-23)
rustc:  1.95.0-nightly (f134bbc78 2026-01-24)
```

**Status**: ✅ Modern nightly with full feature support

---

## 3. Test Environment Configuration

### Environment Variables

**File**: `/home/lionel/code/fraiseql/.env.test`

```
FRAISEQL_TEST_POSTGRES_URL=postgresql://fraiseql_test:fraiseql_test_password@localhost:5433/test_fraiseql
FRAISEQL_TEST_POSTGRES_VECTOR_URL=postgresql://fraiseql_test:fraiseql_test_password@localhost:5434/test_fraiseql_vector
FRAISEQL_TEST_MYSQL_URL=mysql://fraiseql_test:fraiseql_test_password@localhost:3307/test_fraiseql
FRAISEQL_TEST_SQLITE_URL=:memory:
RUST_TEST_THREADS=4
RUST_LOG=debug
RUST_BACKTRACE=1
```

**Status**: ✅ Configured and ready

### Docker Compose

**Files**:
- Root: `/home/lionel/code/fraiseql/docker-compose.yml` (main test DB stack)
- Test: `/home/lionel/code/fraiseql/docker-compose.test.yml` (CI/CD)
- E2E: `/home/lionel/code/fraiseql/docker-compose.e2e.yml` (end-to-end)
- Wire-specific: `/home/lionel/code/fraiseql/crates/fraiseql-wire/docker-compose.yml`

**Status**: ✅ Docker Compose v2 available

**Services in docker-compose.yml**:
- PostgreSQL 17 (port 5433)
- PostgreSQL+pgvector (port 5434)
- MySQL (port 3307)
- Redis, Nats, Kafka (optional)

---

## 4. Fixture Data & Setup Scripts

### SQL Setup Files

```
crates/fraiseql-core/benches/fixtures/
├── setup_bench_data.sql        (1M rows test data)
└── setup_user_data.sql         (user entity data)

crates/fraiseql-wire/benches/
└── setup.sql                   (wire protocol test views)

benches/fixtures/
└── setup_bench_data.sql        (workspace-level fixtures)
```

**Status**: ✅ All scripts present and ready to load

### Benchmark Compilation

```bash
$ cargo build --benches --features "postgres,wire-backend"
   Finished `dev` profile [unoptimized + debuginfo] target(s) in 16.97s
```

**Status**: ✅ Benchmarks compile successfully

---

## 5. What Needs to Happen to Run Benchmarks

### Step 1: Start Test Databases

```bash
cd /home/lionel/code/fraiseql

# Using Docker Compose (recommended)
docker compose -f docker-compose.yml up -d

# Verify services are healthy
docker compose -f docker-compose.yml ps
docker compose -f docker-compose.yml logs postgres  # Check startup
```

**Expected**: All services healthy within 30 seconds

### Step 2: Load Benchmark Data

For **fraiseql-core adapter benchmarks**:

```bash
# Load 1M rows into PostgreSQL
psql "postgresql://fraiseql_test:fraiseql_test_password@localhost:5433/test_fraiseql" \
  < /home/lionel/code/fraiseql/crates/fraiseql-core/benches/fixtures/setup_bench_data.sql

# Wait ~30-60 seconds for data load
```

For **fraiseql-wire integration benchmarks**:

```bash
# Load wire protocol test views
psql "postgresql://fraiseql_test:fraiseql_test_password@localhost:5433/test_fraiseql" \
  < /home/lionel/code/fraiseql/crates/fraiseql-wire/benches/setup.sql
```

**Expected**: ~1-2 minutes for full setup

### Step 3: Verify Environment

```bash
# Verify PostgreSQL is accessible
psql "postgresql://fraiseql_test:fraiseql_test_password@localhost:5433/test_fraiseql" -c "SELECT version();"

# Set environment variable
export DATABASE_URL="postgresql://fraiseql_test:fraiseql_test_password@localhost:5433/test_fraiseql"

# Verify data loaded
psql $DATABASE_URL -c "SELECT COUNT(*) FROM v_benchmark_data LIMIT 1;"
```

**Expected**: Returns row count, proves data is loaded

### Step 4: Run Benchmarks

**Single crate benchmark**:

```bash
cd /home/lionel/code/fraiseql

# Run fraiseql-core adapter benchmarks (most comprehensive)
cargo bench --bench adapter_comparison --features "postgres,wire-backend"

# Run only specific tests
cargo bench --bench adapter_comparison -- "10k_rows"
cargo bench --bench adapter_comparison -- "100k_rows"
cargo bench --bench adapter_comparison -- "1m_rows"
```

**Wire benchmarks**:

```bash
# Micro benchmarks (fast, ~30 seconds)
cargo bench --bench micro_benchmarks -p fraiseql-wire

# Integration benchmarks (requires Postgres)
cargo bench --bench integration_benchmarks -p fraiseql-wire --features "bench-with-postgres"
```

**All benchmarks**:

```bash
# Run all benchmarks in workspace (takes ~15-30 minutes)
cargo bench --features "postgres,wire-backend,arrow"
```

### Step 5: View Results

```bash
# Open HTML report in browser
open /home/lionel/code/fraiseql/target/criterion/report/index.html

# Or view specific benchmark
open /home/lionel/code/fraiseql/target/criterion/10k_rows/report/index.html
```

---

## 6. Expected Baseline Performance (from docs)

### fraiseql-core Adapter Comparison

| Benchmark | PostgresAdapter | FraiseWireAdapter | Winner |
|-----------|-----------------|-------------------|--------|
| 10K latency | 32ms | 33ms | PostgresAdapter (3% faster) |
| 100K latency | 320ms | 330ms | PostgresAdapter (3% faster) |
| 1M latency | 4.2s | 4.0s | FraiseWireAdapter (5% faster) |
| 10K memory | 260 KB | 1.3 KB | FraiseWireAdapter (200x) ⭐ |
| 100K memory | 26 MB | 1.3 KB | FraiseWireAdapter (20,000x) ⭐⭐ |
| 1M memory | 260 MB | 1.3 KB | FraiseWireAdapter (200,000x) ⭐⭐⭐ |

**Key Insight**: FraiseWireAdapter achieves orders-of-magnitude memory savings through streaming.

### fraiseql-wire Micro Benchmarks

Expected times (from docs):
- JSON parsing (small): ~125 µs
- Connection parsing: ~50 µs
- Chunking: ~1-2 µs
- Error handling: <1 µs

**Key Insight**: All micro-operations complete in microseconds.

---

## 7. Troubleshooting Checklist

| Issue | Solution |
|-------|----------|
| "DATABASE_URL not set" | Export: `export DATABASE_URL="postgresql://..."`|
| "Test data not found in v_benchmark_data" | Load fixtures: `psql $DATABASE_URL < benches/fixtures/setup_bench_data.sql` |
| "No database adapters enabled" | Use features: `--features "postgres,wire-backend"` |
| "Connection refused on port 5433" | Start Docker: `docker compose up -d` |
| "Benchmarks out of memory on 1M rows" | Reduce to smaller batches or increase system RAM |
| "Criterion report not generating" | Check `target/criterion/` permissions |

---

## 8. Quick Start Commands

```bash
# 1. Navigate to project
cd /home/lionel/code/fraiseql

# 2. Start databases
docker compose -f docker-compose.yml up -d

# 3. Wait for health checks (~30s)
docker compose -f docker-compose.yml ps

# 4. Load test data (one-time, ~1-2 min)
export DATABASE_URL="postgresql://fraiseql_test:fraiseql_test_password@localhost:5433/test_fraiseql"
psql $DATABASE_URL < crates/fraiseql-core/benches/fixtures/setup_bench_data.sql

# 5. Run benchmarks
cargo bench --bench adapter_comparison --features "postgres,wire-backend"

# 6. View results
open target/criterion/report/index.html
```

**Total time to first baseline**: ~5 minutes (10 min if data load is slow)

---

## 9. Available Benchmark Filters

```bash
# By size
cargo bench -- "10k_rows"
cargo bench -- "100k_rows"
cargo bench -- "1m_rows"

# By operation
cargo bench -- "where_clause"
cargo bench -- "pagination"
cargo bench -- "field_projection"

# By adapter
cargo bench -- "postgres_adapter"
cargo bench -- "wire_adapter"
cargo bench -- "fraiseql"
```

---

## 10. Integration with CI/CD

**GitHub Actions Ready**: See `crates/fraiseql-core/benches/README.md` for full CI example

Benchmark results are currently:
- Generated locally during development
- Stored in `target/criterion/` for trend analysis
- Can be committed to track regressions
- Ready for automated CI detection of 5%+ regressions

---

## Summary

✅ **Status**: All benchmark infrastructure is in place and ready to execute

**What's Available**:
- 11 benchmark files across 3 crates
- Criterion.rs with statistical analysis
- Docker Compose test database stack
- 1M row benchmark dataset
- HTML report generation
- Regression detection

**Next Steps**:
1. Start Docker Compose: `docker compose -f docker-compose.yml up -d`
2. Load benchmark data: `psql $DATABASE_URL < crates/fraiseql-core/benches/fixtures/setup_bench_data.sql`
3. Run baseline: `cargo bench --bench adapter_comparison --features "postgres,wire-backend"`
4. View results: `open target/criterion/report/index.html`

**Estimated Time to Baseline**: 5-10 minutes

