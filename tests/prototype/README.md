# Phase 0 Prototype: PyO3 Async Bridge Validation

**Status**: Prototype / Proof of Concept
**Goal**: Validate PyO3 async/await integration before full Tokio driver implementation

---

## 🎯 Objectives

This prototype validates the following critical components:

1. **PyO3 Async Bridge** - Can we bridge async Rust to Python coroutines?
2. **GIL Handling** - Do concurrent queries work without deadlocks?
3. **Cancellation** - Does Python's asyncio cancellation propagate to Rust?
4. **Error Propagation** - Do Rust errors surface correctly in Python?
5. **Performance** - Is there a measurable speedup vs psycopg?
6. **Memory Safety** - Are there memory leaks after 1000+ queries?

---

## 🏗️ Architecture

```
Python (asyncio)
    ↓
PyO3 FFI Boundary
    ↓
pyo3-async-runtimes (bridge)
    ↓
Tokio Runtime
    ↓
deadpool-postgres
    ↓
tokio-postgres
    ↓
PostgreSQL
```

**Key Components**:
- `fraiseql_rs/src/db/prototype.rs` - Minimal async pool implementation
- `PrototypePool` - Python class wrapping `deadpool_postgres::Pool`
- `future_into_py()` - Bridges Rust `Future` to Python coroutine

---

## 📦 Setup

### Prerequisites

1. **PostgreSQL** running locally:
   ```bash
   # macOS (Homebrew)
   brew services start postgresql@15

   # Linux
   sudo systemctl start postgresql

   # Docker
   docker run -d \
     --name postgres-test \
     -e POSTGRES_PASSWORD=postgres \
     -p 5432:5432 \
     postgres:15
   ```

2. **Python dependencies**:
   ```bash
   # Core dependencies
   pip install pytest pytest-asyncio

   # Baseline comparison (optional)
   pip install psycopg psycopg-pool
   ```

3. **Build Rust extension**:
   ```bash
   cd fraiseql_rs
   maturin develop --release
   ```

---

## 🧪 Running Tests

### Full Test Suite

```bash
# Run all prototype tests
pytest tests/prototype/test_async_bridge.py -v

# Run without slow tests (skip 1000-query memory leak tests)
pytest tests/prototype/test_async_bridge.py -v -m "not slow"

# Run specific test class
pytest tests/prototype/test_async_bridge.py::TestBasicQueries -v

# Run with output (see print statements)
pytest tests/prototype/test_async_bridge.py -v -s
```

### Individual Test Categories

```bash
# Test 1: Basic queries
pytest tests/prototype/test_async_bridge.py::TestBasicQueries -v

# Test 2: Concurrent queries (GIL handling)
pytest tests/prototype/test_async_bridge.py::TestConcurrentQueries -v

# Test 3: Cancellation
pytest tests/prototype/test_async_bridge.py::TestCancellation -v

# Test 4: Error handling
pytest tests/prototype/test_async_bridge.py::TestErrorHandling -v

# Test 5: Memory leaks (slow)
pytest tests/prototype/test_async_bridge.py::TestMemoryLeaks -v
```

---

## 📊 Benchmarks

### Run Benchmark Comparison

```bash
python tests/prototype/benchmark_comparison.py
```

**Expected Output**:
```
================================================================================
                            BENCHMARK RESULTS
================================================================================

1. Simple Query (SELECT 1)
--------------------------------------------------------------------------------
  Average Latency:
    Python (psycopg): 1.234ms
    Rust (prototype): 0.567ms
    Speedup: 2.18x ✅

2. 1000-Row Query
--------------------------------------------------------------------------------
  Average Latency:
    Python (psycopg): 12.345ms
    Rust (prototype): 5.678ms
    Speedup: 2.17x ✅

3. 10 Concurrent Queries
--------------------------------------------------------------------------------
  Average Latency:
    Python (psycopg): 3.456ms
    Rust (prototype): 1.234ms
    Speedup: 2.80x ✅
```

