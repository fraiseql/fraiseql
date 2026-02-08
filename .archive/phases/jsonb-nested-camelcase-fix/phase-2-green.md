# Phase 2: GREEN - Implement Fix

**Status**: Ready for Implementation (after Phase 1)
**Effort**: 2 hours
**Type**: TDD - Make Tests Pass

---

## Objective

Make all RED tests from Phase 1 pass with **minimal code changes**. Focus on fixing the bug, not perfecting the implementation.

---

## Prerequisites

- [ ] Phase 1 completed
- [ ] All new unit tests for `to_camel_case()` PASS
- [ ] Integration tests FAIL with expected error messages
- [ ] Control test (single-word field) PASSES

---

## Root Cause Analysis

Based on codebase investigation, FraiseQL has **two JSON transformation paths**:

### Path A: Schema-Aware (`json_transform.rs`)
- Entry: `build_with_schema()` in `pipeline/builder.rs:86`
- Uses `transform_with_schema()` for type-aware recursion
- Relies on `SchemaRegistry` for nested type resolution
- **Bug**: Falls back to basic recursion when field not in registry

### Path B: Zero-Copy Streaming (`core/transform.rs`)
- Entry: `build_zero_copy()` in `pipeline/builder.rs:145`
- Uses `ZeroCopyTransformer::transform_bytes()`
- Applies `snake_to_camel()` to keys at line 174
- **Bug**: No schema awareness - can't resolve nested types

### The Actual Bug

The bug is likely in **how nested JSONB objects are recursively transformed**:

1. When schema registry lookup fails for a field, it falls back to `transform_value()`
2. `transform_value()` does recursive key conversion BUT may not be called for all code paths
3. The zero-copy path may skip certain nested structures

---

## Investigation Commands

Run these BEFORE implementing to pinpoint the exact location:

### Step 1: Verify `to_camel_case` Works
```bash
python -c "from fraiseql._fraiseql_rs import to_camel_case; print(to_camel_case('smtp_server'), to_camel_case('dns_1'))"
# Expected: smtpServer dns1
```

### Step 2: Check `transform_json` Behavior
```bash
python -c "
from fraiseql._fraiseql_rs import transform_json
import json
data = {'smtp_server': {'ip_address': '1.2.3.4'}, 'dns_1': {'ip_address': '8.8.8.8'}}
result = transform_json(json.dumps(data))
print(result)
"
# If this outputs camelCase keys, bug is in GraphQL response building
# If snake_case, bug is in transform_json
```

### Step 3: Check `build_graphql_response` Behavior
```bash
python -c "
from fraiseql._fraiseql_rs import build_graphql_response
import json
data = {'smtp_server': {'ip_address': '1.2.3.4'}}
result = build_graphql_response([json.dumps(data)], 'test', 'Test', None, False)
print(json.loads(result))
"
# Check if nested keys are converted
```

### Step 4: Trace the Code Path
```bash
# Find where transform_value is defined
grep -n "fn transform_value" fraiseql_rs/src/json_transform.rs

# Find where nested objects are handled
grep -n "is_nested_object\|transform_nested" fraiseql_rs/src/json_transform.rs

# Check build_graphql_response
grep -n -A20 "pub fn build_graphql_response" fraiseql_rs/src/pipeline/builder.rs
```

---

## Likely Fix Locations

Based on investigation, the fix will be in ONE of these locations:

### Option A: `json_transform.rs` - Basic Transform Path

**File**: `fraiseql_rs/src/json_transform.rs`
**Function**: `transform_value()` (around line 91)

**Problem**: May not be recursively applied to all nested structures

**Check**: Look for how `Value::Object` is handled:
```rust
fn transform_value(value: Value) -> Value {
    match value {
        Value::Object(map) => {
            // Are ALL keys converted?
            // Is recursion applied to nested values?
        }
        // ...
    }
}
```

### Option B: `json_transform.rs` - Schema-Aware Path

**File**: `fraiseql_rs/src/json_transform.rs`
**Function**: `transform_with_schema()` (around line 192)

**Problem**: Fallback when field not in schema may not convert keys

**Check**: Look for the fallback path:
```rust
fn transform_with_schema(value: &Value, current_type: &str, registry: &SchemaRegistry) -> Value {
    // What happens when registry.get_field_type() returns None?
    // Does the fallback still convert keys?
}
```

### Option C: `pipeline/builder.rs` - Response Building

**File**: `fraiseql_rs/src/pipeline/builder.rs`
**Function**: `build_graphql_response()` or internal helpers

**Problem**: Transformation may not be applied to JSONB column content

**Check**: Ensure transform is called on JSON data:
```rust
// Is transform_value or transform_with_schema called on parsed JSON?
let transformed = transform_value(parsed_json);  // This should exist
```

### Option D: `core/transform.rs` - Zero-Copy Path

**File**: `fraiseql_rs/src/core/transform.rs`
**Function**: `transform_object()` (around line 128)

**Problem**: Nested objects may not be fully processed

**Check**: Ensure recursive handling:
```rust
fn transform_object(&mut self, ...) {
    // Does this recursively transform nested objects?
    // Line ~174: snake_to_camel is called for keys
    // But are nested objects also processed?
}
```

---

## Implementation Strategy

### If Bug is in `transform_value()`:

Ensure recursive transformation for all value types:

