# Phase 3d: Hot Path Optimization - Moving Query Detection & Response Building to Rust

**Status**: PLANNING
**Date**: January 8, 2026
**Goal**: Eliminate Python from critical path while maintaining clean Python API

---

## Overview

Phase 3d optimizes the critical execution path by moving query detection and response building to Rust, while keeping Python as the API/configuration layer.

### Architecture Principle

```
┌─────────────────────────────┐
│    Python API Layer         │
│  (User-facing, config)      │
└────────────┬────────────────┘
             │ (config → request)
             ↓
┌─────────────────────────────┐
│   Python Routing Layer      │
│  (HTTP, auth, context)      │
└────────────┬────────────────┘
             │ (PreparedRequest)
             ↓
┌─────────────────────────────┐
│  🦀 Unified Rust Core 🦀    │
│  (execution, detection,     │
│   response building)        │
└────────────┬────────────────┘
             │ (HTTP bytes)
             ↓
┌─────────────────────────────┐
│    FastAPI HTTP Response    │
│   (bytes only, no parsing)  │
└─────────────────────────────┘
```

**Key Principle**: Users write Python, everything executes in Rust

---

## Phase 3d Breakdown

### Sprint 1: Query Detection in Rust (Low Risk)

**Objective**: Move query analysis from Python to Rust

#### 1.1 Current Python Flow
```python
# routers.py - current implementation
def create_graphql_router():
    @router.post("/graphql")
    async def graphql_endpoint(request: Request):
        # Parse GraphQL request
        graphql_request = await request.json()

        # Python detection:
        query_text = graphql_request.get("query", "")
        field_count = _count_root_query_fields(query_text)  # ← MOVE TO RUST
        is_introspection = _is_introspection_query(query_text)  # ← MOVE TO RUST

        # Route based on detection
        if is_introspection:
            return await handle_introspection(...)
        elif field_count == 1:
            return await handle_single_field(...)
        else:
            return await handle_multi_field(...)
```

#### 1.2 New Rust FFI: `analyze_graphql_query()`

**New Rust function in `fraiseql_rs/src/lib.rs`:**

```rust
/// Analyze a GraphQL query to determine execution path
///
/// Returns JSON with routing metadata:
/// {
///   "field_count": number,
///   "is_introspection": boolean,
///   "root_fields": ["users", "posts"],
///   "operation_type": "query" | "mutation",
///   "operation_name": string | null,
/// }
#[pyfunction]
pub fn analyze_graphql_query(query: String) -> PyResult<String> {
    let analysis = QueryAnalyzer::analyze(&query)?;
    Ok(serde_json::to_string(&analysis)?)
}
```

**Benefits:**
- ✓ Single pass through query
- ✓ Rust string parsing (faster)
- ✓ Cached metadata for multi-field queries
- ✓ Zero Python overhead

#### 1.3 Python Adapter Update

**New function in `unified_ffi_adapter.py`:**

```python
def analyze_graphql_query(query: str) -> dict:
    """Analyze query to determine execution path.

    Args:
        query: GraphQL query string

    Returns:
        {
            "field_count": int,
            "is_introspection": bool,
            "root_fields": list[str],
            "operation_type": "query" | "mutation",
            "operation_name": str | None,
        }
    """
    result_json = fraiseql_rs.analyze_graphql_query(query)
    return json.loads(result_json)
```

**Router update (minimal):**

```python
@router.post("/graphql")
async def graphql_endpoint(request: Request):
    graphql_request = await request.json()
    query = graphql_request.get("query", "")

    # Rust analysis (replaces Python detection)
    analysis = analyze_graphql_query(query)

    # Route based on analysis
    if analysis["is_introspection"]:
        return await handle_introspection(...)
    elif analysis["field_count"] == 1:
        return await handle_single_field(...)
    else:
        return await handle_multi_field(...)
```

#### 1.4 Rust Implementation Details

**File**: `fraiseql_rs/src/query_analyzer.rs`

```rust
pub struct QueryAnalysis {
    pub field_count: usize,
    pub is_introspection: bool,
    pub root_fields: Vec<String>,
    pub operation_type: OperationType,
    pub operation_name: Option<String>,
}

pub enum OperationType {
    Query,
    Mutation,
    Subscription,
}

impl QueryAnalyzer {
    pub fn analyze(query: &str) -> Result<QueryAnalysis> {
        // Parse GraphQL query using gql crate
        let document = parse_query(query)?;

        // Extract operations
        let operation = document.definitions
            .iter()
            .find(|d| matches!(d, Definition::Operation(_)))?;

        // Detect introspection
        let is_introspection = query.contains("__schema")
            || query.contains("__type");

        // Count root fields
        let root_fields = extract_root_fields(&operation);

        Ok(QueryAnalysis {
            field_count: root_fields.len(),
            is_introspection,
            root_fields,
            operation_type: operation.operation_type(),
            operation_name: operation.name().map(|n| n.to_string()),
        })
    }
}
```

