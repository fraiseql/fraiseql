# Phase 9: Full Integration & Rust-Only Database Layer

**Phase**: 9 of 9 (FINAL)
**Effort**: 8 hours
**Status**: Ready to implement (after Phase 8 complete)
**Prerequisite**: Phase 8 - Query Caching complete

---

## Objective

Complete the migration to a full Rust database layer by unifying all phases into a single, optimized end-to-end execution pipeline:

1. Simplify Python interface to single async function call
2. Remove all Python database-related code (psycopg, SQL builders, etc)
3. Create unified Rust pipeline: Parse → Build → Cache → Execute → Transform
4. Finalize performance optimizations
5. Complete deprecation of Python database layer

**Success Criteria**:
- ✅ Python calls single function: `await execute_graphql_query(query, variables, user_context)`
- ✅ Rust handles entire pipeline end-to-end
- ✅ All 5991+ tests pass with zero regressions
- ✅ Performance: 5-10x overall improvement (10-20ms → 1-4ms per request)
- ✅ All psycopg code removed
- ✅ All Python SQL builders removed
- ✅ All Python WHERE clause code removed
- ✅ Zero Python database I/O code

---

## Architecture Overview

### Unified Rust Pipeline

```
HTTP Request
    ↓
Python FastAPI:
  ├─ Receive GraphQL query
  ├─ Extract user context
  └─ Call: execute_graphql_query(query, variables, user, pool_handle)
    ↓
Rust Core (Single Function):
  ├─ Phase 6: Parse query
  ├─ Phase 7: Build SQL (with Phase 8 caching)
  ├─ Phase 1: Execute with connection from pool
  ├─ Phase 3: Stream results from database
  ├─ Phase 3+4: Transform to JSON + GraphQL response
  └─ Return: Complete JSON response bytes
    ↓
Python FastAPI:
  └─ Send bytes directly to HTTP client
```

### Single Entry Point

```rust
/// Complete end-to-end GraphQL execution in Rust.
#[pyfunction]
pub async fn execute_graphql_query(
    py: Python,
    query_string: String,
    variables: PyDict,
    user_context: PyDict,
) -> PyResult<PyBytes> {
    // All work done in Rust - return complete response
}
```

---

## Implementation Steps

### Step 1: Create Unified Pipeline

**File**: `fraiseql_rs/src/pipeline/mod.rs` (NEW)

```rust
//! Unified GraphQL execution pipeline.

use crate::graphql::parser::parse_query;
use crate::query::composer::SQLComposer;
use crate::db::pool::DatabasePool;
use crate::response::builder::ResponseBuilder;
use anyhow::Result;
use pyo3::prelude::*;

pub struct GraphQLPipeline {
    pool: Arc<DatabasePool>,
    schema: SchemaMetadata,
    cache: Arc<QueryPlanCache>,
}

impl GraphQLPipeline {
    pub fn new(
        pool: Arc<DatabasePool>,
        schema: SchemaMetadata,
        cache: Arc<QueryPlanCache>,
    ) -> Self {
        Self { pool, schema, cache }
    }

    /// Execute complete GraphQL query end-to-end.
    pub async fn execute(
        &self,
        query_string: &str,
        variables: HashMap<String, serde_json::Value>,
        user_context: UserContext,
    ) -> Result<Vec<u8>> {
        // Phase 6: Parse GraphQL
        let parsed_query = parse_query(query_string)?;

        // Phase 7 + 8: Build SQL (with caching)
        let signature = crate::cache::signature::generate_signature(&parsed_query);
        let sql = if let Some(cached) = self.cache.get(&signature)? {
            cached.sql_template
        } else {
            let composer = SQLComposer::new(self.schema.clone());
            let composed = composer.compose(&parsed_query)?;
            self.cache.put(signature, composed.sql.clone())?;
            composed.sql
        };

        // Phase 1: Get connection from pool
        let conn = self.pool.get_connection().await?;

        // Phase 2 + 3: Execute query and stream results
        let rows = conn.query(&sql, &[]).await?;

        // Phase 3 + 4: Transform to GraphQL response
        let mut response_builder = ResponseBuilder::new();
        for row in rows {
            let json_str: String = row.get(0);
            response_builder.add_row(&json_str)?;
        }

        // Return complete response bytes
        Ok(response_builder.build()?)
    }
}

#[pyclass]
pub struct PyGraphQLPipeline {
    pipeline: Arc<GraphQLPipeline>,
}

#[pymethods]
impl PyGraphQLPipeline {
    #[pyo3(name = "execute")]
    pub fn execute_py(
        &self,
        py: Python,
        query_string: String,
        variables: PyDict,
        user_context: PyDict,
    ) -> PyResult<Py<PyAny>> {
        use pyo3_asyncio::tokio;

        let pipeline = self.pipeline.clone();
        let vars = dict_to_hashmap(&variables)?;
        let user = dict_to_user_context(&user_context)?;

        tokio::future_into_py(py, async move {
            let result = pipeline.execute(&query_string, vars, user).await
                .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?;

            Ok(PyBytes::new(py, &result).to_object(py))
        })
    }
}
```

