# Unified Rust Pipeline Architecture - Single FFI Entry Point

**Status**: Architecture Proposal
**Date**: January 8, 2026
**Scope**: Complete GraphQL execution in Rust with single FFI boundary

---

## Executive Summary

Currently, FraiseQL has **6 PyO3 FFI bindings** creating multiple Python↔Rust boundary crossings:

1. `initialize_graphql_pipeline()` - Schema & pipeline setup
2. `initialize_schema_registry()` - Schema registry initialization
3. `reset_schema_registry_for_testing()` - Test cleanup
4. `execute_query_async()` - GraphQL query execution
5. `execute_mutation_async()` - GraphQL mutation execution
6. `build_multi_field_response()` - Multi-field response building

**Proposed**: Single unified FFI entry point that accepts GraphQL query and returns HTTP response without crossing FFI boundary during request processing.

---

## Current Architecture (Multiple FFI Crossings)

```
Python HTTP Server (Axum not used)
    ↓ [FFI 1]
Rust: initialize_graphql_pipeline()
Rust: initialize_schema_registry()
    ↓ (stored in memory)
Python receives GraphQL query
    ↓ [FFI 2]
Rust: execute_query_async()
    ↓ (returns JSON results)
Python: build_multi_field_response() [FFI 3]
    ↓ (returns final JSON)
Python HTTP Response
```

**Problems**:
- Multiple GIL acquisitions per request
- Context switching overhead between Python and Rust
- Serialization/deserialization at each FFI boundary
- Python remains bottleneck for HTTP handling
- Cannot use Rust's HTTP server (Axum) for full request lifecycle

---

## Proposed Architecture (Single FFI Entry Point)

```
HTTP Request (Axum in Rust)
    ↓
Rust HTTP Handler (no FFI needed)
    ↓
Rust GraphQL Pipeline
    ├─ Query validation
    ├─ Schema lookup
    ├─ Field resolution
    ├─ Query building (QueryBuilder)
    ├─ Database execution (tokio-postgres)
    ├─ JSON transformation
    ├─ Response building
    └─ Multi-field composition
    ↓
HTTP Response (JSON)
    ↓
[FFI ONLY if: Configuration loading, schema updates]
```

---

## Key Benefits

### 1. **Zero FFI Per-Request Overhead**
- GIL acquired only once at startup (schema initialization)
- Zero FFI calls during HTTP request processing
- Direct Rust async execution with Tokio

### 2. **Performance Improvements**
- **Async all the way**: No blocking on GIL
- **HTTP handling in Rust**: Axum native request routing
- **Direct database access**: tokio-postgres without Python bridge
- **Memory efficiency**: No Python object marshalling per request
- **Throughput**: 2-5x improvement (typical FFI + GIL contention)

### 3. **Architectural Purity**
- Python acts as configuration layer only
- Rust handles all data processing
- Single responsibility separation
- Clean GraphQL→HTTP request/response mapping

### 4. **Operational Simplicity**
- Single runtime (Tokio, not Tokio + Python threading)
- No GIL contention issues
- Easier deployment (stateless Rust services)
- Better observability (unified tracing)

---

## Implementation Strategy

### Phase 3a: Unified Request Handler FFI

**New Binding**: `process_graphql_request()`
```rust
/// Single entry point for all GraphQL HTTP requests
///
/// Input: GraphQL request (query string or JSON)
/// Output: HTTP response (JSON with status code)
///
/// Everything executed in Rust:
/// - Query parsing & validation
/// - Database execution
/// - Response building
/// - Error handling
#[pyfunction]
pub fn process_graphql_request(
    request_json: &str,  // {"query": "...", "variables": {...}, "operationName": "..."}
    http_context: &PyDict,  // auth headers, request metadata
) -> PyResult<String> {  // HTTP response JSON with status
    // All Rust execution - no FFI until return
    Ok(response_json)
}
```

### Phase 3b: Schema Update FFI (Reload only)

**Existing Binding** (refactored): `update_schema()`
```rust
/// Update schema (called only on schema changes, not per-request)
#[pyfunction]
pub fn update_schema(schema_json: &str) -> PyResult<()> {
    // Update schema registry in-memory
    Ok(())
}
```

### Phase 3c: Move HTTP Handler to Rust

