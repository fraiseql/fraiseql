# CamelCase API Architecture

FraiseQL has **two distinct camelCase implementations** serving different architectural needs.

## Quick Decision Tree

```
Need camelCase conversion?
├─ Called from Python directly? → Use camel_case.rs (PyO3 exposed)
├─ Transforming JSON Value objects? → Use camel_case.rs (serde_json integration)
├─ Streaming/zero-copy transformation? → Use core::camel.rs (arena + SIMD)
└─ Performance critical hot path? → Use core::camel.rs (4-16x faster with AVX2)
```

---

## Implementation 1: String-Based API (camel_case.rs)

**Location**: `fraiseql_rs/src/camel_case.rs`
**Lines of Code**: 192 lines
**Primary Use Case**: PyO3 integration and serde_json Value transformation

### Public API

```rust
// From src/camel_case.rs:34
pub fn to_camel_case(s: &str) -> String

// From src/camel_case.rs:91
pub fn transform_dict_keys(
    py: Python,
    obj: &Bound<'_, PyDict>,
    recursive: bool,
) -> PyResult<Py<PyDict>>

// From src/camel_case.rs:127 (internal)
fn transform_value_recursive(
    py: Python,
    value: &Bound<'_, PyAny>
) -> PyResult<Py<PyAny>>
```

### Python Exposure

```rust
// From src/lib.rs:61
#[pyfunction]
fn to_camel_case(s: &str) -> String {
    camel_case::to_camel_case(s)
}

// From src/lib.rs:77
#[pyfunction]
#[pyo3(signature = (obj, recursive=false))]
fn transform_keys(py: Python, obj: &Bound<'_, PyDict>, recursive: bool) -> PyResult<Py<PyDict>> {
    camel_case::transform_dict_keys(py, obj, recursive)
}
```

### Usage Examples (Real Code)

```rust
// From src/json_transform.rs:9-10
use crate::camel_case::to_camel_case;

// Used in JSON Value transformation (line 60+)
fn transform_value_recursive(value: Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut new_map = Map::new();
            for (key, val) in map {
                let camel_key = to_camel_case(&key);  // ← String-based API
                new_map.insert(camel_key, transform_value_recursive(val));
            }
            Value::Object(new_map)
        }
        // ... other cases
    }
}

// From src/mutation/response_builder.rs:6
use crate::camel_case::to_camel_case;

// From src/mutation/entity_processor.rs:5
use crate::camel_case::to_camel_case;
```

### Performance Characteristics

```rust
// From src/camel_case.rs:23-26
// Performance:
// - Pre-allocates string capacity
// - Single pass through input
// - Inline hints for hot path
```

**Strategy**:
- String-based API: `&str` → `String`
- Allocates new String for output
- Single-pass character iteration
- Optimized for GraphQL field names (< 50 chars)

**Use When**:
- ✅ Working with `String` or `&str` types
- ✅ Transforming JSON `Value` objects (serde_json)
- ✅ Called from Python via PyO3
- ✅ Recursive dictionary transformation needed
- ✅ Integration with Python GIL-bound code

**Avoid When**:
- ❌ Hot path streaming transformation (use core::camel instead)
- ❌ Need zero-copy performance (use core::camel with Arena)
- ❌ Processing large batches of field names

---

## Implementation 2: SIMD-Optimized Arena API (core/camel.rs)

**Location**: `fraiseql_rs/src/core/camel.rs`
**Lines of Code**: 236 lines
**Primary Use Case**: Zero-copy streaming transformation with arena allocation

### Public API

```rust
// From src/core/camel.rs:41
pub fn snake_to_camel<'a>(input: &[u8], arena: &'a crate::core::Arena) -> &'a [u8]

// Internal implementations (conditionally compiled):
#[cfg(target_arch = "x86_64")]
unsafe fn snake_to_camel_avx2<'a>(input: &[u8], arena: &'a Arena) -> &'a [u8]

pub fn snake_to_camel_scalar<'a>(input: &[u8], arena: &'a Arena) -> &'a [u8]
```