### Step 2: Create Global Pipeline Instance

**File**: `fraiseql_rs/src/lib.rs` (MODIFY)

```rust
use std::sync::Arc;
use lazy_static::lazy_static;

lazy_static! {
    static ref GLOBAL_PIPELINE: Arc<Mutex<Option<PyGraphQLPipeline>>> =
        Arc::new(Mutex::new(None));
}

/// Initialize the global GraphQL pipeline (called from Python on startup).
#[pyfunction]
pub fn initialize_pipeline(
    py: Python,
    pool: &PyAny,
    schema_json: String,
) -> PyResult<()> {
    // Deserialize schema
    let schema: SchemaMetadata = serde_json::from_str(&schema_json)
        .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))?;

    // Create pipeline
    let pool_arc = Arc::new(/* wrap PyAny pool */);
    let cache = Arc::new(QueryPlanCache::new(5000));

    let pipeline = PyGraphQLPipeline {
        pipeline: Arc::new(GraphQLPipeline::new(pool_arc, schema, cache)),
    };

    // Store globally
    *GLOBAL_PIPELINE.lock().unwrap() = Some(pipeline);

    Ok(())
}

/// Execute GraphQL query using global pipeline.
#[pyfunction]
pub fn execute_graphql_query(
    py: Python,
    query_string: String,
    variables: PyDict,
    user_context: PyDict,
) -> PyResult<Py<PyAny>> {
    let pipeline = GLOBAL_PIPELINE.lock().unwrap();
    match &*pipeline {
        Some(p) => p.execute_py(py, query_string, variables, user_context),
        None => Err(PyErr::new::<pyo3::exceptions::RuntimeError, _>(
            "Pipeline not initialized"
        )),
    }
}
```

### Step 3: Update Python FastAPI Router

**File**: `src/fraiseql/fastapi/routers.py` (MODIFY)

```python
# OLD CODE (remove):
# async def graphql_endpoint(...):
#     # parse graphql
#     # normalize where
#     # build sql
#     # execute
#     # transform

# NEW CODE:
from fraiseql._fraiseql_rs import execute_graphql_query

@app.post("/graphql")
async def graphql_endpoint(request: GraphQLRequest) -> Response:
    """Execute GraphQL query (all work done in Rust)."""

    # Call unified Rust pipeline
    result_bytes = await execute_graphql_query(
        query_string=request.query,
        variables=request.variables or {},
        user_context={
            "user_id": request.context.user.id,
            "permissions": request.context.user.permissions,
        }
    )

    # Return bytes directly
    return Response(
        content=result_bytes,
        media_type="application/json"
    )
```

### Step 4: Cleanup - Remove Python Database Code

Create a cleanup script to remove deprecated code:

**File**: `scripts/cleanup_python_db.sh` (NEW)

```bash
#!/bin/bash
# Remove Python database layer code

# Remove Python SQL builders
rm -f src/fraiseql/sql/sql_generator.py
rm -f src/fraiseql/sql/where_generator.py
rm -f src/fraiseql/sql/order_by_generator.py
rm -f src/fraiseql/sql/limit_generator.py
rm -rf src/fraiseql/sql/where/

# Remove Python WHERE normalization
rm -f src/fraiseql/where_normalization.py
rm -f src/fraiseql/where_clause.py

# Remove Python GraphQL parsing (now in Rust)
rm -f src/fraiseql/graphql/execute.py (keep minimal wrapper)

# Remove psycopg pool management
rm -f src/fraiseql/fastapi/app.py::create_db_pool()

# Remove unused imports
grep -r "from psycopg" src/ | cut -d: -f1 | sort -u | xargs -I {} sed -i '/from psycopg/d' {}
grep -r "import psycopg" src/ | cut -d: -f1 | sort -u | xargs -I {} sed -i '/import psycopg/d' {}

echo "✓ Python database layer cleanup complete"
```

### Step 5: Update Dependencies

**File**: `pyproject.toml` (MODIFY)

```toml
[tool.poetry.dependencies]
# REMOVE:
# psycopg = {extras = ["binary"], version = ">=3.2.6"}
# psycopg-pool = ">=3.2.6"

# ... other dependencies remain ...
```

### Step 6: Create Integration Tests

**File**: `tests/test_full_pipeline.py` (NEW)

