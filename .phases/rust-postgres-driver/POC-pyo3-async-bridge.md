# PyO3 Async Bridge Proof of Concept

**Document**: Technical PoC for Python-Rust async integration
**Created**: 2025-12-18
**Critical**: YES - Most complex integration point
**Duration**: 4-6 hours to implement and validate
**Prerequisite**: Phase 0 setup complete

---

## Executive Summary

The Python-Rust async bridge using PyO3 is the riskiest technical component. This PoC validates:
- ✅ Can call async Rust code from Python
- ✅ Can return results through FFI boundary
- ✅ Can share connection pool across requests
- ✅ Error propagation works correctly
- ✅ Performance meets expectations

**If this PoC fails, the entire project is at risk.** Implement this before Phase 1.

---

## Why This Matters

**The Challenge**:
- Python has asyncio event loop
- Rust has tokio runtime
- PyO3 FFI boundary is unforgiving
- Connection pool must be Arc-wrapped and shared
- Errors crossing boundary need careful handling

**Failure Modes**:
- Runtime panics when async bridge misaligns
- Memory corruption from improper sharing
- Deadlocks when runtimes conflict
- Type conversion errors at boundary

---

## PoC Scope

### What to Validate

1. **Async Function Call from Python to Rust** ✅
   - Python code calls async Rust function
   - Returns Python coroutine
   - Python awaits result

2. **Connection Pool Sharing** ✅
   - Pool created once in Rust
   - Arc<Pool> accessible from Python
   - Multiple requests share same pool

3. **Type Conversion** ✅
   - Python dict → Rust struct
   - Rust result → Python dict
   - Error handling across boundary

4. **Performance** ✅
   - Overhead < 5% vs direct Rust
   - No memory leaks in iteration
   - GIL contention manageable

### What NOT to Include

- ❌ Full WHERE clause builder
- ❌ All PostgreSQL types
- ❌ Production error handling
- ❌ Query caching
- ❌ Transactions

---

## Implementation: Step by Step

### Step 1: Create Minimal Rust Module

**File**: `fraiseql_rs/src/pyo3_bridge.rs` (NEW)

```rust
//! PyO3 async bridge proof of concept
//!
//! Validates Python ↔ Rust async communication

use pyo3::prelude::*;
use pyo3_asyncio::tokio;
use std::sync::Arc;

/// Minimal connection pool for PoC
pub struct PooCPool {
    connection_count: usize,
}

impl PooCPool {
    pub fn new(size: usize) -> Self {
        PooCPool {
            connection_count: size,
        }
    }

    /// Simulate getting a connection
    async fn get_connection(&self) -> Result<String, String> {
        // Simulate async work
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        Ok(format!("connection_{}", self.connection_count))
    }

    /// Simulate query execution
    async fn execute_query(&self, sql: &str) -> Result<String, String> {
        // Simulate database query
        tokio::time::sleep(tokio::time::Duration::from_millis(20)).await;
        Ok(format!("Result of: {}", sql))
    }
}

/// Python-facing async function
#[pyfunction]
fn create_pool(py: Python, size: usize) -> PyResult<Py<PyAny>> {
    let pool = Arc::new(PooCPool::new(size));

    // Return a Python coroutine
    pyo3_asyncio::tokio::future_into_py(py, async move {
        // Simulate pool initialization
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        Ok(pool)
    })
}

/// Python function to execute query
#[pyfunction]
fn execute_async(py: Python, query: String) -> PyResult<Py<PyAny>> {
    pyo3_asyncio::tokio::future_into_py(py, async move {
        let pool = PooCPool::new(10);

        match pool.execute_query(&query).await {
            Ok(result) => Ok(PyString::new(py, &result).into()),
            Err(e) => Err(PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e)),
        }
    })
}

/// Module definition
#[pymodule]
#[pyo3(name = "_fraiseql_pyo3_bridge")]
fn pyo3_bridge(py: Python, m: &PyModule) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(create_pool, m)?)?;
    m.add_function(wrap_pyfunction!(execute_async, m)?)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pool_creation() {
        let pool = PooCPool::new(10);
        assert_eq!(pool.connection_count, 10);
    }

    #[tokio::test]
    async fn test_async_connection() {
        let pool = PooCPool::new(5);
        let conn = pool.get_connection().await;
        assert!(conn.is_ok());
    }

    #[tokio::test]
    async fn test_async_query() {
        let pool = PooCPool::new(5);
        let result = pool.execute_query("SELECT 1").await;
        assert!(result.is_ok());
    }
}
```

---

### Step 2: Expose in lib.rs

**File**: `fraiseql_rs/src/lib.rs`

```rust
pub mod pyo3_bridge;

// When building for Python
#[cfg(feature = "python-binding")]
pub use pyo3_bridge::*;
```

---

### Step 3: Update Cargo.toml

**File**: `fraiseql_rs/Cargo.toml`