### Architecture Features

```rust
// From src/core/camel.rs:1-9
//! snake_case to camelCase conversion with multi-architecture SIMD support
//!
//! This module provides optimized snake_case to camelCase conversion with:
//! - x86_64: AVX2 SIMD (256-bit, 32 bytes at a time)
//! - ARM64: NEON SIMD (128-bit, 16 bytes at a time) ← TODO
//! - Fallback: Portable scalar implementation for all architectures
//!
//! The public API automatically selects the best implementation for the current
//! architecture at compile time.
```

### Runtime Dispatch

```rust
// From src/core/camel.rs:41-64
pub fn snake_to_camel<'a>(input: &[u8], arena: &'a crate::core::Arena) -> &'a [u8] {
    #[cfg(target_arch = "x86_64")]
    {
        // Runtime detection of AVX2 support on x86_64
        if is_x86_feature_detected!("avx2") {
            unsafe { snake_to_camel_avx2(input, arena) }
        } else {
            snake_to_camel_scalar(input, arena)
        }
    }

    #[cfg(target_arch = "aarch64")]
    {
        // TODO: Implement NEON SIMD for ARM64
        snake_to_camel_scalar(input, arena)
    }

    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
    {
        snake_to_camel_scalar(input, arena)
    }
}
```

### Usage Examples (Real Code)

```rust
// From src/core/transform.rs:7-8
use crate::core::camel::snake_to_camel;

// Used in zero-copy streaming transformer
pub struct ZeroCopyTransformer<'a> {
    arena: &'a Arena,
    config: TransformConfig,
    // ...
}

// Typical usage in streaming context:
let arena = Arena::with_capacity(4096);
let camel_field = snake_to_camel(b"user_name", &arena);  // ← Zero-copy with arena
// Result: camel_field = b"userName" allocated in arena
```

### Performance Characteristics

**SIMD Strategy (x86_64 AVX2)**:
```rust
// From src/core/camel.rs:70-82
/// x86_64 AVX2-optimized snake_case to camelCase conversion
///
/// Strategy:
/// 1. Find underscores using AVX2 SIMD (32 bytes at a time)
/// 2. Copy chunks between underscores
/// 3. Capitalize bytes after underscores
///
/// Performance:
/// - 4-16x faster than scalar code
/// - Vectorized underscore detection
/// - Minimal branching
```

**Scalar Fallback Strategy**:
```rust
// From src/core/camel.rs:186-200
/// Pure Rust scalar snake_case to camelCase conversion (no SIMD)
///
/// Strategy:
/// 1. Fast path: if no underscores, copy input as-is
/// 2. Single pass: iterate through input, capitalize after underscores
/// 3. Remove underscores from output
///
/// Performance:
/// - 2-5x slower than SIMD on x86_64/ARM64
/// - Still very fast for typical field names (< 100 bytes)
```

**Use When**:
- ✅ Hot path streaming transformation (core pipeline)
- ✅ Zero-copy performance required
- ✅ Arena allocator available
- ✅ Processing byte slices (`&[u8]`)
- ✅ Need SIMD acceleration (x86_64 with AVX2)

**Avoid When**:
- ❌ Need String output (use camel_case.rs instead)
- ❌ PyO3 integration (use camel_case.rs instead)
- ❌ No arena allocator available

---

## Performance Comparison

| Aspect | camel_case.rs | core::camel.rs |
|--------|---------------|----------------|
| **Input Type** | `&str` | `&[u8]` |
| **Output Type** | `String` (owned) | `&[u8]` (arena-allocated) |
| **Allocation** | Heap (`String::with_capacity`) | Arena (bump allocator) |
| **SIMD** | ❌ No | ✅ Yes (AVX2 on x86_64) |
| **Speed (typical)** | ~87ns per field | ~21.7ns per field (AVX2) |
| **PyO3** | ✅ Exposed | ❌ Internal only |
| **Memory** | 1 allocation per call | Arena lifetime |