**Success Criteria**:
- ✅ Speedup > 1.5x for simple queries
- ✅ Speedup > 2.0x for concurrent queries
- ✅ No GIL deadlocks
- ✅ No memory leaks

---

## ✅ Success Criteria

| Criteria | Test | Status |
|----------|------|--------|
| **Basic query execution** | `test_simple_select` | ⬜ |
| **Concurrent queries (no deadlock)** | `test_concurrent_simple_queries` | ⬜ |
| **Cancellation works** | `test_query_cancellation` | ⬜ |
| **Errors propagate** | `test_syntax_error` | ⬜ |
| **Performance gain** | `benchmark_comparison.py` | ⬜ |
| **No memory leaks** | `test_no_memory_leak_simple_queries` | ⬜ |

**Decision Point**:
- ✅ All tests pass → Proceed to Phase 1 implementation
- ❌ Tests fail → Investigate issues, revise approach

---

## 🔧 Configuration

### Database Connection

Edit `DB_CONFIG` in test files to match your PostgreSQL setup:

```python
DB_CONFIG = {
    "database": "postgres",       # Your database name
    "host": "localhost",          # Database host
    "port": 5432,                 # Database port
    "username": "postgres",       # Your username
    "password": None,             # Your password (or None)
    "max_connections": 10,        # Pool size
}
```

### Test Database Setup

```sql
-- Create test database (optional)
CREATE DATABASE fraiseql_test;

-- Grant permissions
GRANT ALL PRIVILEGES ON DATABASE fraiseql_test TO postgres;
```

---

## 🐛 Troubleshooting

### Test Failures

**"Cannot connect to PostgreSQL"**:
```bash
# Check if PostgreSQL is running
psql -h localhost -U postgres -c "SELECT 1"

# Check connection details
psql postgresql://postgres@localhost/postgres
```

**"fraiseql_rs module not found"**:
```bash
# Rebuild Rust extension
cd fraiseql_rs
maturin develop --release

# Verify import works
python -c "from fraiseql._fraiseql_rs import PrototypePool; print('✅ OK')"
```

**"Test hangs / times out"**:
- GIL deadlock detected
- Check `pyo3-async-runtimes` version matches `pyo3`
- Try running with `-s` flag to see where it hangs

**"Memory leak detected"**:
- Run with `tracemalloc` to identify source
- Check for connection pool leaks
- Verify `Arc` references are dropped

---

## 📝 Next Steps

### After Prototype Succeeds

1. **Document findings**:
   - Performance gains (actual speedup measured)
   - GIL handling patterns that work
   - Edge cases discovered

2. **Update implementation plan** based on learnings

3. **Proceed to Phase 1**: Full Tokio driver implementation

### If Prototype Fails

1. **Identify blockers**:
   - GIL deadlocks?
   - Performance regression?
   - Memory leaks?
   - Cancellation issues?

2. **Explore alternatives**:
   - Different async bridge (sync with thread pool?)
   - PyO3 without async (blocking calls?)
   - Different runtime (async-std?)

3. **Revise plan** based on findings

---

## 📚 Resources

- [PyO3 Async/Await Guide](https://pyo3.rs/latest/ecosystem/async-await)
- [pyo3-async-runtimes](https://github.com/awestlake87/pyo3-asyncio)
- [deadpool-postgres](https://docs.rs/deadpool-postgres)
- [tokio-postgres](https://docs.rs/tokio-postgres)

---

## ⚠️ Limitations

**This is a PROTOTYPE** - not production code:

- ❌ No SSL/TLS support (uses `NoTls`)
- ❌ Minimal error handling
- ❌ No connection pool configuration
- ❌ No health checks / monitoring
- ❌ No prepared statement caching
- ❌ No transaction support

These will be added in the full Phase 1-4 implementation.

---

**Last Updated**: 2026-01-01
**Status**: Ready for Testing
