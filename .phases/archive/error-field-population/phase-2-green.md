# Phase 2: GREEN - Implement Error Field Population

## Objective

Implement the minimum Rust code changes to make all Phase 1 tests pass. Focus on getting functionality working, not on perfect code structure (refactoring comes in Phase 3).

## Context

**Current State**: 5 tests failing, 1 passing (reserved fields test)

**Target State**: All 6 tests passing

**Reference Implementation**: `build_success_response()` in response_builder.rs:45-188 already has the pattern we need to replicate for errors.

## Files to Modify

1. `fraiseql_rs/src/mutation/response_builder.rs` - Add field extraction logic
2. `src/fraiseql/mutations/rust_executor.py` - Pass error class fields to Rust

## Implementation Steps

### Step 1: Update Rust Function Signature

**File**: `fraiseql_rs/src/mutation/response_builder.rs`

**Location**: Line ~196 (function definition)

**Change**:

```rust
// BEFORE
pub fn build_error_response(
    result: &MutationResult,
    error_type: &str,
    auto_camel_case: bool,
) -> Result<Value, String> {

// AFTER
pub fn build_error_response(
    result: &MutationResult,
    error_type: &str,
    auto_camel_case: bool,
    error_type_fields: Option<&Vec<String>>,  // NEW: Expected error class fields
) -> Result<Value, String> {
```

### Step 2: Extract Custom Fields from Entity

**File**: `fraiseql_rs/src/mutation/response_builder.rs`

**Location**: After the `errors` array insertion (line ~253), before `Ok(Value::Object(obj))`

**Add**:

```rust
    // Add errors array
    // ... existing code ...
    obj.insert("errors".to_string(), json!([auto_error]));

    // NEW: Populate custom fields from entity
    if let Some(entity) = &result.entity {
        if let Value::Object(entity_map) = entity {
            if let Some(expected_fields) = error_type_fields {
                for field_name in expected_fields {
                    // Skip reserved error fields (already added above)
                    if is_reserved_error_field(field_name) {
                        continue;
                    }

                    // Check if entity has this field
                    if let Some(field_value) = entity_map.get(field_name) {
                        let camel_key = if auto_camel_case {
                            to_camel_case(field_name)
                        } else {
                            field_name.clone()
                        };

                        // Transform value (add __typename if it's an entity)
                        let transformed = transform_field_value(
                            field_value,
                            field_name,
                            auto_camel_case
                        );

                        obj.insert(camel_key, transformed);
                    }
                }
            }
        }
    }

    // NEW: Also check metadata as fallback
    if let Some(metadata) = &result.metadata {
        if let Value::Object(metadata_map) = metadata {
            if let Some(expected_fields) = error_type_fields {
                for field_name in expected_fields {
                    // Skip if already populated from entity or reserved
                    if obj.contains_key(field_name) || is_reserved_error_field(field_name) {
                        continue;
                    }

                    if let Some(field_value) = metadata_map.get(field_name) {
                        let camel_key = if auto_camel_case {
                            to_camel_case(field_name)
                        } else {
                            field_name.clone()
                        };

                        let transformed = transform_field_value(
                            field_value,
                            field_name,
                            auto_camel_case
                        );

                        obj.insert(camel_key, transformed);
                    }
                }
            }
        }
    }

    Ok(Value::Object(obj))
}
```

### Step 3: Add Helper Function for Reserved Fields

**File**: `fraiseql_rs/src/mutation/response_builder.rs`

**Location**: After `build_error_response()` function, before `transform_entity()`

**Add**:

```rust
/// Check if field name is a reserved error field
fn is_reserved_error_field(field_name: &str) -> bool {
    matches!(
        field_name,
        "message" | "status" | "code" | "errors" | "__typename"
    )
}
```

### Step 4: Add Helper Function for Field Value Transformation

**File**: `fraiseql_rs/src/mutation/response_builder.rs`

**Location**: After `is_reserved_error_field()`

**Add**:

```rust
/// Transform field value: add __typename if it's an entity, apply camelCase
fn transform_field_value(
    value: &Value,
    field_name: &str,
    auto_camel_case: bool,
) -> Value {
    match value {
        Value::Object(map) => {
            // Check if this looks like an entity (has 'id' field or nested structure)
            // If so, infer entity type from field name and add __typename
            let entity_type = infer_entity_type_from_field_name(field_name);

            if !entity_type.is_empty() {
                // Transform as entity (adds __typename)
                transform_entity(value, &entity_type, auto_camel_case)
            } else {
                // Transform as regular value (no __typename)
                transform_value(value, auto_camel_case)
            }
        }
        Value::Array(arr) => {
            // Check if array contains entities
            if let Some(first) = arr.first() {
                if first.is_object() {
                    let entity_type = infer_entity_type_from_field_name(field_name);
                    if !entity_type.is_empty() {
                        // Array of entities
                        return Value::Array(
                            arr.iter()
                                .map(|item| transform_entity(item, &entity_type, auto_camel_case))
                                .collect()
                        );
                    }
                }
            }
            // Array of primitives or non-entities
            Value::Array(
                arr.iter()
                    .map(|item| transform_value(item, auto_camel_case))
                    .collect()
            )
        }
        _ => value.clone(),
    }
}

/// Infer entity type from field name
/// Examples:
///   conflict_dns_server -> DnsServer
///   validation_errors -> ValidationError
///   dns_server -> DnsServer
fn infer_entity_type_from_field_name(field_name: &str) -> String {
    // Remove common prefixes
    let cleaned = field_name
        .trim_start_matches("conflict_")
        .trim_start_matches("existing_")
        .trim_start_matches("related_");

    // Remove plural suffix for array fields
    let singular = if cleaned.ends_with("_errors") {
        cleaned.trim_end_matches("s") // validation_errors -> validation_error
    } else if cleaned.ends_with("ies") {
        // entities -> entity
        format!("{}y", cleaned.trim_end_matches("ies"))
    } else if cleaned.ends_with('s') && !cleaned.ends_with("ss") {
        // dns_servers -> dns_server (but not address -> addres)
        cleaned.trim_end_matches('s')
    } else {
        cleaned
    };

    // Convert to PascalCase
    singular
        .split('_')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => {
                    first.to_uppercase().collect::<String>() + chars.as_str()
                }
            }
        })
        .collect::<String>()
}
```