```toml
[features]
python-binding = []
default = ["python-binding"]

[dependencies]
tokio = { version = "1.0", features = ["full"] }
tokio-postgres = "0.7"
deadpool-postgres = "0.14"
pyo3 = { version = "0.19", features = ["macros"] }
pyo3-asyncio = { version = "0.19", features = ["tokio-runtime"] }
```

---

### Step 4: Python Test Code

**File**: `tests/poc_pyo3_bridge.py` (NEW)

```python
"""
PoC test for PyO3 async bridge
Tests Python-Rust async communication
"""

import asyncio
import pytest
import sys
import os

# Add the built extension to path
sys.path.insert(0, os.path.join(os.path.dirname(__file__), '..', 'fraiseql_rs', 'target', 'debug'))

# Import the Rust module
try:
    import _fraiseql_pyo3_bridge as bridge
except ImportError as e:
    print(f"⚠️  Could not import bridge: {e}")
    print("Build with: cargo build -p fraiseql_rs --features python-binding")
    sys.exit(1)


class TestPyO3Bridge:
    """Test Python-Rust async bridge"""

    @pytest.mark.asyncio
    async def test_async_execute(self):
        """Test calling async Rust function from Python"""
        result = await bridge.execute_async("SELECT 1")
        assert "SELECT 1" in result
        print(f"✅ Async execute: {result}")

    @pytest.mark.asyncio
    async def test_multiple_calls(self):
        """Test multiple concurrent calls"""
        tasks = [
            bridge.execute_async(f"SELECT {i}")
            for i in range(5)
        ]
        results = await asyncio.gather(*tasks)
        assert len(results) == 5
        print(f"✅ Multiple calls: {len(results)} succeeded")

    @pytest.mark.asyncio
    async def test_error_handling(self):
        """Test error propagation from Rust to Python"""
        try:
            # This might fail with a simulated error
            result = await bridge.execute_async("")
            print(f"✅ Error handling: {result}")
        except RuntimeError as e:
            print(f"✅ Error propagated correctly: {e}")

    def test_sync_call(self):
        """Test that async functions return coroutines"""
        coro = bridge.execute_async("SELECT 1")
        assert asyncio.iscoroutine(coro)
        coro.close()  # Clean up
        print("✅ Returns coroutine correctly")

    @pytest.mark.asyncio
    async def test_concurrent_load(self):
        """Test concurrent load on async bridge"""
        async def make_query(i):
            return await bridge.execute_async(f"SELECT {i}")

        # 20 concurrent requests
        tasks = [make_query(i) for i in range(20)]
        results = await asyncio.gather(*tasks)

        assert len(results) == 20
        print(f"✅ Concurrent load: {len(results)} requests succeeded")

    @pytest.mark.asyncio
    async def test_performance(self):
        """Validate PoC performance"""
        import time

        start = time.perf_counter()
        result = await bridge.execute_async("SELECT 1")
        elapsed = (time.perf_counter() - start) * 1000

        # Should be fast (< 100ms including Python overhead)
        assert elapsed < 100
        print(f"✅ Performance: {elapsed:.2f}ms")


class TestAsyncBridgeLifecycle:
    """Test async bridge lifecycle"""

    @pytest.mark.asyncio
    async def test_repeated_calls(self):
        """Test that bridge handles repeated calls"""
        for i in range(10):
            result = await bridge.execute_async(f"Query {i}")
            assert f"Query {i}" in result
        print(f"✅ Repeated calls: 10 succeeded")

    @pytest.mark.asyncio
    async def test_memory_stability(self):
        """Test for memory leaks"""
        import gc

        gc.collect()

        # Make 1000 calls
        for i in range(1000):
            result = await bridge.execute_async(f"Query {i}")
            if i % 100 == 0:
                gc.collect()

        print("✅ Memory stability: 1000 calls succeeded")

    @pytest.mark.asyncio
    async def test_exception_handling(self):
        """Test exception handling across bridge"""
        try:
            # Try calling with invalid input
            await bridge.execute_async(None)
        except (TypeError, RuntimeError) as e:
            print(f"✅ Exception handling: {type(e).__name__}")
        except Exception as e:
            print(f"⚠️  Unexpected exception: {type(e).__name__}: {e}")


if __name__ == "__main__":
    # Run tests manually for debugging
    import asyncio

    async def manual_test():
        print("🧪 PyO3 Bridge Manual Tests\n")

        bridge_tests = TestPyO3Bridge()
        lifecycle_tests = TestAsyncBridgeLifecycle()

        # Test 1: Basic async call
        print("Test 1: Basic async call...")
        await bridge_tests.test_async_execute()

        # Test 2: Multiple calls
        print("\nTest 2: Multiple concurrent calls...")
        await bridge_tests.test_multiple_calls()

        # Test 3: Sync property
        print("\nTest 3: Sync call property...")
        bridge_tests.test_sync_call()

        # Test 4: Concurrent load
        print("\nTest 4: Concurrent load...")
        await bridge_tests.test_concurrent_load()

        # Test 5: Performance
        print("\nTest 5: Performance...")
        await bridge_tests.test_performance()

        # Test 6: Lifecycle
        print("\nTest 6: Lifecycle (repeated calls)...")
        await lifecycle_tests.test_repeated_calls()

        print("\n✅ All manual tests completed")

    asyncio.run(manual_test())
```