**From Python to Rust** (fraiseql_rs/src/http/axum_server.rs):
```rust
/// Axum route handler - all Rust, no FFI during request
pub async fn graphql_handler(
    Extension(pipeline): Extension<Arc<GraphQLPipeline>>,
    Json(request): Json<GraphQLRequest>,
) -> Json<GraphQLResponse> {
    // Direct Rust execution
    let response = pipeline.execute(request).await;
    Json(response)
}
```

---

## File Organization

### Current State
```
fraiseql/
├── src/fraiseql/
│   ├── app.py                    # Python HTTP routing (BOTTLENECK)
│   ├── db_core.py                # Python query building (DEPRECATED)
│   ├── graphql.py                # Python GraphQL coordination
│   └── [graphql handlers]        # Python request handling
└── fraiseql_rs/
    ├── src/
    │   ├── lib.rs                # 6 PyO3 bindings (FFI per request)
    │   ├── http/axum_server.rs   # Rust HTTP (UNUSED)
    │   ├── pipeline/unified.rs   # Unified pipeline (callable from Rust)
    │   └── [other modules]
```

### Proposed State
```
fraiseql/
├── src/fraiseql/
│   ├── config.py                 # Configuration loading only
│   ├── schema_loader.py          # Schema loading (calls Rust via FFI once)
│   └── startup.py                # Server startup coordination
└── fraiseql_rs/
    ├── src/
    │   ├── lib.rs                # 2 FFI bindings (startup only)
    │   │   ├── process_graphql_request()  # Main request handler
    │   │   └── update_schema()
    │   ├── http/
    │   │   ├── axum_server.rs    # HTTP server (NOW MAIN)
    │   │   ├── handlers.rs       # Request handlers (Rust)
    │   │   └── middleware.rs     # Auth, logging, etc. (Rust)
    │   ├── pipeline/unified.rs   # Unified pipeline (internal Rust)
    │   └── [all other modules]   # All Rust
```

---

## Migration Path (Zero Breaking Changes)

### Step 1: Phase 3a - Add Unified FFI
- Create `process_graphql_request()` binding
- Keep existing 6 bindings functional
- Python can call either old or new binding
- **Effort**: 4-6 hours
- **Risk**: Low (additive change)

### Step 2: Phase 3b - Gradual Migration
- Point Python HTTP handlers to new binding
- Test compatibility thoroughly
- Run both old and new side-by-side
- **Effort**: 2-4 hours
- **Risk**: Low (can rollback)

### Step 3: Phase 3c - Remove Old FFI
- Deprecate old 6 bindings
- Python calls `process_graphql_request()` only
- Remove old bindings after 1 release cycle
- **Effort**: 1 hour
- **Risk**: None (after deprecation period)

### Step 4: Phase 3d - Move HTTP to Rust (Future)
- Replace Python Axum wrapper with native Rust
- Rust binary handles all requests
- Python becomes optional (config layer only)
- **Effort**: 8-12 hours
- **Risk**: Medium (major restructuring, but separable)

---

## Detailed FFI Comparison

### Current (6 Bindings)

| Binding | Called | Frequency | GIL Impact | Serialization |
|---------|--------|-----------|-----------|-----------------|
| `initialize_graphql_pipeline()` | Startup | 1× | Acquire | Config object |
| `initialize_schema_registry()` | Startup | 1× | Acquire | Schema JSON |
| `reset_schema_registry_for_testing()` | Test setup | 1× per test | Acquire | None |
| `execute_query_async()` | **Per request** | **100s/sec** | **Acquire** | **Query + response** |
| `execute_mutation_async()` | **Per request** | **10s/sec** | **Acquire** | **Mutation + response** |
| `build_multi_field_response()` | **Per request** | **100s/sec** | **Acquire** | **Intermediate JSON** |

**Total FFI calls per second**: ~600-1000 (contention point)

### Proposed (2 Bindings)

| Binding | Called | Frequency | GIL Impact | Serialization |
|---------|--------|-----------|-----------|-----------------|
| `update_schema()` | Schema change | 0-10× per day | Acquire once | Schema JSON |
| `process_graphql_request()` | **Per request** | **100s/sec** | **None (Rust async)** | **Request + response only** |

**Total FFI calls per second**: ~100 (request boundary only)

---

## Performance Estimates

