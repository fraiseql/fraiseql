# Comprehensive Testing Strategy

**Document**: Testing & Quality Assurance Guide
**Created**: 2025-12-18
**Applies to**: All phases (0-5)

---

## Overview

This document defines the testing strategy for the Rust PostgreSQL driver migration. It covers:
- Test architecture and organization
- Test types and when to use each
- Parity testing (Rust vs psycopg)
- Performance regression detection
- Code quality gates
- Coverage targets

**Success Definition**:
- ✅ All 5991+ existing tests pass with Rust backend
- ✅ Zero performance regressions (< 5% deviation)
- ✅ 100% code coverage of new Rust code
- ✅ Clippy passes with zero warnings (strict mode)

---

## Test Architecture

### Test Pyramid

```
                    ▲
                   / \
                  /   \    E2E Tests (10%)
                 /─────\   - Full GraphQL queries
                /       \  - Real database
               /─────────\
              /           \  Integration Tests (30%)
             / ─ ─ ─ ─ ─ ─ \ - Module communication
            /               \ - Connection pool
           / ─ ─ ─ ─ ─ ─ ─ ─ \
          /                   \ Unit Tests (60%)
         / ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ \ - Individual functions
        /___________________________\ - No external deps
```

### Test Organization

```
fraiseql_rs/
├── src/
│   ├── db/
│   │   ├── mod.rs
│   │   ├── pool.rs
│   │   │   └── [inline tests]
│   │   ├── query.rs
│   │   │   └── [inline tests]
│   │   └── types.rs
│   │       └── [inline tests]
│   └── ...
│
├── tests/
│   ├── common/
│   │   └── mod.rs          # Shared test utilities
│   │
│   ├── unit/               # Fast, no DB
│   │   ├── mod.rs
│   │   ├── db_types.rs
│   │   ├── json_transform.rs
│   │   └── query_param.rs
│   │
│   ├── integration/        # Requires DB
│   │   ├── mod.rs
│   │   ├── pool_tests.rs
│   │   ├── query_tests.rs
│   │   ├── where_clause_tests.rs
│   │   └── streaming_tests.rs
│   │
│   ├── e2e/               # Full GraphQL
│   │   ├── mod.rs
│   │   ├── graphql_queries.rs
│   │   └── graphql_mutations.rs
│   │
│   └── performance/       # Benchmarks
│       └── benches/
│           ├── connection_pool.rs
│           ├── query_execution.rs
│           └── streaming.rs
```

---

## Test Types & Strategies

### 1. Unit Tests (60% of tests)

**Purpose**: Test individual functions in isolation

**Location**: Inline in `src/` files + `tests/unit/`

**Example**:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_snake_to_camel_case() {
        assert_eq!(to_camel_case("user_id"), "userId");
        assert_eq!(to_camel_case("_private"), "_private");
    }

    #[test]
    #[should_panic]
    fn test_invalid_input_panics() {
        let _ = parse_dangerous_input("DROP TABLE");
    }
}
```

**Coverage Target**: ≥ 85%

**Tools**:
```bash
# Generate coverage
cargo tarpaulin --out Html