```python
"""Tests for unified Rust GraphQL pipeline."""

import pytest
from httpx import AsyncClient
from fraiseql.fastapi.app import app


@pytest.fixture
async def client():
    async with AsyncClient(app=app, base_url="http://test") as c:
        yield c


@pytest.mark.asyncio
async def test_simple_query(client):
    """Test simple GraphQL query through full pipeline."""
    query = """
    query {
        users {
            id
            firstName
        }
    }
    """

    response = await client.post(
        "/graphql",
        json={"query": query}
    )

    assert response.status_code == 200
    data = response.json()
    assert "data" in data
    assert "users" in data["data"]


@pytest.mark.asyncio
async def test_query_with_where(client):
    """Test query with WHERE clause."""
    query = """
    query {
        users(where: {status: "active"}) {
            id
            name
        }
    }
    """

    response = await client.post(
        "/graphql",
        json={"query": query}
    )

    assert response.status_code == 200
    data = response.json()
    assert "data" in data


@pytest.mark.asyncio
async def test_query_with_pagination(client):
    """Test pagination arguments."""
    query = """
    query {
        users(limit: 10, offset: 5) {
            id
        }
    }
    """

    response = await client.post(
        "/graphql",
        json={"query": query}
    )

    assert response.status_code == 200


@pytest.mark.asyncio
async def test_mutation(client):
    """Test mutation execution."""
    query = """
    mutation {
        createUser(input: {name: "John"}) {
            id
            name
        }
    }
    """

    response = await client.post(
        "/graphql",
        json={"query": query}
    )

    assert response.status_code == 200


@pytest.mark.asyncio
async def test_query_with_variables(client):
    """Test query with variables."""
    query = """
    query GetUsers($where: UserWhere!) {
        users(where: $where) {
            id
        }
    }
    """

    response = await client.post(
        "/graphql",
        json={
            "query": query,
            "variables": {
                "where": {"status": "active"}
            }
        }
    )

    assert response.status_code == 200


@pytest.mark.asyncio
async def test_nested_fields(client):
    """Test nested field selection."""
    query = """
    query {
        users {
            id
            equipment {
                name
                status
            }
        }
    }
    """

    response = await client.post(
        "/graphql",
        json={"query": query}
    )

    assert response.status_code == 200
    data = response.json()
    assert "equipment" in str(data)


@pytest.mark.asyncio
async def test_error_handling(client):
    """Test error handling."""
    query = "query { invalidField { id } }"

    response = await client.post(
        "/graphql",
        json={"query": query}
    )

    assert response.status_code == 400
    data = response.json()
    assert "errors" in data
```

### Step 7: Benchmark Suite

**File**: `benches/full_pipeline.rs` (NEW)

```rust
//! Benchmarks for unified Rust pipeline.

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use fraiseql_rs::pipeline::GraphQLPipeline;

fn benchmark_simple_query(c: &mut Criterion) {
    c.bench_function("simple_query", |b| {
        b.to_async(tokio::runtime::Runtime::new().unwrap()).iter(|| async {
            let pipeline = create_test_pipeline();
            pipeline.execute(
                black_box("query { users { id } }"),
                black_box(HashMap::new()),
                black_box(create_test_user()),
            ).await
        });
    });
}

fn benchmark_complex_where(c: &mut Criterion) {
    c.bench_function("complex_where_clause", |b| {
        b.to_async(tokio::runtime::Runtime::new().unwrap()).iter(|| async {
            let pipeline = create_test_pipeline();
            pipeline.execute(
                black_box("query { users(where: {AND: [{status: \"active\"}, {role: \"admin\"}]}) { id } }"),
                black_box(HashMap::new()),
                black_box(create_test_user()),
            ).await
        });
    });
}

fn benchmark_cached_query(c: &mut Criterion) {
    c.bench_function("cached_query", |b| {
        b.to_async(tokio::runtime::Runtime::new().unwrap()).iter(|| async {
            let pipeline = create_test_pipeline();
            // Run twice to hit cache on second run
            let _ = pipeline.execute(
                black_box("query { users { id } }"),
                black_box(HashMap::new()),
                black_box(create_test_user()),
            ).await;

            pipeline.execute(
                black_box("query { users { id } }"),
                black_box(HashMap::new()),
                black_box(create_test_user()),
            ).await
        });
    });
}

criterion_group!(
    benches,
    benchmark_simple_query,
    benchmark_complex_where,
    benchmark_cached_query
);
criterion_main!(benches);
```

Run benchmarks:
```bash
cd fraiseql_rs && cargo bench
```

---

## Migration Checklist

### Pre-Migration
- [ ] All Phase 1-8 tests passing
- [ ] All 5991+ existing tests passing
- [ ] Performance baseline established

### Migration
- [ ] Initialize Rust pipeline on app startup
- [ ] Update FastAPI router to call unified function
- [ ] Update all GraphQL endpoints
- [ ] Remove deprecated Python code
- [ ] Run full test suite