```rust
pub fn transform_value(value: Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut new_map = serde_json::Map::new();
            for (key, val) in map {
                let camel_key = to_camel_case(&key);
                let transformed_val = transform_value(val);  // RECURSIVE!
                new_map.insert(camel_key, transformed_val);
            }
            Value::Object(new_map)
        }
        Value::Array(arr) => {
            Value::Array(arr.into_iter().map(transform_value).collect())
        }
        other => other,
    }
}
```

### If Bug is in `transform_with_schema()`:

Ensure fallback path converts keys:

```rust
fn transform_with_schema(value: &Value, current_type: &str, registry: &SchemaRegistry) -> Value {
    match value {
        Value::Object(map) => {
            let mut new_map = serde_json::Map::new();
            for (key, val) in map {
                let camel_key = to_camel_case(&key);

                // Try schema-aware resolution first
                if let Some(field_info) = registry.get_field_type(current_type, &key) {
                    // Schema-aware path
                    let transformed = transform_with_schema(val, &field_info.type_name, registry);
                    new_map.insert(camel_key, transformed);
                } else {
                    // Fallback: still convert keys recursively!
                    let transformed = transform_value(val.clone());  // FIX: ensure this is called
                    new_map.insert(camel_key, transformed);
                }
            }
            Value::Object(new_map)
        }
        // ... arrays, primitives
    }
}
```

### If Bug is in Response Building:

Ensure `build_graphql_response` applies transformation:

```rust
pub fn build_graphql_response(...) -> Vec<u8> {
    // Parse JSON
    let parsed: Value = serde_json::from_str(&json_string)?;

    // MUST apply transformation
    let transformed = transform_value(parsed);  // or transform_with_schema

    // Build response with transformed data
    // ...
}
```

---

## Implementation Steps

### Step 1: Run Investigation Commands
Execute all commands in "Investigation Commands" section above.
Document findings.

### Step 2: Identify Exact Bug Location
Based on investigation:
- If `transform_json` works but `build_graphql_response` doesn't → Fix in builder
- If `transform_json` doesn't work → Fix in json_transform.rs
- If both work but integration test fails → Fix in JSONB column handling

### Step 3: Apply Minimal Fix
Edit the identified file with smallest possible change.

### Step 4: Rebuild Rust Extension
```bash
cd fraiseql_rs
maturin develop --release
cd ..
```

### Step 5: Run Unit Tests
```bash
uv run pytest tests/unit/core/test_jsonb_camelcase_conversion.py -v
```

### Step 6: Run Integration Tests
```bash
uv run pytest tests/regression/test_jsonb_nested_camelcase.py -v
```

### Step 7: Run Full Test Suite
```bash
uv run pytest tests/ -v --tb=short
```

---

## Troubleshooting

### Issue: Tests still fail after Rust changes
**Solution**: Ensure extension is rebuilt:
```bash
cd fraiseql_rs && maturin develop --release && cd ..
```

### Issue: Existing tests break
**Solution**: Check if change affects other patterns:
```bash
uv run pytest tests/regression/test_issue_112_nested_jsonb_typename.py -v
uv run pytest tests/integration/rust/test_camel_case.py -v
```

### Issue: Schema registry not initialized
**Solution**: The zero-copy path may be used. Check:
```bash
python -c "from fraiseql._fraiseql_rs import is_schema_registry_initialized; print(is_schema_registry_initialized())"
```

### Issue: dns_1 field still missing
**Solution**: Check GraphQL schema generation. The field must be defined as `dns1` in schema for query to work:
```bash
# In test, check schema introspection
query { __type(name: "NetworkConfiguration") { fields { name } } }
```

---

## Acceptance Criteria

- [ ] All Phase 1 unit tests PASS
- [ ] All Phase 1 integration tests PASS
- [ ] Existing tests still PASS (no regressions)
- [ ] `uv run pytest tests/ -v` shows no new failures
- [ ] Rust extension builds without warnings

---

## Expected Test Output

```bash
$ uv run pytest tests/unit/core/test_jsonb_camelcase_conversion.py -v
PASSED test_underscore_pattern_to_camelcase
PASSED test_underscore_number_pattern_to_camelcase
PASSED test_single_word_unchanged
PASSED test_already_camelcase_unchanged
PASSED test_transform_json_nested_dict
PASSED test_nested_object_keys_converted
PASSED test_array_item_keys_converted
PASSED test_deeply_nested_keys_converted

8 passed
```

```bash
$ uv run pytest tests/regression/test_jsonb_nested_camelcase.py -v
PASSED test_single_word_nested_object_converts_to_camelcase
PASSED test_underscore_nested_object_converts_to_camelcase
PASSED test_underscore_number_nested_object_is_present
PASSED test_array_nested_objects_convert_to_camelcase
PASSED test_all_nested_fields_in_single_query

5 passed
```

---

## Commit Message

```
fix(jsonb): convert nested JSONB object fields to camelCase [GREEN]

Fix bug where nested JSONB objects have their field names returned
as snake_case instead of camelCase in GraphQL responses.

Changes:
- [describe actual change based on investigation]

Fixes:
- smtp_server → smtpServer
- dns_1 → dns1
- print_servers → printServers

All Phase 1 RED tests now pass.
```

---

## DO NOT

- Do NOT over-engineer the solution
- Do NOT refactor unrelated code
- Do NOT add new features
- Do NOT optimize prematurely
- Do NOT change public API signatures

## DO

- DO make the minimal change to pass tests
- DO ensure no regressions
- DO rebuild Rust extension after changes
- DO verify with full test suite
- DO document the actual root cause in commit message

---

**Next Phase**: Phase 3 - REFACTOR (Clean up the implementation)
