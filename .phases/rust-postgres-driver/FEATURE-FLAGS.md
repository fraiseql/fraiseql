# Feature Flags: Gradual Rollout Strategy

**Document**: Feature flag implementation for Phases 1-4
**Created**: 2025-12-18
**Critical**: NO - But useful for risk mitigation
**Part of**: All phases 1-5

---

## Overview

Feature flags allow running both Rust and Python database backends in parallel, enabling:

- ✅ Gradual rollout without risk
- ✅ Easy rollback if issues found
- ✅ A/B testing between implementations
- ✅ Parity verification before full migration

---

## Cargo.toml Configuration

**File**: `fraiseql_rs/Cargo.toml`

```toml
[package]
name = "fraiseql_rs"
version = "0.1.0"

[features]
# Database backend features
rust-db = []           # Rust native database backend (DEFAULT)
python-db = ["psycopg"]  # Fall back to psycopg

# Default: use Rust backend
default = ["rust-db"]

# For testing: enable both simultaneously
dev = ["rust-db", "python-db"]

# Test feature flags
[dev-dependencies]
tokio = { version = "1.0", features = ["full"] }
```

---

## Rust Code with Feature Flags

### Connection Pool Module

**File**: `fraiseql_rs/src/db/pool.rs`

```rust
//! Connection pool with feature-gated backends

use pyo3::prelude::*;

#[cfg(feature = "rust-db")]
pub mod rust_impl {
    use super::*;
    use deadpool_postgres::Pool;
    use std::sync::Arc;

    pub struct ConnectionPool {
        pool: Arc<Pool>,
    }

    impl ConnectionPool {
        pub async fn new(url: &str) -> PyResult<Self> {
            // Rust implementation using tokio-postgres + deadpool
            let pool = Arc::new(
                create_pool(url)
                    .await
                    .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?
            );
            Ok(ConnectionPool { pool })
        }

        pub async fn get_connection(&self) -> PyResult<tokio_postgres::Client> {
            let client = self.pool
                .get()
                .await
                .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?;
            Ok(client)
        }
    }

    async fn create_pool(url: &str) -> Result<Pool, Box<dyn std::error::Error>> {
        let config = url.parse()?;
        let pool = deadpool_postgres::Pool::new(config, tokio_postgres::NoTls);
        Ok(pool)
    }
}

#[cfg(feature = "python-db")]
pub mod python_impl {
    use super::*;

    pub struct ConnectionPool {
        python_pool: PyObject,
    }

    impl ConnectionPool {
        pub async fn new(url: &str) -> PyResult<Self> {
            // Fall back to Python implementation
            Python::with_gil(|py| {
                let psycopg = py.import("psycopg_pool")?;
                let pool = psycopg.call1("SimpleConnectionPool", (url,))?;
                Ok(ConnectionPool {
                    python_pool: pool.into(),
                })
            })
        }

        pub async fn get_connection(&self) -> PyResult<PyObject> {
            Python::with_gil(|py| {
                let pool = self.python_pool.as_ref(py);
                pool.call_method0("getconn")
            })
        }
    }
}

// Export based on feature flags
#[cfg(feature = "rust-db")]
pub use rust_impl::ConnectionPool;

#[cfg(feature = "python-db")]
pub use python_impl::ConnectionPool;

#[cfg(all(feature = "rust-db", feature = "python-db"))]
compile_error!("Cannot enable both rust-db and python-db features simultaneously");

#[cfg(not(any(feature = "rust-db", feature = "python-db")))]
compile_error!("Must enable at least one database backend (rust-db or python-db)");
```

### Query Execution Module

**File**: `fraiseql_rs/src/db/query.rs`