### Current Architecture (6 FFI bindings)
```
Per 1000 requests:
├─ 6000+ FFI calls
├─ 6000+ GIL acquisitions
├─ Serialization overhead: ~10-20ms per request
├─ Python HTTP routing: ~5-10ms per request
└─ Total overhead: ~15-30ms per request = 50-66% of Rust execution time
```

### Proposed Architecture (Single FFI)
```
Per 1000 requests:
├─ 1000 FFI calls (startup only)
├─ 0 GIL acquisitions during request
├─ No serialization overhead
├─ Rust HTTP routing: <1ms per request
└─ Total overhead: <1ms per request = 1-5% of Rust execution time
```

**Expected improvement**: 10-30x faster request handling (depending on query complexity)

---

## Code Example: Unified Handler

### Before (Multiple FFI crossings)
```python
# Python HTTP handler
@app.route('/graphql', methods=['POST'])
def graphql():
    request_data = request.get_json()

    # FFI Call 1: Python → Rust execution
    result = execute_query_async(request_data['query'])

    # FFI Call 2: Python → Rust building response
    response = build_multi_field_response(result)

    return response
```

### After (Single FFI)
```python
# Python HTTP handler (thin wrapper)
@app.route('/graphql', methods=['POST'])
def graphql():
    request_data = request.get_json()

    # Single FFI call - everything else is Rust
    response_json = process_graphql_request(
        json.dumps(request_data),
        {
            'headers': dict(request.headers),
            'remote_addr': request.remote_addr,
        }
    )

    return response_json
```

---

## Rust Implementation Detail

### Current pipeline (callable from Rust)
```rust
// Already exists in fraiseql_rs/src/pipeline/unified.rs
pub struct PyGraphQLPipeline {
    schema: GraphQLSchema,
    transformers: Arc<TransformerCache>,
    pool: DatabasePool,
}

impl PyGraphQLPipeline {
    pub async fn execute(&self, query: &str, vars: &Value) -> Result<Value> {
        // Full GraphQL execution in Rust
        // 1. Parse query
        // 2. Validate against schema
        // 3. Execute query plan
        // 4. Build response
        Ok(response)
    }
}
```

### New FFI wrapper
```rust
#[pyfunction]
pub fn process_graphql_request(request_json: &str, context_dict: &PyDict) -> PyResult<String> {
    // Parse request
    let request: GraphQLRequest = serde_json::from_str(request_json)?;

    // Extract context
    let auth_header = context_dict.get_item("headers")?;

    // Get global pipeline (initialized once at startup)
    let pipeline = GLOBAL_PIPELINE.lock().unwrap();

    // Execute (all in Rust, no GIL needed)
    let runtime = tokio::runtime::Handle::current();
    let response = runtime.block_on(pipeline.execute(&request))?;

    // Return as JSON string
    Ok(serde_json::to_string(&response)?)
}
```

---

## Remaining QueryBuilder Integration

### Phase 3 (This Sprint) - FFI Architecture
- Create `process_graphql_request()` binding ← We are here
- Move HTTP layer coordination to Rust
- **QueryBuilder**: Called internally from `execute_query_plan()` in Rust

### Phase 4 (Next Sprint) - Remove Old Python Query Building
- Python `query_builder.py` no longer used
- Rust `QueryBuilder` handles all SQL generation
- Old Python code deprecated

---

## Summary: FFI Boundary Strategy

**Current**: 6 FFI bindings × 100-1000 calls/sec = Contention point
**Proposed**: 2 FFI bindings × <10 calls/sec = Negligible overhead

**Benefits**:
- ✅ 10-30x faster request handling
- ✅ Better scalability (no GIL)
- ✅ Cleaner architecture (single responsibility)
- ✅ Future-proof (move HTTP to Rust later)
- ✅ Zero breaking changes during migration

**This enables the QueryBuilder (Phase 2) to be used within Rust without any Python FFI overhead.**

---

## Timeline

| Phase | Task | FFI Bindings | Effort | Risk |
|-------|------|--------------|--------|------|
| 3a | Add `process_graphql_request()` | +1 (7 total) | 4-6h | Low |
| 3b | Migrate Python to new binding | 7 total | 2-4h | Low |
| 3c | Remove old bindings | 2 final | 1h | None |
| 4 (Future) | Move HTTP to Rust | 1-2 | 8-12h | Medium |

---

**Recommendation**: Implement Phase 3a immediately after Phase 2 (QueryBuilder) to maximize performance gains.