### Post-Migration
- [ ] All 5991+ tests still passing
- [ ] Zero regressions detected
- [ ] Performance benchmarks confirm improvements
- [ ] Monitor error rates (should be 0% delta)
- [ ] Production deployment

---

## Performance Summary

### Before (Python + Rust):
```
GraphQL Parse (graphql-core):    40-60µs    (Python C ext)
Python SQL generation:           2-4ms      (string + dict ops)
SQL execute (psycopg):           5-10ms     (network + DB)
Rust JSON transform:             0.5-1ms    (fast)
Total per request:               ~10-20ms
```

### After (Full Rust):
```
Parse (graphql-parser):          20-30µs    (pure Rust)
SQL generation (cached):         1-10µs     (cache hit) / 50-100µs (miss)
SQL execute (tokio-postgres):    5-10ms     (same, network bottleneck)
JSON transform (Rust pipeline):  0.2-0.5ms  (zero-copy)
Total per request:               ~5-11ms    (with caching ~6-8ms)
```

### Real-World Impact (100 req/s workload):
- **Before**: 1000ms+ total time
- **After**: 600-800ms total time
- **Improvement**: 1.5-2x overall (5-10x on compute, 0x on network/DB)

---

## Cleanup & Finalization

### Remove Deprecated Code
```bash
# Phase 6 cleanup: Remove Python GraphQL parsing
rm -rf src/fraiseql/graphql/

# Phase 7 cleanup: Remove Python SQL generation
rm -rf src/fraiseql/sql/

# Phase 1 cleanup: Remove psycopg pool
rm -f src/fraiseql/fastapi/app.py::create_db_pool()

# Cleanup: Remove Python database module
rm -f src/fraiseql/db.py
```

### Update Documentation
```bash
# Update all docs to reflect Rust-based architecture
sed -i 's/psycopg/tokio-postgres/g' docs/architecture/**/*.md
sed -i 's/Python database layer/Rust database layer/g' docs/**/*.md
```

### Final Verification
```bash
# Run full test suite
pytest -v

# Run benchmarks
cargo bench

# Check for any remaining Python DB imports
grep -r "from psycopg\|import psycopg\|from fraiseql.db\|from fraiseql.sql" src/

# Should return 0 matches
```

---

## Files Created/Modified

| File | Status | Purpose |
|------|--------|---------|
| `fraiseql_rs/src/pipeline/mod.rs` | New | Unified execution pipeline |
| `fraiseql_rs/src/lib.rs` | Modified | Global pipeline instance |
| `src/fraiseql/fastapi/routers.py` | Modified | Simplified endpoint |
| `src/fraiseql/fastapi/app.py` | Modified | Remove DB pool setup |
| `pyproject.toml` | Modified | Remove psycopg dependency |
| `tests/test_full_pipeline.py` | New | Integration tests |
| `benches/full_pipeline.rs` | New | Performance benchmarks |
| `scripts/cleanup_python_db.sh` | New | Cleanup script |

---

## Success Criteria - FINAL

### Functional
- ✅ All 5991+ tests pass (zero regressions)
- ✅ All existing GraphQL queries work identically
- ✅ All mutations work identically
- ✅ Error handling matches previous behavior
- ✅ No psycopg code remains

### Performance
- ✅ Query building: 10-80x faster (2-4ms → 50-200µs)
- ✅ Cached queries: 5-10x faster due to cache hits
- ✅ Overall requests: 1.5-2x faster (network is bottleneck)

### Code Quality
- ✅ Zero unsafe code (unless in critical paths)
- ✅ Full error handling with descriptive messages
- ✅ Comprehensive logging and metrics
- ✅ Memory efficient (< 100MB cache)

### Operational
- ✅ Easy deployment (single binary)
- ✅ Monitoring and observability
- ✅ Graceful error handling
- ✅ Zero breaking changes for users

---

## Deployment Strategy

### Phase 1: Canary (5% traffic)
```python
# Route 5% of requests to Rust pipeline
# Monitor for errors, latency, memory usage
```

### Phase 2: Gradual Rollout (25% → 50% → 100%)
```python
# Increase traffic percentage as confidence grows
# Monitor performance metrics
# Keep Python pipeline as fallback
```

### Phase 3: Full Cutover
```python
# All traffic on Rust pipeline
# Remove Python database code
# Simplify codebase
```

---

## What's Next?

After Phase 9 (Full Integration) is complete:

1. **Monitoring & Observability**: Add Prometheus metrics, distributed tracing
2. **Advanced Caching**: Query result caching (not just plans)
3. **Subscriptions**: Real-time updates via WebSocket
4. **Batching**: Multiple queries in single request
5. **APQ Enhancement**: Persisted query optimization
6. **Performance**: Further optimizations based on production data

---

*End of Phase 9: Full Rust GraphQL Database Layer*