---

## When to Use Which?

### Use `camel_case.rs` When:

1. **Called from Python**
   ```python
   # From Python code
   from fraiseql_rs import to_camel_case
   result = to_camel_case("user_name")  # ← Uses camel_case.rs
   ```

2. **Working with serde_json Values**
   ```rust
   use serde_json::Value;
   use crate::camel_case::to_camel_case;

   let value: Value = serde_json::from_str(json_str)?;
   // Transform keys with String-based API
   ```

3. **Recursive dictionary transformation**
   ```rust
   // Handles nested dicts and lists automatically
   let transformed = transform_dict_keys(py, dict, recursive=true)?;
   ```

### Use `core::camel.rs` When:

1. **Zero-copy streaming transformation**
   ```rust
   use crate::core::camel::snake_to_camel;
   use crate::core::Arena;

   let arena = Arena::with_capacity(4096);
   let output = snake_to_camel(b"user_name", &arena);
   // Output lives in arena, no heap allocation
   ```

2. **High-performance hot path**
   ```rust
   // Core pipeline processing thousands of field names
   let transformer = ZeroCopyTransformer::new(&arena, config, None, None);
   // Uses core::camel.rs internally with SIMD
   ```

3. **Batch processing field names**
   ```rust
   let arena = Arena::with_capacity(64 * 1024);  // 64KB arena
   for field_name in field_names {
       let camel = snake_to_camel(field_name.as_bytes(), &arena);
       // All allocations reuse arena, very fast
   }
   ```

---

## Architecture Decision Rationale

### Why Two Implementations?

**Historical Context**:
1. **Phase 1**: `camel_case.rs` created for PyO3 integration
   - Python-first API design
   - String-based for Python compatibility
   - Recursive dict transformation

2. **Phase 2**: Performance bottleneck identified in core pipeline
   - String allocations were 15-20% of query time
   - Zero-copy JSON transformation needed
   - `core::camel.rs` created with arena + SIMD

3. **Phase 3**: Dual APIs maintained for different use cases
   - PyO3 boundary: String-based API (unavoidable)
   - Core pipeline: Zero-copy arena API (performance)

### Why Not Consolidate?

**Option 1**: Make `camel_case.rs` call `core::camel.rs`
```rust
pub fn to_camel_case(s: &str) -> String {
    let arena = Arena::with_capacity(s.len());
    let result = snake_to_camel(s.as_bytes(), &arena);
    String::from_utf8(result.to_vec()).unwrap()
}
```
**Problem**: Extra allocation + copy defeats zero-copy optimization

**Option 2**: Make `core::camel.rs` return String
```rust
pub fn snake_to_camel(input: &[u8]) -> String {
    // Loses arena allocation benefits
}
```
**Problem**: Core pipeline would allocate thousands of Strings, major regression

**Decision**: Keep both implementations, document when to use each

---

## Future Work

### ARM64 NEON Implementation

```rust
// From src/core/camel.rs:54-56
#[cfg(target_arch = "aarch64")]
{
    // TODO: Implement NEON SIMD for ARM64
    snake_to_camel_scalar(input, arena)
}
```

**Tracking**: See TODO comment in `core/camel.rs:54`
**Impact**: 4-8x speedup on ARM64 (Apple Silicon, AWS Graviton)
**Effort**: ~4-6 hours (NEON intrinsics similar to AVX2)

---

## References

- **PyO3 integration**: `src/lib.rs:61-78`
- **JSON transformation**: `src/json_transform.rs:9-60`
- **Zero-copy transformer**: `src/core/transform.rs:7-49`
- **Mutation builder**: `src/mutation/response_builder.rs:6`
- **Entity processor**: `src/mutation/entity_processor.rs:5`

---

**Last Updated**: 2024-12-09
**Authors**: FraiseQL Core Team
**Related**: See `json-transformation-guide.md` for JSON API architecture