# Exclude specific modules
cargo tarpaulin --exclude-files fraiseql_rs/examples/*
```

### 2. Integration Tests (30% of tests)

**Purpose**: Test module interactions and database operations

**Location**: `tests/integration/`

**Categories**:

#### A. Connection Pool Tests
```rust
#[tokio::test]
async fn test_concurrent_connections() {
    // Setup
    let pool = create_test_pool(max_connections: 5).await;
    let mut handles = vec![];

    // Action: Spawn 10 concurrent tasks (more than pool size)
    for i in 0..10 {
        let pool_clone = pool.clone();
        let handle = tokio::spawn(async move {
            let conn = pool_clone.acquire_connection().await
                .expect("Should acquire or wait");

            // Use connection
            conn.execute("SELECT 1", &[]).await.expect("Query should succeed");
        });
        handles.push(handle);
    }

    // Assert: All should complete without deadlock
    for handle in handles {
        handle.await.expect("Task should complete");
    }
}

#[tokio::test]
async fn test_stale_connection_detection() {
    let pool = create_test_pool(max_connections: 5).await;

    // Acquire connection
    let mut conn = pool.acquire_connection().await.expect("Acquire failed");

    // Simulate stale connection by dropping database connection
    postgres::execute_system_command("systemctl restart postgres").await;

    // Pool should detect and recover
    let result = conn.execute("SELECT 1", &[]).await;
    assert!(result.is_err(), "Should detect stale connection");

    // Next acquire should succeed (new connection)
    let new_conn = pool.acquire_connection().await.expect("Should get fresh connection");
    let result = new_conn.execute("SELECT 1", &[]).await;
    assert!(result.is_ok(), "Should succeed with new connection");
}

#[tokio::test]
async fn test_connection_timeout() {
    let pool = create_test_pool(
        max_connections: 1,
        timeout_ms: 100,
    ).await;

    // Acquire first connection
    let _conn1 = pool.acquire_connection().await.expect("First acquire");

    // Try to acquire second (should timeout)
    let result = tokio::time::timeout(
        Duration::from_millis(200),
        pool.acquire_connection(),
    ).await;

    assert!(result.is_err(), "Should timeout waiting for connection");
}
```

#### B. Query Execution Tests
```rust
#[tokio::test]
async fn test_simple_select() {
    let db = setup_test_db().await;

    let rows = db.query("SELECT * FROM users LIMIT 1", &[])
        .await
        .expect("Query should succeed");

    assert!(!rows.is_empty(), "Should have result");
}

#[tokio::test]
async fn test_parameter_binding() {
    let db = setup_test_db().await;

    let rows = db.query(
        "SELECT * FROM users WHERE id = $1",
        &[&42],
    ).await.expect("Query should succeed");

    assert!(!rows.is_empty(), "Should find user");
    let id: i32 = rows[0].get("id");
    assert_eq!(id, 42);
}

#[tokio::test]
async fn test_transaction_rollback() {
    let db = setup_test_db().await;

    let mut tx = db.begin_transaction().await
        .expect("Begin transaction");

    // Insert
    tx.execute("INSERT INTO users (name) VALUES ('test')", &[])
        .await
        .expect("Insert should succeed");

    // Verify insert is visible within transaction
    let rows = tx.query("SELECT COUNT(*) FROM users", &[]).await.ok();
    assert!(rows.is_some());

    // Rollback
    tx.rollback().await.expect("Rollback should succeed");

    // Verify rollback worked
    let rows_after = db.query("SELECT * FROM users WHERE name = 'test'", &[])
        .await
        .expect("Query after rollback");

    assert!(rows_after.is_empty(), "Insert should be rolled back");
}
```

#### C. WHERE Clause Tests
```rust
#[test]
fn test_where_parity_with_python() {
    // Generate WHERE clause in Rust
    let filters = json!({
        "user_id": {"eq": 42},
        "status": {"in": ["active", "pending"]},
    });

    let (sql_rust, params_rust) = build_where_clause_rust("users", &filters)
        .expect("Build should succeed");

    // Compare with Python version
    let (sql_python, params_python) = build_where_clause_python("users", &filters)
        .expect("Build should succeed");

    // SQL might differ in order, so normalize
    let sql_rust_norm = normalize_sql(&sql_rust);
    let sql_python_norm = normalize_sql(&sql_python);

    assert_eq!(sql_rust_norm, sql_python_norm, "SQL should be equivalent");
    assert_eq!(params_rust, params_python, "Parameters should match exactly");
}

#[test]
fn test_where_edge_cases() {
    let test_cases = vec![
        (
            json!({"id": {"eq": null}}),
            "Should handle NULL",
        ),
        (
            json!({"array_field": {"in": []}}),
            "Should handle empty array",
        ),
        (
            json!({"nested": {"or": [{"eq": 1}, {"ne": 2}]}}),
            "Should handle nested OR",
        ),
    ];

    for (filters, description) in test_cases {
        let result = build_where_clause("users", &filters);
        assert!(result.is_ok(), "Should handle {}", description);
    }
}
```

#### D. Streaming Tests
```rust
#[tokio::test]
async fn test_streaming_large_result_set() {
    let db = setup_test_db().await;

    // Insert 10,000 rows
    for i in 0..10_000 {
        db.execute(
            "INSERT INTO test_data (id, value) VALUES ($1, $2)",
            &[&i, &format!("value_{}", i)],
        ).await.ok();
    }

    // Stream results
    let mut stream = db.stream_query("SELECT * FROM test_data", &[]).await
        .expect("Stream should start");

    let mut count = 0;
    while let Some(row) = stream.next().await {
        let _row = row.expect("Row should be valid");
        count += 1;
    }

    // All rows should be retrieved
    assert_eq!(count, 10_000, "All rows should be streamed");
}

#[tokio::test]
async fn test_streaming_memory_usage() {
    use std::alloc::GlobalAlloc;

    let db = setup_test_db().await;

    // Get baseline memory
    let baseline = measure_memory().await;

    // Stream 100K rows
    let mut stream = db.stream_query("SELECT * FROM large_table", &[]).await
        .expect("Stream should start");

    let mut max_memory = baseline;
    while let Some(row) = stream.next().await {
        let _row = row.ok();
        let current = measure_memory().await;
        max_memory = max_memory.max(current);
    }

    // Memory increase should be < 50MB for streaming
    let increase = max_memory - baseline;
    assert!(increase < 50_000_000, "Memory increase should be reasonable");
}
```

### 3. Parity Tests (Regression Detection)

**Purpose**: Verify Rust implementation matches psycopg exactly

**Location**: `tests/regression/parity/`

**Strategy**:
```rust
#[tokio::test]
async fn test_query_result_parity() {
    let db_rust = create_rust_pool().await;
    let db_py = create_psycopg_pool().await;  // Fallback for comparison

    let query = "SELECT * FROM users ORDER BY id LIMIT 100";

    // Execute on both backends
    let result_rust = db_rust.query(query, &[]).await.expect("Rust query");
    let result_py = db_py.query(query, &[]).await.expect("Python query");

    // Compare results
    assert_eq!(
        serialize_rows(&result_rust),
        serialize_rows(&result_py),
        "Results should be identical"
    );
}

#[test]
fn test_where_clause_parity_extensive() {
    let complex_filters = vec![
        // Simple equality
        json!({"id": {"eq": 42}}),

        // Multiple operators
        json!({"id": {"gt": 10}, "name": {"like": "%john%"}}),

        // Nested AND
        json!({"status": {"and": [
            {"ne": "deleted"},
            {"ne": "archived"}
        ]}}),

        // Nested OR
        json!({"priority": {"or": [
            {"eq": "high"},
            {"eq": "urgent"}
        ]}}),

        // Complex nesting
        json!({"status": {"or": [
            {"eq": "active"},
            {"and": [{"eq": "pending"}, {"gt": 0}]}
        ]}}),
    ];

    for filters in complex_filters {
        let (sql_rust, params_rust) = build_where_rust("table", &filters)
            .expect("Rust build");
        let (sql_py, params_py) = build_where_python("table", &filters)
            .expect("Python build");

        let sql_rust_norm = normalize_sql(&sql_rust);
        let sql_py_norm = normalize_sql(&sql_py);

        assert_eq!(sql_rust_norm, sql_py_norm,
            "WHERE should match for: {:?}", filters);
        assert_eq!(params_rust, params_py,
            "Parameters should match for: {:?}", filters);
    }
}

#[tokio::test]
async fn test_mutation_result_parity() {
    let db_rust = create_rust_pool().await;
    let db_py = create_psycopg_pool().await;

    // Insert via Rust
    let result_rust = db_rust.execute(
        "INSERT INTO test (name) VALUES ('rust') RETURNING id",
        &[],
    ).await.expect("Insert via Rust");

    let id_rust = result_rust.rows_affected();

    // Insert via Python
    let result_py = db_py.execute(
        "INSERT INTO test (name) VALUES ('python') RETURNING id",
        &[],
    ).await.expect("Insert via Python");

    let id_py = result_py.rows_affected();

    // Both should return valid IDs
    assert!(id_rust > 0);
    assert!(id_py > 0);
}
```

### 4. Performance Tests (Benchmarks)

**Purpose**: Detect performance regressions

**Location**: `benches/`

**Strategy**:
```rust
// benches/query_performance.rs

use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};

fn benchmark_query_execution(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let pool = rt.block_on(async { create_test_pool().await });

    let mut group = c.benchmark_group("query_execution");

    // Setup test data
    rt.block_on(async {
        for i in 0..1000 {
            pool.execute(
                "INSERT INTO bench_data (id, value) VALUES ($1, $2)",
                &[&i, &format!("value_{}", i)],
            ).await.ok();
        }
    });

    group.bench_function("simple_select", |b| {
        b.to_async(&rt).iter(|| async {
            let rows = pool.query(
                black_box("SELECT * FROM bench_data WHERE id = $1"),
                black_box(&[&42]),
            ).await.expect("Query should succeed");

            black_box(rows)
        });
    });

    group.bench_function("where_complex", |b| {
        b.to_async(&rt).iter(|| async {
            let rows = pool.query(
                black_box("SELECT * FROM bench_data WHERE id > $1 AND id < $2 AND value LIKE $3"),
                black_box(&[&100, &900, &"%5%"]),
            ).await.expect("Query should succeed");

            black_box(rows)
        });
    });

    group.bench_function("large_result_stream", |b| {
        b.to_async(&rt).iter(|| async {
            let mut stream = pool.stream_query(
                black_box("SELECT * FROM bench_data"),
                &[],
            ).await.expect("Stream should start");

            let mut count = 0;
            while let Some(_row) = stream.next().await {
                count += 1;
            }

            black_box(count)
        });
    });

    group.finish();
}

criterion_group!(benches, benchmark_query_execution);
criterion_main!(benches);
```

**Running Benchmarks**:
```bash
# Establish baseline
cargo bench -- --save-baseline main

# Compare against baseline
cargo bench -- --baseline main

# Output
test query_execution::simple_select    ... bench: 5.234 ms/iter (+/- 0.123)
test query_execution::where_complex    ... bench: 8.456 ms/iter (+/- 0.234)
test query_execution::large_result_stream ... bench: 245.123 ms/iter (+/- 12.345)

# Regressions detected if > 10% variance
```

### 5. End-to-End Tests (10% of tests)

**Purpose**: Test full GraphQL pipeline

**Location**: Python integration tests (use Rust backend)

**Strategy**:
```python
# tests/integration/graphql/test_rust_backend.py

import pytest
from fraiseql import create_app_with_rust_backend

@pytest.fixture
async def app():
    return create_app_with_rust_backend()

@pytest.mark.asyncio
async def test_graphql_query_simple(app):
    """Test simple GraphQL query executes through Rust backend."""
    response = await app.execute("""
        query {
            users(limit: 10) {
                id
                name
                email
            }
        }
    """)

    assert response.status == 200
    assert response.data["data"]["users"] is not None
    assert len(response.data["data"]["users"]) <= 10

@pytest.mark.asyncio
async def test_graphql_mutation_insert(app):
    """Test GraphQL mutation (INSERT) through Rust backend."""
    response = await app.execute("""
        mutation {
            createUser(name: "Test User", email: "test@example.com") {
                id
                name
                email
            }
        }
    """)

    assert response.status == 200
    assert response.data["data"]["createUser"]["name"] == "Test User"

@pytest.mark.asyncio
async def test_graphql_with_complex_where(app):
    """Test GraphQL query with complex WHERE filters."""
    response = await app.execute("""
        query {
            users(
                where: {
                    status: {in: ["active", "pending"]}
                    createdAt: {gt: "2025-01-01"}
                }
                limit: 50
            ) {
                id
                name
            }
        }
    """)

    assert response.status == 200
    users = response.data["data"]["users"]
    assert all(u["id"] is not None for u in users)
```

---

## Code Quality Gates

### Clippy Configuration

**Strict mode** - all warnings are errors:

```toml
[lints.clippy]
all = "warn"
pedantic = "warn"
nursery = "warn"
unwrap_used = "warn"
expect_used = "warn"
panic = "warn"
unimplemented = "warn"
todo = "deny"  # ← Must resolve before merge
dbg_macro = "warn"
println_macro = "warn"
```

**Fixed warnings before merge**:
```bash
cargo clippy --fix --allow-dirty
cargo fmt
```

### Code Coverage

**Target**: ≥ 80% for new code

```bash
# Generate report
cargo tarpaulin --manifest-path fraiseql_rs/Cargo.toml --out Html

# View in browser
open tarpaulin-report.html
```

**Excluded from coverage**:
- Example files
- Test utilities
- Generated code

### Documentation

**All public APIs documented**:

```rust
/// Acquire a connection from the pool.
///
/// This method waits up to `connection_timeout_ms` for a connection.
///
/// # Errors
///
/// Returns an error if:
/// - The pool is exhausted
/// - Connection acquisition times out
/// - Database connection fails
///
/// # Examples
///
/// ```no_run
/// let pool = DatabasePool::new("postgres://...", None)?;
/// let conn = pool.acquire_connection().await?;
/// ```
pub async fn acquire_connection(&self) -> Result<Connection> {
    // ...
}
```

---

## CI/CD Integration

### GitHub Actions Workflow

```yaml
name: Quality Gates

on: [push, pull_request]

jobs:
  quality:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3

      # Clippy (must pass)
      - name: Clippy
        run: cargo clippy -- -D warnings

      # Tests (must pass)
      - name: Unit Tests
        run: cargo test --lib

      - name: Integration Tests
        run: cargo test --test '*'

      # Coverage (must be ≥ 80%)
      - name: Coverage
        run: |
          cargo tarpaulin --out Xml
          bash <(curl -s https://codecov.io/bash)

      # Benchmarks (detect regressions)
      - name: Benchmarks
        run: cargo bench -- --baseline main
        if: github.event_name == 'push'
```

---

## 🔄 Test Migration Strategy: Handling 5991+ Existing Python Tests

**Key Principle**: Don't port all tests - replace them as Rust code replaces Python code.

### Test Categorization

**~80% of tests stay as-is** (Integration & E2E tests):
```python
# These work through the Python API wrapper
# Backend (Python or Rust) is invisible to them

def test_graphql_query():
    """This test doesn't care if backend is Python or Rust"""
    result = schema.execute("{ users { id, name } }")
    assert result.data['users'][0]['name'] == 'Alice'
    # Works in Phase 0-4 (with feature flags)
    # Works in Phase 5 (Rust-only)
```

**~20% need updating or removal** (Unit tests of Python-specific code):
```python
# These test Python internals being replaced

from fraiseql.core.db import DatabaseConnection  # Removed in Phase 5
from fraiseql.where_builder import build_where  # Replaced by Rust in Phase 2

# These tests either:
# 1. Get removed when Python code is deleted (Phase 5)
# 2. Get updated to test Rust backend instead
# 3. Get replaced with Rust equivalents
```

### Migration by Phase

#### **Phase 0: Foundation**
```
Existing Python Tests: ✅ ALL 5991+ PASS
├─ No changes to Python code yet
├─ Rust infrastructure only (Clippy, benchmarks, tests)
└─ Both backends available via feature flags
```

#### **Phase 1: Connection Pool**
```
New Rust Tests: ✅ Connection pool unit tests (Rust)
Existing Python Tests: ✅ 5991+ PASS
├─ Python pool tests still run
├─ Feature flags: can test both backends
├─ Parity tests: Rust pool == psycopg pool
└─ Result: 5991+ + ~50 new Rust tests

Changes: None (Python code unchanged)
```

#### **Phase 2: Query Execution**
```
New Rust Tests: ✅ WHERE clause tests (Rust)
Existing Python Tests: ⚠️ 5991+ PASS (some redundant)
├─ Python WHERE tests still run (unchanged)
├─ Rust WHERE tests verify Rust implementation
├─ Parity tests: Rust WHERE == Python WHERE
├─ Python-only WHERE tests now redundant (but still pass)
└─ Result: 5991+ + ~100 new Rust tests

Changes: None (Python code still there, feature-flagged)
```

#### **Phase 3: Result Streaming**
```
New Rust Tests: ✅ Streaming unit tests (Rust)
Existing Python Tests: ⚠️ 5991+ PASS (some redundant)
├─ Python streaming tests still run
├─ Rust streaming tests verify Rust implementation
├─ Parity tests: Rust results == Python results
└─ Result: 5991+ + ~100 new Rust tests

Changes: None (Python code still there, feature-flagged)
```

#### **Phase 4: Full Integration**
```
New Rust Tests: ✅ GraphQL E2E tests (testing Rust backend)
Existing Python Tests: ✅ ALL 5991+ PASS (all backends tested)
├─ All integration tests work with Rust backend
├─ Parity tests: Full pipeline Rust == Python
├─ Both backends fully tested side-by-side
└─ Result: 5991+ Python integration tests + Rust unit/integration tests

Changes: None (Python code still there, feature-flagged)

**Critical**: All 5991+ existing tests pass with Rust backend
```

#### **Phase 5: Deprecation & Cleanup**
```
Removed Python Code:
├─ src/fraiseql/db.py (replaced by Rust)
├─ src/fraiseql/where_builder.py (replaced by Rust)
├─ src/fraiseql/streaming.py (replaced by Rust)
└─ Feature flags (only Rust backend now)

Tests Removed: ~50-100 tests
├─ psycopg-specific tests
├─ Python db.py unit tests
├─ Python WHERE builder unit tests
└─ Feature flag tests (no longer needed)

Tests Kept: ~5900 tests
├─ All GraphQL integration tests (work with Rust)
├─ All E2E tests (work with Rust)
├─ All schema tests (work with Rust)
└─ All API tests (work with Rust)

Result: Same coverage, all Rust-backed
```

### Test Count Summary

| Phase | Python Tests | Rust Tests | Total | Notes |
|-------|-------------|-----------|-------|-------|
| Phase 0 | 5991 | 0 | 5991 | No changes |
| Phase 1 | 5991 | ~50 | 6041 | Pool tests added |
| Phase 2 | 5991 | ~150 | 6141 | WHERE tests added |
| Phase 3 | 5991 | ~250 | 6241 | Streaming tests added |
| Phase 4 | 5991 | ~350 | 6341 | GraphQL E2E added |
| Phase 5 | 5900 | ~350 | 6250 | psycopg tests removed |

### Why We Don't Port All Python Tests

❌ **Porting would be wasteful** because:

1. **Integration tests work through API**
   - 80% of tests call `schema.execute()` or HTTP endpoints
   - They don't care about backend
   - No code changes needed - they just work

2. **Unit tests should be native to their language**
   - Python tests test Python code (being replaced)
   - Rust tests test Rust code (being added)
   - Different test paradigms, different testing libraries
   - Translation would be lossy

3. **Test pyramid changes**
   - Python pyramid: mostly tests of Python db.py internals
   - Rust pyramid: tests of Rust internals
   - Structure is different - can't just copy-paste

4. **Gradual replacement is safer**
   - Feature flags let both backends run simultaneously
   - Python tests validate Python path (until Phase 5)
   - Rust tests validate Rust path (starting Phase 1)
   - Parity tests verify they match
   - No abrupt "migration" - smooth transition

### Handling Python-Specific Unit Tests

**Example: WHERE clause builder tests**

Phase 0-4:
```python
# Python WHERE tests
def test_python_where_simple():
    from fraiseql.where_builder import build_where  # Python
    sql, params = build_where("users", {"id": {"eq": 42}})
    assert sql == "id = $1"
    assert params == [42]
    # ✅ Still passes (Python code still there)

# Rust WHERE tests (NEW in Phase 2)
#[test]
fn test_rust_where_simple() {
    let (sql, params) = build_where_rust("users", json!({"id": {"eq": 42}}));
    assert_eq!(sql, "id = $1");
    assert_eq!(params, vec![42]);
    // ✅ New Rust implementation tested
}

# Parity test (NEW in Phase 2)
#[test]
fn test_where_parity() {
    let python_result = call_python_where(...);
    let rust_result = build_where_rust(...);
    assert_eq!(python_result, rust_result);  // ✅ Must match
}
```

Phase 5 (after removing Python db.py):
```python
# Python WHERE tests removed (Python code deleted)
# Rust WHERE tests kept (Rust code remains)
# No parity tests (only one implementation now)
```

### Testing Both Backends (Phases 1-4)

**Use feature flags to test both**:

```bash
# Test Rust backend
FRAISEQL_DB_BACKEND=rust cargo test

# Test Python backend (fallback)
FRAISEQL_DB_BACKEND=python cargo test

# Test both backends match (parity testing)
FRAISEQL_PARITY_TESTING=true cargo test
```

### Success Criteria by Phase

| Phase | Python Tests | Rust Tests | Parity | Performance |
|-------|-------------|-----------|--------|-------------|
| 0 | 5991 pass | N/A | N/A | baseline |
| 1 | 5991 pass | 50 pass | ✓ pools match | < 10% overhead |
| 2 | 5991 pass | 150 pass | ✓ WHERE match | < 10% overhead |
| 3 | 5991 pass | 250 pass | ✓ results match | < 10% overhead |
| 4 | 5991 pass | 350 pass | ✓ all match | 20-30% faster |
| 5 | 5900 pass | 350 pass | N/A (Rust only) | 20-30% faster |

### Common Questions

**Q: Do I need to port test X to Rust?**

A: Ask these questions:
- Does it test Python `db.py` internals? → Remove in Phase 5
- Does it test the Python API wrapper? → Keep as-is (works with Rust backend)
- Does it test database functionality? → Keep as-is (parity guaranteed)
- Is it a GraphQL/HTTP test? → Keep as-is

**Q: Why do tests still pass if I'm replacing Python code?**

A: Because feature flags keep both backends running. Tests can choose which to use.

**Q: When do I remove the Python tests?**

A: Phase 5, when you delete the Python code they test. If a test doesn't test Python internals, keep it!

---

## Test Execution Timeline

### Phase 1: Foundation
```
- Unit tests: Pool types (100% coverage)
- Integration: Pool initialization, health check
- Time: 15 min
```

### Phase 2: Query Execution
```
- Unit tests: WHERE clause building (100% coverage)
- Integration: Query execution, parameter binding
- Parity: Rust vs psycopg WHERE output
- Time: 20 min
```

### Phase 3: Streaming
```
- Unit tests: JSON transformation (100% coverage)
- Integration: Streaming large results
- Performance: Memory usage benchmarks
- Time: 25 min
```

### Phase 4: Integration
```
- E2E: Full GraphQL queries
- Parity: All query/mutation types
- Performance: Throughput benchmarks
- Time: 30 min
```

### Phase 5: Deprecation
```
- Final regression: Full test suite with Rust
- Benchmark comparison: Phase 4 vs Phase 5
- Coverage: Verify no gaps
- Time: 20 min
```

---

## Performance Regression Detection

### Baseline Establishment

```bash
# After Phase 1 completion
cargo bench -- --save-baseline phase-1

# After each phase
cargo bench -- --save-baseline phase-2
cargo bench -- --save-baseline phase-3
cargo bench -- --save-baseline phase-4
```

### Regression Detection

```bash
# Compare current vs baseline
cargo bench -- --baseline phase-4

# Output shows variance
test simple_select ... bench: 5.234 ms/iter (+/- 0.123) [+5%] ⚠️
test where_complex ... bench: 8.456 ms/iter (+/- 0.234) [+2%] ✓
```

### Threshold Rules

| Regression | Action |
|-----------|--------|
| < 5% | ✅ Accept (normal variance) |
| 5-10% | ⚠️ Investigate (likely problem) |
| > 10% | ❌ Reject (regression) |

---

## Troubleshooting Failed Tests

### Test Hangs (Deadlock)

```bash
# Add timeout
timeout 30s cargo test -- --test-threads=1

# Run with backtrace
RUST_BACKTRACE=1 cargo test -- --nocapture
```

### Flaky Tests

```rust
// Retry mechanism
#[test]
fn test_flaky_connection() {
    for attempt in 1..=3 {
        match try_connect() {
            Ok(conn) => return assert!(conn.is_valid()),
            Err(_) if attempt < 3 => continue,
            Err(e) => panic!("Failed after 3 attempts: {}", e),
        }
    }
}
```

### Database Not Ready

```bash
# Wait for database
docker-compose up -d postgres
sleep 5
cargo test
```

---

## Success Criteria Summary

✅ **Code Quality**:
- Clippy: 0 warnings (strict)
- Format: 100% (`cargo fmt`)
- Coverage: ≥ 80% new code
- Docs: All public APIs

✅ **Testing**:
- Unit: 100% of logic
- Integration: All modules
- E2E: Full GraphQL
- Parity: Rust == psycopg

✅ **Performance**:
- Regression: < 5%
- Memory: Stable
- Latency: < 100ms p99
- Throughput: 2-3x psycopg

✅ **CI/CD**:
- All workflows passing
- Benchmarks tracked
- Coverage reported
- No manual steps

---

**Status**: ✅ Complete Testing & Quality Strategy
**Ready for**: Phase 0 Implementation
