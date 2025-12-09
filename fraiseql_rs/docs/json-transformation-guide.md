# JSON Transformation Architecture

FraiseQL has **two distinct JSON transformation strategies** optimized for different use cases.

## Quick Decision Tree

```
Need JSON transformation?
├─ Called from Python directly? → Use json_transform.rs (PyO3 exposed, serde_json)
├─ Need schema-aware transformation? → Use json_transform.rs (SchemaRegistry integration)
├─ GraphQL query pipeline (hot path)? → Use core::transform.rs (zero-copy streaming)
├─ Need max performance (1000s/sec)? → Use core::transform.rs (arena + SIMD)
└─ Working with JSON strings? → Use json_transform.rs (String → String)
```

---

## Implementation 1: Value-Based API (json_transform.rs)

**Location**: `fraiseql_rs/src/json_transform.rs`
**Lines of Code**: 718 lines
**Primary Use Case**: PyO3 integration, schema-aware transformation, String-based API

### Public API

```rust
// From src/json_transform.rs:49
pub fn transform_json_string(json_str: &str) -> PyResult<String>

// Internal helpers (schema-aware):
fn transform_value(value: Value) -> Value
fn transform_nested_object(value: &Value, type_name: &str, is_list: bool, registry: &SchemaRegistry) -> Value
fn transform_with_schema(value: &Value, type_name: &str, registry: &SchemaRegistry) -> Value
```

### Python Exposure

```rust
// From src/lib.rs:67-71
#[pyfunction]
pub fn transform_json(json_str: &str) -> PyResult<String> {
    json_transform::transform_json_string(json_str)
}
```

### Architecture

```rust
// From src/json_transform.rs:12-26
/// Transform a JSON string by converting all keys from snake_case to camelCase
///
/// This function provides the **fastest path** for JSON transformation:
/// 1. Parse JSON (serde_json - zero-copy where possible)
/// 2. Transform keys recursively (move semantics, no clones)
/// 3. Serialize back to JSON (optimized buffer writes)
///
/// This avoids the Python dict round-trip, making it **10-50x faster**
/// for large JSON objects compared to Python-based transformation.
///
/// # Performance Characteristics
/// - **Zero-copy parsing**: serde_json optimizes for owned string slices
/// - **Move semantics**: Values moved, not cloned during transformation
/// - **Single allocation**: Output buffer pre-sized by serde_json
/// - **No Python GIL**: Entire operation runs in Rust (GIL-free)
```

### Implementation Strategy

```rust
// From src/json_transform.rs:48-60
#[inline]
pub fn transform_json_string(json_str: &str) -> PyResult<String> {
    // 1. Parse JSON string → serde_json::Value
    let value: Value = serde_json::from_str(json_str)
        .map_err(|e| PyValueError::new_err(format!("Invalid JSON: {}", e)))?;

    // 2. Transform Value recursively (move semantics)
    let transformed = transform_value(value);

    // 3. Serialize Value → JSON string
    serde_json::to_string(&transformed)
        .map_err(|e| PyValueError::new_err(format!("Failed to serialize JSON: {}", e)))
}
```

### Recursive Transformation

```rust
// From src/json_transform.rs:62-86
fn transform_value(value: Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut new_map = Map::new();
            for (key, val) in map {
                let camel_key = to_camel_case(&key);  // ← Uses camel_case.rs
                let transformed_val = transform_value(val);  // ← Recursive
                new_map.insert(camel_key, transformed_val);
            }
            Value::Object(new_map)
        }
        Value::Array(arr) => {
            let transformed_arr: Vec<Value> = arr.into_iter()
                .map(transform_value)  // ← Recursive on each element
                .collect();
            Value::Array(transformed_arr)
        }
        // Primitives (String, Number, Bool, Null): return as-is
        other => other,
    }
}
```

### Schema-Aware Transformation

```rust
// From src/json_transform.rs:88-135
/// Transform a nested object field using schema information
///
/// This is a helper function that handles:
/// - Null values (pass through)
/// - Nested objects (recursive transformation with correct type)
/// - Arrays of nested objects (map over items)
///
/// # Performance
/// Uses schema registry O(1) lookup to resolve correct type for nested objects
#[inline]
fn transform_nested_object(
    value: &Value,
    type_name: &str,
    is_list: bool,
    registry: &SchemaRegistry,
) -> Value {
    if is_list {
        // Array of nested objects
        match value {
            Value::Array(items) => {
                let transformed_items: Vec<Value> = items
                    .iter()
                    .map(|item| match item {
                        Value::Null => Value::Null,
                        _ => transform_with_schema(item, type_name, registry),
                    })
                    .collect();
                Value::Array(transformed_items)
            }
            // ... handle null and error cases
        }
    } else {
        // Single nested object
        match value {
            Value::Null => Value::Null,
            _ => transform_with_schema(value, type_name, registry),
        }
    }
}
```