**Testing:**
- ✓ Single-field query detection
- ✓ Multi-field query detection
- ✓ Introspection detection
- ✓ Mutation vs Query
- ✓ Named operations
- ✓ Fragment handling

---

### Sprint 2: Response Building in Rust (Medium Risk)

**Objective**: Move `ExecutionResult → JSON HTTP response` to Rust

#### 2.1 Current Python Flow
```python
# execution/unified_executor.py - current
result = await graphql.execute(schema, query, ...)

# routers.py - current response building
if isinstance(result, RustResponseBytes):
    return result  # Direct bytes
else:
    # Python conversion: ExecutionResult → dict → JSON
    response_dict = {
        "data": result.data,
        "errors": [
            {
                "message": str(error),
                "locations": error.locations,
                "path": error.path,
                "extensions": error.extensions,
            }
            for error in (result.errors or [])
        ],
    }
    return FraiseQLJSONResponse(response_dict)
```

#### 2.2 New Rust FFI: `build_graphql_response_from_execution()`

**Extended function in `unified_ffi_adapter.py`:**

```python
def build_graphql_response_from_execution_result(
    data: dict | None,
    errors: list[dict] | None,
    extensions: dict | None = None,
) -> bytes:
    """Build HTTP-ready GraphQL response in Rust.

    Converts ExecutionResult to complete GraphQL response JSON.
    Handles error formatting, null data, and extensions.

    Args:
        data: Result data from execution
        errors: List of GraphQL errors
        extensions: Optional response extensions

    Returns:
        Complete GraphQL response as JSON bytes
    """
    execution_result = {
        "data": data,
        "errors": errors,
        "extensions": extensions,
    }
    return fraiseql_rs.build_response_from_execution(
        json.dumps(execution_result)
    )
```

**New Rust function in `fraiseql_rs/src/lib.rs`:**

```rust
/// Build complete GraphQL HTTP response from execution result
///
/// Takes an ExecutionResult (data + errors) and builds a complete
/// GraphQL response JSON with proper formatting, error details, etc.
#[pyfunction]
pub fn build_response_from_execution(
    result_json: String,
) -> PyResult<PyObject> {
    let result = serde_json::from_str::<ExecutionResult>(&result_json)?;
    let response = build_graphql_response(&result)?;

    // Return as bytes for direct HTTP response
    Ok(serde_json::to_string(&response)?.into_bytes().into_py())
}
```

#### 2.3 Router Update (Simplified)

**Current (`routers.py`):**
```python
result = await execute(...)  # ExecutionResult
if isinstance(result, RustResponseBytes):
    return result
else:
    # 10 lines of Python dict building
    return FraiseQLJSONResponse(response_dict)
```

**New (`routers.py`):**
```python
result = await execute(...)  # ExecutionResult
if isinstance(result, RustResponseBytes):
    return result
else:
    # Single Rust call
    response_bytes = build_graphql_response_from_execution_result(
        data=result.data,
        errors=format_errors(result.errors) if result.errors else None,
    )
    return Response(content=response_bytes, media_type="application/json")
```

#### 2.4 Rust Implementation Details

**File**: `fraiseql_rs/src/response_builder.rs`

```rust
pub struct ExecutionResult {
    pub data: Option<serde_json::Value>,
    pub errors: Option<Vec<GraphQLError>>,
    pub extensions: Option<serde_json::Value>,
}

pub struct GraphQLError {
    pub message: String,
    pub locations: Option<Vec<SourceLocation>>,
    pub path: Option<Vec<PathSegment>>,
    pub extensions: Option<serde_json::Value>,
}

impl ResponseBuilder {
    pub fn build(result: ExecutionResult) -> Result<serde_json::Value> {
        let mut response = serde_json::json!({});

        // Always include data (can be null)
        response["data"] = result.data.unwrap_or(serde_json::json!(null));

        // Include errors if present
        if let Some(errors) = result.errors {
            response["errors"] = serde_json::to_value(
                errors.iter()
                    .map(|e| self.format_error(e))
                    .collect::<Vec<_>>()
            )?;
        }

        // Include extensions if present
        if let Some(exts) = result.extensions {
            response["extensions"] = exts;
        }

        Ok(response)
    }

    fn format_error(&self, error: &GraphQLError) -> serde_json::Value {
        let mut err_obj = serde_json::json!({
            "message": error.message,
        });

        if let Some(locations) = &error.locations {
            err_obj["locations"] = serde_json::to_value(locations).unwrap();
        }

        if let Some(path) = &error.path {
            err_obj["path"] = serde_json::to_value(path).unwrap();
        }

        if let Some(exts) = &error.extensions {
            err_obj["extensions"] = exts.clone();
        }

        err_obj
    }
}
```