```rust
//! Query execution with feature-gated backends

use pyo3::prelude::*;

#[cfg(feature = "rust-db")]
pub mod rust_impl {
    use super::*;

    pub async fn execute_query(sql: &str, params: &[&str]) -> PyResult<Vec<String>> {
        // Rust implementation
        Ok(vec![format!("Rust executed: {}", sql)])
    }
}

#[cfg(feature = "python-db")]
pub mod python_impl {
    use super::*;

    pub async fn execute_query(sql: &str, params: &[&str]) -> PyResult<Vec<String>> {
        // Python implementation using psycopg
        Python::with_gil(|py| {
            let psycopg = py.import("psycopg")?;
            // Call Python database code
            Ok(vec![format!("Python executed: {}", sql)])
        })
    }
}

#[cfg(feature = "rust-db")]
pub use rust_impl::execute_query;

#[cfg(feature = "python-db")]
pub use python_impl::execute_query;
```

---

## Python Configuration

**File**: `src/fraiseql/core/database.py`

```python
"""Database backend with feature flag support"""

import os
from typing import Dict, Any

# Check which backend to use
USE_RUST_BACKEND = os.getenv("FRAISEQL_DB_BACKEND", "rust").lower() == "rust"

# For dev/testing: can enable both
ENABLE_PARITY_TESTING = os.getenv("FRAISEQL_PARITY_TESTING", "false").lower() == "true"

class DatabaseBackend:
    """Abstraction layer for database backend selection"""

    def __init__(self):
        self.use_rust = USE_RUST_BACKEND
        self.parity_testing = ENABLE_PARITY_TESTING

        if self.use_rust:
            try:
                from _fraiseql_rs import execute_query_async
                self.rust_execute = execute_query_async
            except ImportError:
                raise RuntimeError("Rust backend enabled but fraiseql_rs not available")

        if self.parity_testing or not self.use_rust:
            from psycopg_pool import SimpleConnectionPool
            self.python_pool = SimpleConnectionPool(os.getenv("DATABASE_URL"))

    async def execute_query(self, query_def: Dict[str, Any]) -> Dict[str, Any]:
        """Execute query using configured backend"""

        if self.use_rust:
            result = await self.rust_execute(query_def)
        else:
            result = await self.python_execute(query_def)

        # Optionally run both and compare
        if self.parity_testing:
            rust_result = await self.rust_execute(query_def)
            python_result = await self.python_execute(query_def)

            if rust_result != python_result:
                raise RuntimeError(
                    f"Parity test failed!\nRust: {rust_result}\nPython: {python_result}"
                )

        return result

    async def python_execute(self, query_def: Dict[str, Any]) -> Dict[str, Any]:
        """Fallback: Python psycopg implementation"""
        # Implementation using psycopg
        pass
```

---

## Building with Feature Flags

### Build Rust Backend (Default)

```bash
# Build with Rust backend (default)
cd fraiseql_rs
cargo build --features rust-db

# Or just
cargo build  # Uses default = ["rust-db"]
```

### Build Python Backend (Fallback)

```bash
# Build with Python backend only
cd fraiseql_rs
cargo build --no-default-features --features python-db
```

### Build for Parity Testing

```bash
# Build with both backends for comparison
cd fraiseql_rs
cargo build --features "rust-db,python-db"
```

---

## Environment Variable Configuration

**File**: `.env.example`

```bash
# Database backend (rust or python)
FRAISEQL_DB_BACKEND=rust

# Enable parity testing (run both and compare)
FRAISEQL_PARITY_TESTING=false

# Performance comparison logging
FRAISEQL_LOG_PERFORMANCE=false

# Performance threshold (ms) - log queries slower than this
FRAISEQL_PERFORMANCE_THRESHOLD_MS=100
```

---

## Testing with Feature Flags

### Test Rust Backend Only

```bash
# Run tests with Rust backend
cargo test --features rust-db

# Or with environment variable
FRAISEQL_DB_BACKEND=rust cargo test
```

### Test Python Backend Only

```bash
# Run tests with Python backend
cargo test --no-default-features --features python-db
```

### Test Both (Parity Testing)

```bash
# Run tests with both backends enabled
FRAISEQL_PARITY_TESTING=true cargo test --features "rust-db,python-db"

# This will execute queries on both backends and compare results
```