### Usage Examples (Real Code)

```rust
// From src/lib.rs:67-71 (Python exposure)
#[pyfunction]
pub fn transform_json(json_str: &str) -> PyResult<String> {
    json_transform::transform_json_string(json_str)
}

// Python usage:
// >>> from fraiseql_rs import transform_json
// >>> transform_json('{"user_name": "John", "email_address": "john@example.com"}')
// '{"userName":"John","emailAddress":"john@example.com"}'
```

### Performance Characteristics

```rust
// From src/json_transform.rs:28-31
/// # Typical Performance
/// - Simple object (10 fields): ~0.1-0.2ms (vs 5-10ms Python)
/// - Complex object (50 fields): ~0.5-1ms (vs 20-30ms Python)
/// - Nested (User + 15 posts): ~1-2ms (vs 40-80ms CamelForge)
```

**Memory Pattern**:
- Parses JSON into `serde_json::Value` (heap-allocated tree)
- Transforms tree in-place using move semantics (no clones for values)
- Re-serializes tree to JSON string

**Use When**:
- ✅ Called from Python (PyO3 boundary)
- ✅ Need schema-aware transformation
- ✅ Working with JSON strings (`String` → `String`)
- ✅ Need serde_json `Value` compatibility
- ✅ Moderate volume (< 1000 req/sec)

**Avoid When**:
- ❌ Hot path query pipeline (use core::transform instead)
- ❌ Need streaming/zero-copy (use core::transform instead)
- ❌ Ultra-high performance required (use core::transform instead)

---

## Implementation 2: Zero-Copy Streaming API (core/transform.rs)

**Location**: `fraiseql_rs/src/core/transform.rs`
**Lines of Code**: 669 lines
**Primary Use Case**: GraphQL query pipeline, streaming transformation, zero-copy

### Public API

```rust
// From src/core/transform.rs:11-18
#[derive(Clone, Copy)]
pub struct TransformConfig {
    pub add_typename: bool,
    pub camel_case: bool,
    pub project_fields: bool,
    pub add_graphql_wrapper: bool,
}

// From src/core/transform.rs:38-42
pub struct ZeroCopyTransformer<'a> {
    arena: &'a Arena,
    config: TransformConfig,
    typename: Option<&'a str>,
    field_projection: Option<&'a FieldSet>,
}

// Constructor:
impl<'a> ZeroCopyTransformer<'a> {
    pub fn new(
        arena: &'a Arena,
        config: TransformConfig,
        typename: Option<&'a str>,
        field_projection: Option<&'a FieldSet>,
    ) -> Self { ... }

    pub fn transform_bytes(&self, input: &[u8], output: &mut ByteBuf) -> Result<(), PyErr> { ... }
}
```

### Architecture Overview

```rust
// From src/core/transform.rs:19-35
/// Zero-copy JSON transformer
///
/// PERFORMANCE CHARACTERISTICS:
/// - Single-pass: Reads input once, writes output once
/// - Zero-copy keys: Keys transformed in-place when possible
/// - Arena allocation: All intermediate data uses bump allocator
/// - SIMD: Vectorized operations for escaping and case conversion
///
/// Memory layout:
/// ┌─────────────────────────────────────────────────┐
/// │ Input Buffer (read-only)                        │ ← PostgreSQL result
/// ├─────────────────────────────────────────────────┤
/// │ Arena (bump allocator)                          │ ← Temporary keys/values
/// ├─────────────────────────────────────────────────┤
/// │ Output Buffer (write-only, pre-sized)           │ → HTTP response
/// └─────────────────────────────────────────────────┘
```

### Usage in Pipeline