---

## Implementation Timeline

### Week 1: Query Detection (Sprint 1)
- **Monday-Wednesday**: Implement Rust `analyze_graphql_query()`
  - Parse GraphQL in Rust using `gql` crate
  - Extract field count, introspection flag, operation type
  - Return JSON metadata

- **Thursday**: Python adapter + router integration
  - Add `analyze_graphql_query()` to adapter
  - Update `create_graphql_router()` to use Rust analysis
  - Remove Python analysis functions

- **Friday**: Testing & validation
  - Unit tests for Rust analyzer
  - Integration tests with routers
  - Benchmark comparison (Python vs Rust analysis)

**Deliverable**: Router detection moved to Rust, 5-10% faster query routing

---

### Week 2: Response Building (Sprint 2)
- **Monday-Tuesday**: Implement Rust `build_response_from_execution()`
  - Handle data + errors serialization
  - Format error objects correctly
  - Return complete JSON response

- **Wednesday**: Python adapter integration
  - Add `build_graphql_response_from_execution_result()` to adapter
  - Update routers to use Rust response builder
  - Remove Python response building code

- **Thursday-Friday**: Testing & optimization
  - Unit tests for error formatting
  - Edge cases (null data, no errors, extensions)
  - Performance benchmarking

**Deliverable**: Response building moved to Rust, 10-15% faster responses

---

### Week 3: Verification & Documentation (Sprint 3)
- **Monday-Tuesday**: Full test suite
  - Run all 5991+ tests
  - Verify zero regressions
  - Benchmark end-to-end improvements

- **Wednesday-Thursday**: Documentation
  - Phase 3d completion guide
  - Architecture diagrams
  - Performance analysis

- **Friday**: Commit & cleanup
  - Final commit to feature branch
  - Update version documentation
  - Close Phase 3d

---

## Testing Strategy

### Unit Tests (Rust)

**`fraiseql_rs/tests/query_analyzer.rs`:**
```rust
#[test]
fn test_single_field_query() {
    let analysis = QueryAnalyzer::analyze("{ users { id } }").unwrap();
    assert_eq!(analysis.field_count, 1);
    assert_eq!(analysis.root_fields, vec!["users"]);
    assert!(!analysis.is_introspection);
}

#[test]
fn test_multi_field_query() {
    let analysis = QueryAnalyzer::analyze("{ users { id } posts { id } }").unwrap();
    assert_eq!(analysis.field_count, 2);
    assert!(analysis.root_fields.contains(&"users".to_string()));
}

#[test]
fn test_introspection_detection() {
    let analysis = QueryAnalyzer::analyze("{ __schema { types { name } } }").unwrap();
    assert!(analysis.is_introspection);
}
```

**`fraiseql_rs/tests/response_builder.rs`:**
```rust
#[test]
fn test_success_response() {
    let result = ExecutionResult {
        data: Some(json!({"users": []})),
        errors: None,
        extensions: None,
    };
    let response = ResponseBuilder::build(result).unwrap();
    assert_eq!(response["data"], json!({"users": []}));
    assert!(!response.get("errors").is_some());
}

#[test]
fn test_error_response() {
    let result = ExecutionResult {
        data: None,
        errors: Some(vec![GraphQLError {
            message: "Not found".to_string(),
            ...
        }]),
        ...
    };
    let response = ResponseBuilder::build(result).unwrap();
    assert!(response.get("errors").is_some());
}
```

### Integration Tests (Python)

**`tests/integration/phase_3d/test_query_analyzer.py`:**
```python
def test_single_field_query_analysis():
    analysis = analyze_graphql_query("{ users { id } }")
    assert analysis["field_count"] == 1
    assert "users" in analysis["root_fields"]
    assert not analysis["is_introspection"]

def test_multi_field_query_analysis():
    analysis = analyze_graphql_query("{ users { id } posts { id } }")
    assert analysis["field_count"] == 2
    assert set(analysis["root_fields"]) == {"users", "posts"}
```