---

### Step 5: Build and Test

```bash
# Step 1: Build the Rust module
cd fraiseql_rs
cargo build --features python-binding
# Expected: ✅ Compilation succeeds

# Step 2: Install Python dependencies
cd ..
uv pip install pytest pytest-asyncio

# Step 3: Run manual test
python tests/poc_pyo3_bridge.py
# Expected: ✅ All manual tests pass

# Step 4: Run pytest
uv run pytest tests/poc_pyo3_bridge.py -v
# Expected: ✅ All pytest tests pass
```

---

## Validation Checklist

- ✅ Rust code compiles without warnings
- ✅ Python can import the module
- ✅ Async function returns coroutine
- ✅ Python can await the coroutine
- ✅ Result is correct
- ✅ Errors propagate correctly
- ✅ Multiple concurrent calls work
- ✅ Performance < 5% overhead
- ✅ No memory leaks (1000+ calls)
- ✅ Works from async function
- ✅ Works from sync function (wrapped)

---

## Expected Results

### If PoC Succeeds ✅

You should see:
```
✅ Async execute: Result of: SELECT 1
✅ Multiple calls: 5 succeeded
✅ Error handling: [error details]
✅ Returns coroutine correctly
✅ Concurrent load: 20 requests succeeded
✅ Performance: 23.45ms
✅ Repeated calls: 10 succeeded
✅ Memory stability: 1000 calls succeeded
✅ Exception handling: RuntimeError
```

→ **Proceed to Phase 1**

---

### If PoC Fails ❌

**Common Issues**:

1. **"Module not found"**
   ```bash
   # Make sure you built it
   cd fraiseql_rs && cargo build --features python-binding
   ```

2. **"RuntimeError: no running event loop"**
   ```python
   # Function must be called from async context
   # Wrap in asyncio.run() if needed
   asyncio.run(bridge.execute_async("SELECT 1"))
   ```

3. **"TypeError: expected coroutine"**
   ```python
   # Make sure you're awaiting
   result = await bridge.execute_async("SELECT 1")  # ✅
   result = bridge.execute_async("SELECT 1")         # ❌
   ```

4. **"Segfault or panic"**
   - This indicates FFI boundary issue
   - Check type conversions
   - Ensure Arc is properly shared
   - Run with `RUST_BACKTRACE=1` for details

---

## Troubleshooting

### Debug PyO3 Issues

```bash
# Get detailed error messages
RUST_BACKTRACE=full python tests/poc_pyo3_bridge.py

# Check module location
python -c "import _fraiseql_pyo3_bridge; print(_fraiseql_pyo3_bridge.__file__)"

# Verify async works
python -c "
import asyncio
from _fraiseql_pyo3_bridge import execute_async
async def test():
    result = await execute_async('SELECT 1')
    print(f'Result: {result}')
asyncio.run(test())
"
```

---

## Next: Integration into Phase 1

Once PoC succeeds, copy patterns to Phase 1:

1. **Connection Pool Module**
   ```rust
   // Copy Arc<Pool> pattern from PoC
   pub struct DatabasePool {
       pool: Arc<deadpool_postgres::Pool>,
   }
   ```

2. **Async Bridge Functions**
   ```rust
   // Copy pyo3_asyncio pattern from PoC
   #[pyfunction]
   fn execute_query_async(py: Python, ...) -> PyResult<Py<PyAny>> {
       pyo3_asyncio::tokio::future_into_py(py, async { ... })
   }
   ```

3. **Error Handling**
   ```rust
   // Copy error propagation from PoC
   Err(e) => Err(PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e))
   ```

---

## Success Definition

**PoC is successful when:**
- ✅ All tests pass
- ✅ No panics or segfaults
- ✅ Performance acceptable
- ✅ Code is understandable pattern for Phase 1
- ✅ Team feels confident about async bridge

**If any test fails:**
- Debug and fix before proceeding
- Don't proceed to Phase 1 with unknowns
- This is the foundation for everything else

---

## Timeline

**Estimated**: 4-6 hours
- Setup & dependencies: 30 min
- Write Rust code: 1.5 hours
- Python test code: 1.5 hours
- Build & test: 1 hour
- Debug & fix: 1-2 hours (if needed)

---

## Next Steps After Success

1. ✅ Commit PoC code to feature branch
2. ✅ Document any gotchas learned
3. ✅ Start Phase 1 with confidence
4. ✅ Reference PoC patterns in Phase 1 code

---

**Status**: Ready to implement NOW (before Phase 1)
**Criticality**: HIGHEST - Risk mitigation, not feature implementation
**Last Updated**: 2025-12-18