```rust
// From src/pipeline/builder.rs:5-11
use crate::core::arena::Arena;
use crate::core::transform::{ByteBuf, TransformConfig, ZeroCopyTransformer};
use crate::json_transform;
use crate::pipeline::projection::FieldSet;
use crate::schema_registry;
use pyo3::prelude::*;
use serde_json::{json, Value};

// From src/pipeline/builder.rs:145-197
pub fn build_graphql_response(
    json_rows: Vec<String>,
    field_name: &str,
    type_name: Option<&str>,
    field_paths: Option<Vec<String>>,
    field_selections: Option<Vec<String>>,
    is_list: Option<bool>,
) -> PyResult<Vec<u8>> {
    // 1. Setup arena for scratch allocations
    let arena = Arena::with_capacity(estimate_arena_size(&json_rows));

    // 2. Configure transformation
    let config = TransformConfig {
        add_typename: type_name.is_some(),
        camel_case: true,
        project_fields: field_paths.is_some(),
        add_graphql_wrapper: false,
    };

    // 3. Setup field projection (if requested)
    let field_set = field_paths.map(|paths| FieldSet::from_paths(&paths, &arena));

    // 4. Create zero-copy transformer
    let transformer = ZeroCopyTransformer::new(
        &arena,
        config,
        type_name,
        field_set.as_ref()
    );

    // 5. Estimate output size (avoid reallocs)
    let total_input_size: usize = json_rows.iter().map(|s| s.len()).sum();
    let wrapper_overhead = 50 + field_name.len();
    let estimated_size = total_input_size + wrapper_overhead;
    let mut result = Vec::with_capacity(estimated_size);

    // 6. Build GraphQL wrapper
    result.extend_from_slice(b"{\"data\":{\"");
    result.extend_from_slice(field_name.as_bytes());
    result.extend_from_slice(b"\":");

    // 7. Transform each row (zero-copy streaming)
    if is_list.unwrap_or(true) {
        result.push(b'[');
        for (i, row) in json_rows.iter().enumerate() {
            let mut temp_buf = ByteBuf::with_estimated_capacity(row.len(), &config);
            transformer.transform_bytes(row.as_bytes(), &mut temp_buf)?;  // ← Zero-copy!
            result.extend_from_slice(&temp_buf.into_vec());

            if i < json_rows.len() - 1 {
                result.push(b',');
            }
        }
        result.push(b']');
    } else if !json_rows.is_empty() {
        let row = &json_rows[0];
        let mut temp_buf = ByteBuf::with_estimated_capacity(row.len(), &config);
        transformer.transform_bytes(row.as_bytes(), &mut temp_buf)?;  // ← Zero-copy!
        result.extend_from_slice(&temp_buf.into_vec());
    }

    // 8. Close GraphQL wrapper
    result.extend_from_slice(b"}}");
    Ok(result)
}
```

### Zero-Copy Strategy

**Key Insight**: Never parse JSON into `serde_json::Value`
- Read bytes directly from PostgreSQL result
- Write bytes directly to HTTP response
- No intermediate Value tree allocation

**Transformation Steps**:
1. **Scan** input JSON bytes (single pass)
2. **Transform** keys using `core::camel::snake_to_camel()` (SIMD, arena)
3. **Write** output bytes (pre-sized buffer, no realloc)
4. **Project** fields if requested (bitmap-based)
5. **Add** `__typename` if requested

**Memory Efficiency**:
- Arena allocator: O(1) bump allocation, bulk deallocation
- No heap allocations for transformed keys (arena-based)
- No JSON tree (avoids 10-20x memory overhead)
- Output buffer pre-sized (no Vec reallocs)

### Performance Characteristics

**Typical Performance**:
- Simple query (10 rows × 10 fields): ~0.5ms
- Complex query (100 rows × 50 fields): ~3-5ms
- Nested query (50 rows × 20 fields + cascade): ~2-4ms

**Compared to json_transform.rs**:
- **3-5x faster** for typical queries
- **10-15x faster** for large result sets (1000+ rows)
- **60-80% less memory** (no Value tree)

**Use When**:
- ✅ GraphQL query pipeline (hot path)
- ✅ High volume (> 1000 req/sec)
- ✅ Need streaming transformation
- ✅ Need zero-copy performance
- ✅ Arena allocator available
- ✅ Field projection required

**Avoid When**:
- ❌ Need schema-aware transformation (use json_transform.rs)
- ❌ Called from Python directly (use json_transform.rs)
- ❌ Need serde_json Value compatibility (use json_transform.rs)

---

## Performance Comparison

| Aspect | json_transform.rs | core::transform.rs |
|--------|-------------------|---------------------|
| **Strategy** | Parse → Transform → Serialize | Streaming (no parse) |
| **Input Type** | `&str` (JSON string) | `&[u8]` (raw bytes) |
| **Output Type** | `String` (JSON string) | `Vec<u8>` (raw bytes) |
| **Intermediate** | `serde_json::Value` tree | None (direct write) |
| **Memory** | 10-20x input size | ~1.2x input size |
| **Allocation** | Heap (Value tree) | Arena (scratch only) |
| **SIMD** | ❌ No | ✅ Yes (camelCase + escape) |
| **PyO3** | ✅ Exposed | ❌ Internal only |
| **Schema** | ✅ Schema-aware | ❌ Schema-agnostic |
| **Speed (typical)** | 0.5-2ms | 0.2-0.5ms (3-5x faster) |
| **Use Case** | Python API, schema transforms | Query pipeline, hot path |