### Run Full Test Suite Against Both Backends

**Script**: `scripts/test_both_backends.sh`

```bash
#!/bin/bash
# Test both Rust and Python backends, verify parity

set -e

echo "🧪 Testing Rust Backend..."
FRAISEQL_DB_BACKEND=rust uv run pytest tests/ -v
RUST_RESULT=$?

echo ""
echo "🧪 Testing Python Backend..."
FRAISEQL_DB_BACKEND=python uv run pytest tests/ -v
PYTHON_RESULT=$?

echo ""
echo "🧪 Testing Parity..."
FRAISEQL_PARITY_TESTING=true uv run pytest tests/regression/test_parity.py -v
PARITY_RESULT=$?

if [ $RUST_RESULT -eq 0 ] && [ $PYTHON_RESULT -eq 0 ] && [ $PARITY_RESULT -eq 0 ]; then
    echo "✅ All backend tests passed!"
    exit 0
else
    echo "❌ Some tests failed"
    exit 1
fi
```

---

## Rollout Phases

### Phase 1-2: Both Backends Available (Feature Flag)

```python
# Users or tests can choose backend
db = DatabaseBackend()  # Uses FRAISEQL_DB_BACKEND env var

# Or explicitly
from _fraiseql_rs import execute_query_async as rust_execute
# vs
from psycopg_pool import SimpleConnectionPool  # Python
```

### Phase 3-4: Rust Primary, Python Fallback

```python
# By default use Rust
FRAISEQL_DB_BACKEND=rust  # This is default

# Fallback if issues:
FRAISEQL_DB_BACKEND=python
```

### Phase 5: Rust Only (Remove Python Backend)

```toml
# In Cargo.toml
[features]
default = ["rust-db"]
# python-db feature removed entirely
```

---

## Monitoring & Logging

**File**: `fraiseql_rs/src/logging.rs`

```rust
pub fn log_query_execution(backend: &str, query: &str, duration_ms: f64) {
    if duration_ms > get_threshold_ms() {
        eprintln!(
            "⏱️  {} query took {:.2}ms: {}",
            backend, duration_ms, query
        );
    }
}

#[cfg(feature = "rust-db")]
pub fn compare_performance(rust_ms: f64, python_ms: f64) {
    let diff_percent = ((rust_ms - python_ms) / python_ms) * 100.0;
    println!(
        "📊 Rust: {:.2}ms, Python: {:.2}ms, Diff: {:.1}%",
        rust_ms, python_ms, diff_percent
    );
}
```

---

## CI/CD Integration

**File**: `.github/workflows/test-backends.yml`

```yaml
name: Test Both Backends

on:
  push:
    branches: [ dev ]

jobs:
  rust-backend:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - run: FRAISEQL_DB_BACKEND=rust uv run pytest tests/ -v

  python-backend:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - run: FRAISEQL_DB_BACKEND=python uv run pytest tests/ -v

  parity:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - run: FRAISEQL_PARITY_TESTING=true uv run pytest tests/regression/test_parity.py -v
```

---

## Troubleshooting

### "feature python-db not found"

**Issue**: Feature doesn't exist

**Fix**: Make sure feature is defined in Cargo.toml `[features]` section

---

### "Cannot enable both rust-db and python-db"

**Issue**: Compile error when both features enabled

**Fix**: This is intentional. For production:

```bash
cargo build --features rust-db      # ✅
cargo build --features python-db    # ✅

cargo build --features "rust-db,python-db"  # ❌ Not allowed
```

For testing parity, see test script above.

---

## Success Criteria

- ✅ `cargo build` uses Rust backend
- ✅ `FRAISEQL_DB_BACKEND=python` uses Python backend
- ✅ Tests pass with both backends
- ✅ Parity tests verify identical results
- ✅ Performance logging works
- ✅ Easy to toggle between backends

---

**Last Updated**: 2025-12-18