**`tests/integration/phase_3d/test_response_builder.py`:**
```python
async def test_graphql_success_response():
    response = build_graphql_response_from_execution_result(
        data={"users": []},
        errors=None,
    )
    parsed = json.loads(response)
    assert parsed["data"] == {"users": []}
    assert "errors" not in parsed
```

### Regression Tests (Full Suite)

```bash
# Must pass: all 5991+ existing tests
pytest tests/ -v

# Performance baseline
pytest tests/benchmarks/ -v --benchmark-only
```

---

## Success Criteria

### Phase 3d Completion

✓ **Query Detection in Rust**
- Rust FFI: `analyze_graphql_query()` implemented
- Python adapter updated
- Router uses Rust analysis
- All tests pass
- 5-10% routing faster

✓ **Response Building in Rust**
- Rust FFI: `build_response_from_execution()` implemented
- Python adapter updated
- Router uses Rust response builder
- All tests pass
- 10-15% response building faster

✓ **Zero Breaking Changes**
- 100% backward compatible API
- All 5991+ tests pass
- No changes to user-facing decorators or APIs

✓ **Documentation**
- Phase 3d completion guide
- Architecture diagrams updated
- Performance analysis
- Code comments

---

## Risk Assessment

### Low Risk Changes
- ✓ Query analyzer (new Rust code, doesn't affect existing flow)
- ✓ Response builder (replaces Python building, same output format)

### Potential Issues & Mitigation

| Risk | Impact | Mitigation |
|------|--------|-----------|
| Query parsing differences | Query rejection | Full test suite validates |
| Error format changes | Client breaks | Unit tests verify format |
| Performance regression | Slower responses | Benchmarks verify improvement |
| Rust compilation | Build failure | CI/CD validates |

---

## Files to Create/Modify

### New Rust Files
- `fraiseql_rs/src/query_analyzer.rs` - Query analysis (200 lines)
- `fraiseql_rs/src/response_builder.rs` - Response building (150 lines)

### Modified Python Files
- `src/fraiseql/core/unified_ffi_adapter.py` - Add new FFI functions (+50 lines)
- `src/fraiseql/fastapi/routers.py` - Use Rust analysis & builders (-50 lines)
- `src/fraiseql/execution/unified_executor.py` - Simplify response handling (-20 lines)

### New Test Files
- `fraiseql_rs/tests/query_analyzer.rs` - Rust query analysis tests
- `fraiseql_rs/tests/response_builder.rs` - Rust response building tests
- `tests/integration/phase_3d/test_query_analyzer.py` - Python integration tests
- `tests/integration/phase_3d/test_response_builder.py` - Python integration tests

### Documentation
- `docs/PHASE_3D_IMPLEMENTATION_SUMMARY.md` - Final Phase 3d summary

---

## Performance Expectations

### Query Detection Improvement
```
Before (Python):
- graphql-core parsing: ~2ms
- Field detection: ~1ms
- Total: ~3ms per request

After (Rust):
- Rust parsing: ~0.5ms
- Field detection: ~0.1ms
- Total: ~0.6ms per request

Improvement: 5x faster detection
```

### Response Building Improvement
```
Before (Python):
- ExecutionResult → dict conversion: ~1ms
- JSON serialization: ~2ms
- Total: ~3ms per request

After (Rust):
- Direct JSON building: ~0.3ms
- Total: ~0.3ms per request

Improvement: 10x faster building
```

### End-to-End Improvement
```
Before (Phase 3c):
- Query detection: 3ms
- Response building: 3ms
- Total overhead: 6ms per request
- Real latency: 16-30ms (query + DB)

After (Phase 3d):
- Query detection: 0.6ms
- Response building: 0.3ms
- Total overhead: 0.9ms per request
- Real latency: 10-25ms (6ms improvement)

Overall improvement: 15-25% faster responses
```

---

## Next Steps After Phase 3d

### Phase 3e: Mutation Pipeline Unification
- Move mutation execution to same Rust path
- Eliminate dual paths (query vs mutation)
- Single unified executor in Rust

### Phase 3f: Subscription Support
- Move subscription management to Rust
- Async Rust with tokio
- WebSocket handling in Rust

### Phase 4: Full Rust Runtime
- Axum HTTP server (replace FastAPI)
- Tokio async runtime
- Pure Rust deployment

---

**Status**: Planning complete. Ready for implementation.

**Recommendation**: Start with Sprint 1 (Query Detection) for quick win and validation, then proceed to Sprint 2.