### Step 5: Update Python Caller to Pass Error Fields

**File**: `src/fraiseql/mutations/rust_executor.py`

**Location**: Search for the call to `fraiseql_rs.build_error_response()` (around line 200-250)

**Change**:

```python
# BEFORE
error_response = fraiseql_rs.build_error_response(
    result_dict,
    error_cls.__name__,
    auto_camel_case=auto_camel_case,
)

# AFTER
# Extract error class field names
error_type_fields = None
if hasattr(error_cls, '__annotations__'):
    error_type_fields = list(error_cls.__annotations__.keys())

error_response = fraiseql_rs.build_error_response(
    result_dict,
    error_cls.__name__,
    auto_camel_case=auto_camel_case,
    error_type_fields=error_type_fields,  # NEW
)
```

### Step 6: Update Rust Python Bindings

**File**: `fraiseql_rs/src/lib.rs`

**Location**: Find the `build_error_response` PyO3 wrapper function

**Change**:

```rust
// BEFORE
#[pyfunction]
fn build_error_response(
    result: &PyDict,
    error_type: &str,
    auto_camel_case: bool,
) -> PyResult<PyObject> {

// AFTER
#[pyfunction]
fn build_error_response(
    result: &PyDict,
    error_type: &str,
    auto_camel_case: bool,
    error_type_fields: Option<Vec<String>>,  // NEW
) -> PyResult<PyObject> {
```

**Also update the call to the actual implementation**:

```rust
// BEFORE
let result = mutation::response_builder::build_error_response(
    &rust_result,
    error_type,
    auto_camel_case,
)?;

// AFTER
let result = mutation::response_builder::build_error_response(
    &rust_result,
    error_type,
    auto_camel_case,
    error_type_fields.as_ref(),  // NEW
)?;
```

### Step 7: Rebuild Rust Extension

```bash
# Rebuild the Rust extension with new bindings
cd fraiseql_rs
cargo build --release
cd ..

# Or use maturin if that's the build system
maturin develop --release
```

### Step 8: Run Tests and Verify GREEN

```bash
# Run the error field population tests
uv run pytest tests/integration/mutations/test_error_field_population.py -v

# Expected: All 6 tests PASS
```

## Verification Commands

### Full Test Suite

```bash
# Run all error field population tests
uv run pytest tests/integration/mutations/test_error_field_population.py -v

# Expected output:
# test_error_field_from_entity_object PASSED
# test_error_scalar_field_from_entity PASSED
# test_error_field_from_metadata PASSED
# test_error_field_camelcase_transformation PASSED
# test_error_nested_entity_typename PASSED
# test_error_reserved_fields_not_overridden PASSED
#
# ====== 6 passed in X.XXs ======
```

### Spot Check Individual Tests

```bash
# Test entity field population
uv run pytest tests/integration/mutations/test_error_field_population.py::TestErrorFieldPopulation::test_error_field_from_entity_object -v

# Test metadata field population
uv run pytest tests/integration/mutations/test_error_field_population.py::TestErrorFieldPopulation::test_error_field_from_metadata -v

# Test camelCase transformation
uv run pytest tests/integration/mutations/test_error_field_population.py::TestErrorFieldPopulation::test_error_field_camelcase_transformation -v
```

### Regression Check (Ensure Existing Tests Still Pass)

```bash
# Run all mutation tests to ensure we didn't break anything
uv run pytest tests/integration/mutations/ -v

# Run all integration tests
uv run pytest tests/integration/ -v
```

## Acceptance Criteria

- [ ] `build_error_response()` signature updated with `error_type_fields` parameter
- [ ] Custom fields extracted from `result.entity`
- [ ] Custom fields extracted from `result.metadata` as fallback
- [ ] Reserved fields (`message`, `status`, `code`, `errors`, `__typename`) not overridden
- [ ] CamelCase transformation applied when `auto_camel_case=true`
- [ ] Nested entities get `__typename` added automatically
- [ ] Helper functions added:
  - [ ] `is_reserved_error_field()`
  - [ ] `transform_field_value()`
  - [ ] `infer_entity_type_from_field_name()`
- [ ] Python caller updated to pass error class fields
- [ ] Rust PyO3 bindings updated
- [ ] **All 6 Phase 1 tests now PASS**
- [ ] **No existing tests broken** (regression check)

## Known Issues / Technical Debt

These are acceptable in GREEN phase (will be fixed in REFACTOR):

- `infer_entity_type_from_field_name()` is heuristic-based (not perfect)
- Code duplication between entity/metadata extraction loops
- No validation that inferred entity types actually exist in schema
- Error messages could be more helpful (no warnings for missing fields)

## DO NOT

- ❌ Optimize performance yet (Phase 3)
- ❌ Extract shared code between success/error builders (Phase 3)
- ❌ Add extensive error handling beyond what's needed for tests (Phase 3)
- ❌ Write additional tests (that's Phase 1 or Phase 4)
- ❌ Update documentation (Phase 4)

## Next Phase

After all tests pass (GREEN), proceed to **Phase 3: REFACTOR** to improve code quality, extract shared helpers, and add better validation.