---

## When to Use Which?

### Use `json_transform.rs` When:

1. **Called from Python**
   ```python
   # From Python code
   from fraiseql_rs import transform_json
   result = transform_json('{"user_name": "Alice"}')  # ← Uses json_transform.rs
   ```

2. **Need schema-aware transformation**
   ```rust
   // Handles nested object types via SchemaRegistry
   let transformed = transform_with_schema(&value, "User", &registry);
   ```

3. **Working with serde_json Values**
   ```rust
   use serde_json::Value;
   let value: Value = serde_json::from_str(json_str)?;
   let transformed = transform_value(value);  // ← Uses json_transform.rs
   ```

4. **Moderate performance requirements**
   - < 1000 requests/sec
   - Response times 1-5ms acceptable
   - Schema awareness more important than raw speed

### Use `core::transform.rs` When:

1. **GraphQL query pipeline (hot path)**
   ```rust
   // From pipeline/builder.rs
   let transformer = ZeroCopyTransformer::new(&arena, config, type_name, field_set);
   transformer.transform_bytes(row.as_bytes(), &mut output)?;
   ```

2. **High performance requirements**
   - > 1000 requests/sec
   - Response times < 1ms critical
   - Every microsecond counts

3. **Streaming transformation**
   ```rust
   // No JSON parsing, direct byte transformation
   for row in rows {
       transformer.transform_bytes(row.as_bytes(), &mut output)?;
   }
   ```

4. **Field projection**
   ```rust
   // Only include selected fields in output
   let field_set = FieldSet::from_paths(&selected_fields, &arena);
   let transformer = ZeroCopyTransformer::new(&arena, config, None, Some(&field_set));
   ```

---

## Architecture Decision Rationale

### Why Two Implementations?

**Historical Context**:
1. **Phase 1**: `json_transform.rs` created for PyO3 integration
   - Python-first API design
   - Schema-aware for type resolution
   - Value-based for flexibility

2. **Phase 2**: Performance bottleneck identified
   - JSON parsing overhead was 40-50% of query time
   - Value tree allocations were 2-3MB per 100 rows
   - Goal: Eliminate parsing, eliminate allocations

3. **Phase 3**: `core::transform.rs` created for pipeline
   - Zero-copy streaming transformation
   - Arena allocator for scratch space
   - SIMD optimizations for hot paths

4. **Phase 4**: Dual APIs maintained
   - Python API: `json_transform.rs` (flexibility)
   - Pipeline: `core::transform.rs` (performance)

### Why Not Consolidate?

**Option 1**: Make `json_transform.rs` call `core::transform.rs`
```rust
pub fn transform_json_string(json_str: &str) -> PyResult<String> {
    let arena = Arena::with_capacity(json_str.len());
    let transformer = ZeroCopyTransformer::new(&arena, config, None, None);
    let mut output = ByteBuf::new();
    transformer.transform_bytes(json_str.as_bytes(), &mut output)?;
    String::from_utf8(output.into_vec()).unwrap()
}
```
**Problem**: Loses schema-aware transformation capability

**Option 2**: Add schema support to `core::transform.rs`
```rust
pub fn transform_bytes_with_schema(&self, input: &[u8], output: &mut ByteBuf, registry: &SchemaRegistry) { ... }
```
**Problem**: Complicates zero-copy API, requires parsing for type lookups

**Decision**: Keep both implementations, document when to use each

---

## Future Work

### Potential Optimizations

1. **SIMD JSON Parsing** (simd-json crate)
   - Could speed up `json_transform.rs` by 2-3x
   - Requires AVX2/NEON support
   - Trade-off: More complex dependency

2. **Lazy Schema Resolution**
   - Cache type lookups in `json_transform.rs`
   - Could reduce O(1) → O(1) but with better constants

3. **Hybrid Approach**
   - Use `core::transform.rs` for simple cases
   - Fall back to `json_transform.rs` for schema-aware cases
   - Auto-detect at runtime

4. **Streaming Schema-Aware Transformation**
   - Extend `core::transform.rs` with optional schema support
   - Requires careful API design to maintain zero-copy

---

## References

- **PyO3 integration**: `src/lib.rs:67-71`
- **Value-based transformation**: `src/json_transform.rs:48-86`
- **Schema-aware transformation**: `src/json_transform.rs:88-200`
- **Zero-copy transformer**: `src/core/transform.rs:38-200`
- **Pipeline usage**: `src/pipeline/builder.rs:145-210`
- **CamelCase APIs**: See `camel-case-apis.md`

---

**Last Updated**: 2024-12-09
**Authors**: FraiseQL Core Team
**Related**: See `camel-case-apis.md` for camelCase implementation details
