# Phase 2: Rust Field Selection (GREEN → REFACTOR)

## 🎯 Objective

Implement field selection filtering in Rust response builder so that only requested fields are included in the mutation response.

**Time**: 2 hours

---

## 📋 Context

### The Problem

**FraiseQL mutations return `RustResponseBytes` which bypasses GraphQL executor's field filtering.**

Current flow:
1. Rust builds complete JSON with ALL fields
2. Returns `RustResponseBytes` (raw bytes)
3. GraphQL middleware detects it and returns directly to HTTP
4. **NO field filtering happens** - all fields returned even when not requested

### Why This Matters

**GraphQL Spec Violation**: Fields should only appear in response if explicitly requested in the query.

**Current behavior**:
```graphql
mutation {
  createMachine(input: {...}) {
    ... on CreateMachineSuccess {
      machine { id }  # Only request machine
    }
  }
}
```

Returns:
```json
{
  "id": "123",           // ❌ Not requested
  "message": "Created",  // ❌ Not requested
  "status": "success",   // ❌ Not requested
  "errors": [],          // ❌ Not requested
  "machine": {...}       // ✅ Requested
}
```

Expected:
```json
{
  "machine": {...}  // ✅ Only requested field
}
```

---

## 🔧 Implementation Steps

### Step 1: Understand Current Rust Code (10 min)

**File**: `fraiseql_rs/src/mutation/response_builder.rs`

**Key function**: `build_success_response()` (lines 87-254)

**Parameters**:
- `success_type_fields: Option<&Vec<String>>` - List of selected fields from GraphQL query
- Currently used only for validation warnings (lines 217-251)
- **NOT used for filtering** ❌

**Current logic**:
```rust
// Lines 100-112: Unconditionally adds ALL fields
obj.insert("id".to_string(), json!(entity_id));
obj.insert("message".to_string(), json!(result.message));
obj.insert("status".to_string(), json!(result.status.to_string()));
obj.insert("errors".to_string(), json!([]));

// Lines 217-251: Only validates, doesn't filter
if let Some(expected_fields) = success_type_fields {
    // Just prints warnings - doesn't remove unrequested fields!
    eprintln!("Schema validation warning: Extra fields...");
}
```

---

### Step 2: Write Rust Tests (RED) (30 min)

**File**: `fraiseql_rs/src/mutation/tests/response_building.rs`

Add tests for field selection filtering:

```rust
#[cfg(test)]
mod field_selection_tests {
    use super::*;
    use crate::mutation::{MutationResult, MutationStatus};
    use serde_json::json;

    #[test]
    fn test_success_response_filters_by_selection() {
        let result = MutationResult {
            status: MutationStatus::Success("success".to_string()),
            message: "Machine created".to_string(),
            entity_id: Some("123".to_string()),
            entity_type: Some("Machine".to_string()),
            entity: Some(json!({"id": "123", "name": "CNC-1"})),
            updated_fields: Some(vec!["name".to_string()]),
            cascade: None,
            metadata: None,
            is_simple_format: false,
        };

        // Only request 'machine' field
        let selected_fields = vec!["machine".to_string()];

        let response = build_success_response(
            &result,
            "CreateMachineSuccess",
            Some("machine"),
            true,
            Some(&selected_fields),
            None,
        )
        .unwrap();

        let obj = response.as_object().unwrap();

        // Should include __typename (always present)
        assert!(obj.contains_key("__typename"));

        // Should include requested field
        assert!(obj.contains_key("machine"), "machine should be present (requested)");

        // Should NOT include unrequested fields
        assert!(
            !obj.contains_key("id"),
            "id should NOT be present (not requested)"
        );
        assert!(
            !obj.contains_key("message"),
            "message should NOT be present (not requested)"
        );
        assert!(
            !obj.contains_key("status"),
            "status should NOT be present (not requested)"
        );
        assert!(
            !obj.contains_key("errors"),
            "errors should NOT be present (not requested)"
        );
        assert!(
            !obj.contains_key("updatedFields"),
            "updatedFields should NOT be present (not requested)"
        );
    }

    #[test]
    fn test_success_response_includes_selected_fields() {
        let result = MutationResult {
            status: MutationStatus::Success("success".to_string()),
            message: "Machine created".to_string(),
            entity_id: Some("123".to_string()),
            entity_type: Some("Machine".to_string()),
            entity: Some(json!({"id": "123", "name": "CNC-1"})),
            updated_fields: Some(vec!["name".to_string()]),
            cascade: None,
            metadata: None,
            is_simple_format: false,
        };

        // Request status, message, and machine
        let selected_fields = vec![
            "status".to_string(),
            "message".to_string(),
            "machine".to_string(),
        ];

        let response = build_success_response(
            &result,
            "CreateMachineSuccess",
            Some("machine"),
            true,
            Some(&selected_fields),
            None,
        )
        .unwrap();

        let obj = response.as_object().unwrap();

        // Should include requested fields
        assert!(obj.contains_key("status"), "status should be present");
        assert!(obj.contains_key("message"), "message should be present");
        assert!(obj.contains_key("machine"), "machine should be present");

        // Should NOT include unrequested fields
        assert!(!obj.contains_key("id"), "id should NOT be present");
        assert!(!obj.contains_key("errors"), "errors should NOT be present");
        assert!(
            !obj.contains_key("updatedFields"),
            "updatedFields should NOT be present"
        );
    }

    #[test]
    fn test_no_selection_returns_all_fields() {
        let result = MutationResult {
            status: MutationStatus::Success("success".to_string()),
            message: "Machine created".to_string(),
            entity_id: Some("123".to_string()),
            entity_type: Some("Machine".to_string()),
            entity: Some(json!({"id": "123", "name": "CNC-1"})),
            updated_fields: Some(vec!["name".to_string()]),
            cascade: None,
            metadata: None,
            is_simple_format: false,
        };

        // No field selection provided (None) - should return all fields
        let response = build_success_response(
            &result,
            "CreateMachineSuccess",
            Some("machine"),
            true,
            None,  // No selection - backward compatibility
            None,
        )
        .unwrap();

        let obj = response.as_object().unwrap();

        // All fields should be present when no selection provided
        assert!(obj.contains_key("id"), "id should be present");
        assert!(obj.contains_key("message"), "message should be present");
        assert!(obj.contains_key("status"), "status should be present");
        assert!(obj.contains_key("errors"), "errors should be present");
        assert!(obj.contains_key("machine"), "machine should be present");
        assert!(obj.contains_key("updatedFields"), "updatedFields should be present");
    }

    #[test]
    fn test_typename_always_present() {
        let result = MutationResult {
            status: MutationStatus::Success("success".to_string()),
            message: "Created".to_string(),
            entity_id: None,
            entity_type: Some("Machine".to_string()),
            entity: Some(json!({"id": "123"})),
            updated_fields: None,
            cascade: None,
            metadata: None,
            is_simple_format: false,
        };

        // Empty selection set - only __typename should be returned
        let selected_fields = vec![];

        let response = build_success_response(
            &result,
            "CreateMachineSuccess",
            Some("machine"),
            true,
            Some(&selected_fields),
            None,
        )
        .unwrap();

        let obj = response.as_object().unwrap();

        // __typename is always present (GraphQL spec)
        assert!(obj.contains_key("__typename"));
        assert_eq!(obj["__typename"], "CreateMachineSuccess");

        // No other fields should be present
        assert_eq!(obj.len(), 1, "Only __typename should be present");
    }

    #[test]
    fn test_error_response_filters_by_selection() {
        let result = MutationResult {
            status: MutationStatus::Error("failed:validation".to_string()),
            message: "Validation failed".to_string(),
            entity_id: None,
            entity_type: None,
            entity: None,
            updated_fields: None,
            cascade: None,
            metadata: None,
            is_simple_format: false,
        };

        // Only request 'errors' field
        let selected_fields = vec!["errors".to_string()];

        let response = build_error_response_with_code_filtered(
            &result,
            "CreateMachineError",
            true,
            None,
            Some(&selected_fields),
        )
        .unwrap();

        let obj = response.as_object().unwrap();

        // Should include __typename and errors
        assert!(obj.contains_key("__typename"));
        assert!(obj.contains_key("errors"));

        // Should NOT include unrequested fields
        assert!(!obj.contains_key("code"));
        assert!(!obj.contains_key("status"));
        assert!(!obj.contains_key("message"));
    }
}
```

**Run tests** (should FAIL):
```bash
cd fraiseql_rs
cargo test test_success_response_filters_by_selection -- --nocapture
```

---

### Step 3: Implement Rust Field Filtering (GREEN) (60 min)

**File**: `fraiseql_rs/src/mutation/response_builder.rs`

#### Modify `build_success_response()` function:

```rust
pub fn build_success_response(
    result: &MutationResult,
    success_type: &str,
    entity_field_name: Option<&str>,
    auto_camel_case: bool,
    success_type_fields: Option<&Vec<String>>,
    cascade_selections: Option<&str>,
) -> Result<Value, String> {
    let mut obj = Map::new();

    // Always add __typename (GraphQL special field - always present)
    obj.insert("__typename".to_string(), json!(success_type));

    // ✅ NEW: Helper to check if field should be included
    let should_include_field = |field_name: &str| -> bool {
        match success_type_fields {
            None => true,  // No selection = include all (backward compat)
            Some(fields) => fields.contains(&field_name.to_string()),
        }
    };

    // Add id ONLY if selected
    if should_include_field("id") {
        if let Some(ref entity_id) = result.entity_id {
            obj.insert("id".to_string(), json!(entity_id));
        }
    }

    // Add message ONLY if selected
    if should_include_field("message") {
        obj.insert("message".to_string(), json!(result.message));
    }

    // Add status ONLY if selected
    if should_include_field("status") {
        obj.insert("status".to_string(), json!(result.status.to_string()));
    }

    // Add errors ONLY if selected
    if should_include_field("errors") {
        obj.insert("errors".to_string(), json!([]));
    }

    // v1.8.0: SUCCESS MUST HAVE ENTITY (non-null guarantee)
    if result.entity.is_none() {
        return Err(format!(
            "Success type '{}' requires non-null entity. \
             Status '{}' returned null entity. \
             This indicates a logic error: non-success statuses (noop:*, failed:*, etc.) \
             should return Error type, not Success type.",
            success_type,
            result.status.to_string()
        ));
    }

    // Add entity with __typename and camelCase keys
    if let Some(entity) = &result.entity {
        let entity_type = result.entity_type.as_deref().unwrap_or("Entity");

        // Determine the field name for the entity in the response
        let field_name = entity_field_name
            .map(|name| {
                if auto_camel_case {
                    to_camel_case(name)
                } else {
                    name.to_string()
                }
            })
            .unwrap_or_else(|| {
                if auto_camel_case {
                    to_camel_case(&entity_type.to_lowercase())
                } else {
                    entity_type.to_lowercase()
                }
            });

        // ✅ NEW: Only add entity if field is selected
        if should_include_field(&field_name) {
            // Check if entity is a wrapper object containing entity_field_name
            let actual_entity = if let Value::Object(entity_map) = entity {
                if let Some(entity_field_name_raw) = entity_field_name {
                    if let Some(nested_entity) = entity_map.get(entity_field_name_raw) {
                        nested_entity
                    } else {
                        entity
                    }
                } else {
                    entity
                }
            } else {
                entity
            };

            let transformed = transform_entity(actual_entity, entity_type, auto_camel_case);
            obj.insert(field_name.clone(), transformed);

            // If entity was a wrapper, copy other fields from it (like "message")
            // But ONLY if those fields are selected
            if let Value::Object(entity_map) = entity {
                if let Some(entity_field_name_raw) = entity_field_name {
                    if entity_map.contains_key(entity_field_name_raw) {
                        for (key, value) in entity_map {
                            if key != entity_field_name_raw && key != "entity" && key != "cascade" {
                                let field_key = if auto_camel_case {
                                    to_camel_case(key)
                                } else {
                                    key.clone()
                                };

                                // ✅ NEW: Only add if selected AND not already present
                                if should_include_field(&field_key) && !obj.contains_key(&field_key) {
                                    obj.insert(field_key, transform_value(value, auto_camel_case));
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // Add updatedFields ONLY if selected
    if should_include_field("updatedFields") {
        if let Some(fields) = &result.updated_fields {
            let transformed_fields: Vec<Value> = fields
                .iter()
                .map(|f| {
                    json!(if auto_camel_case {
                        to_camel_case(f)
                    } else {
                        f.to_string()
                    })
                })
                .collect();
            obj.insert("updatedFields".to_string(), json!(transformed_fields));
        }
    }

    // Add cascade if present AND requested in selection
    // Note: cascade has its own selection filtering via cascade_selections
    if should_include_field("cascade") {
        add_cascade_if_selected(&mut obj, result, cascade_selections, auto_camel_case)?;
    }

    // ✅ REMOVED: Schema validation warnings (lines 217-251)
    // Field filtering now handles this automatically - response only contains selected fields

    Ok(Value::Object(obj))
}
```

#### Also modify `build_error_response_with_code()`:

```rust
pub fn build_error_response_with_code(
    result: &MutationResult,
    error_type: &str,
    auto_camel_case: bool,
    cascade_selections: Option<&str>,
    error_type_fields: Option<&Vec<String>>,  // ✅ NEW parameter
) -> Result<Value, String> {
    let mut obj = Map::new();

    // Always add __typename
    obj.insert("__typename".to_string(), json!(error_type));

    // ✅ NEW: Helper to check if field should be included
    let should_include_field = |field_name: &str| -> bool {
        match error_type_fields {
            None => true,  // No selection = include all (backward compat)
            Some(fields) => fields.contains(&field_name.to_string()),
        }
    };

    // Add code ONLY if selected
    if should_include_field("code") {
        let code = map_status_to_code(&result.status);
        obj.insert("code".to_string(), json!(code));
    }

    // Add status ONLY if selected
    if should_include_field("status") {
        obj.insert("status".to_string(), json!(result.status.to_string()));
    }

    // Add message ONLY if selected
    if should_include_field("message") {
        obj.insert("message".to_string(), json!(result.message));
    }

    // Add errors array ONLY if selected
    if should_include_field("errors") {
        let code = map_status_to_code(&result.status);
        let errors = generate_errors_array(result, code)?;
        obj.insert("errors".to_string(), errors);
    }

    // Add cascade ONLY if selected
    if should_include_field("cascade") {
        add_cascade_if_selected(&mut obj, result, cascade_selections, auto_camel_case)?;
    }

    Ok(Value::Object(obj))
}
```

#### Update function signature for `build_graphql_response()`:

Find the call to `build_error_response_with_code()` and update it to pass `error_type_fields`:

```rust
pub fn build_graphql_response(
    result: &MutationResult,
    field_name: &str,
    success_type: &str,
    error_type: &str,
    entity_field_name: Option<&str>,
    _entity_type: Option<&str>,
    auto_camel_case: bool,
    success_type_fields: Option<&Vec<String>>,
    error_type_fields: Option<&Vec<String>>,  // ✅ NEW parameter
    cascade_selections: Option<&str>,
) -> Result<Value, String> {
    let response_obj = if result.status.is_success() {
        build_success_response(
            result,
            success_type,
            entity_field_name,
            auto_camel_case,
            success_type_fields,
            cascade_selections,
        )?
    } else {
        build_error_response_with_code(
            result,
            error_type,
            auto_camel_case,
            cascade_selections,
            error_type_fields,  // ✅ NEW parameter
        )?
    };

    Ok(json!({
        "data": {
            field_name: response_obj
        }
    }))
}
```

---

### Step 4: Update Python to Pass Error Type Fields (15 min)

**File**: `src/fraiseql/mutations/rust_executor.py`

Find where `build_graphql_response()` is called and add `error_type_fields` parameter:

```python
async def execute_mutation_rust(
    mutation_result: MutationResult,
    mutation_name: str,
    success_type: str,
    error_type: str,
    entity_field_name: str | None = None,
    entity_type: str | None = None,
    auto_camel_case: bool = True,
    success_type_fields: list[str] | None = None,
    error_type_fields: list[str] | None = None,  # ✅ NEW parameter
    cascade_selections: str | None = None,
) -> RustResponseBytes:
    """Execute mutation using Rust response builder.

    Args:
        mutation_result: The mutation result from database
        mutation_name: GraphQL mutation field name
        success_type: GraphQL success type name
        error_type: GraphQL error type name
        entity_field_name: Field name for entity in response
        entity_type: GraphQL entity type name
        auto_camel_case: Whether to convert snake_case to camelCase
        success_type_fields: List of fields selected for success type
        error_type_fields: List of fields selected for error type
        cascade_selections: CASCADE field selections as JSON

    Returns:
        RustResponseBytes ready for HTTP response
    """
    # ... existing code ...

    response_bytes = fraiseql_rs.build_graphql_response(
        raw_result,
        mutation_name,
        success_type,
        error_type,
        entity_field_name,
        entity_type,
        auto_camel_case,
        success_type_fields,
        error_type_fields,  # ✅ NEW parameter
        cascade_selections,
    )

    return RustResponseBytes(response_bytes, schema_type=success_type)
```

**Find all call sites** and update them to pass `error_type_fields`:

```bash
cd src/fraiseql
grep -r "execute_mutation_rust" --include="*.py"
```

Update each call site to extract error type fields from the GraphQL info object.

---

### Step 5: Rebuild Rust Extension (5 min)

```bash
cd fraiseql_rs

# Run Rust tests
cargo test field_selection -- --nocapture

# If tests pass, rebuild Python extension
cd ..
maturin develop --release

# Verify rebuild
python3 -c "import fraiseql_rs; print('Rust extension rebuilt successfully')"
```

---

### Step 6: Integration Test (10 min)

**Quick Python test** to verify Rust filtering works:

```python
# test_rust_field_selection.py
import asyncio
import pytest
from fraiseql.mutations.rust_executor import execute_mutation_rust
from fraiseql.mutations.types import MutationResult, MutationStatus


@pytest.mark.asyncio
async def test_rust_filters_fields():
    """Verify Rust only returns selected fields."""

    result = MutationResult(
        status=MutationStatus.SUCCESS,
        message="Machine created",
        entity_id="123",
        entity_type="Machine",
        entity={"id": "123", "name": "CNC-1"},
        updated_fields=["name"],
        cascade=None,
        metadata=None,
    )

    # Only select 'machine' field
    selected_fields = ["machine"]

    response = await execute_mutation_rust(
        mutation_result=result,
        mutation_name="createMachine",
        success_type="CreateMachineSuccess",
        error_type="CreateMachineError",
        entity_field_name="machine",
        entity_type="Machine",
        auto_camel_case=True,
        success_type_fields=selected_fields,
        error_type_fields=None,
        cascade_selections=None,
    )

    import json
    response_json = json.loads(response.to_json())
    data = response_json["data"]["createMachine"]

    # Should include __typename and machine
    assert "__typename" in data
    assert "machine" in data

    # Should NOT include unrequested fields
    assert "id" not in data, "id should not be present (not requested)"
    assert "message" not in data, "message should not be present"
    assert "status" not in data, "status should not be present"
    assert "errors" not in data, "errors should not be present"
    assert "updatedFields" not in data, "updatedFields should not be present"


@pytest.mark.asyncio
async def test_rust_includes_all_when_no_selection():
    """Verify backward compatibility - no selection = all fields."""

    result = MutationResult(
        status=MutationStatus.SUCCESS,
        message="Machine created",
        entity_id="123",
        entity_type="Machine",
        entity={"id": "123", "name": "CNC-1"},
        updated_fields=["name"],
        cascade=None,
        metadata=None,
    )

    # No field selection
    response = await execute_mutation_rust(
        mutation_result=result,
        mutation_name="createMachine",
        success_type="CreateMachineSuccess",
        error_type="CreateMachineError",
        entity_field_name="machine",
        entity_type="Machine",
        auto_camel_case=True,
        success_type_fields=None,  # No selection
        error_type_fields=None,
        cascade_selections=None,
    )

    import json
    response_json = json.loads(response.to_json())
    data = response_json["data"]["createMachine"]

    # All fields should be present
    assert "id" in data
    assert "message" in data
    assert "status" in data
    assert "errors" in data
    assert "machine" in data
    assert "updatedFields" in data
```

**Run**:
```bash
pytest tests/integration/test_rust_field_selection.py -xvs
```

---

## ✅ Acceptance Criteria

- [ ] Rust tests pass (field selection filtering)
- [ ] Python integration tests pass
- [ ] Only requested fields in response
- [ ] `None` selection returns all fields (backward compat)
- [ ] `__typename` always present
- [ ] Both success and error types filtered
- [ ] No performance regression

---

## 🔍 Verification Commands

```bash
# Rust tests
cd fraiseql_rs
cargo test field_selection -- --nocapture

# Rebuild extension
cd ..
maturin develop --release

# Python integration tests
pytest tests/integration/test_rust_field_selection.py -xvs

# Check that existing tests still pass
pytest tests/unit/mutations/ -v
```

---

## 🐛 Common Issues & Solutions

### Issue 1: Rust compilation errors
**Cause**: Function signature changes

**Solution**: Make sure ALL call sites are updated:
- `build_graphql_response()` signature
- `build_error_response_with_code()` signature
- Python FFI bindings in `src/lib.rs`

### Issue 2: Tests fail with "field not found"
**Cause**: Field selection too aggressive

**Solution**: Check `should_include_field()` logic - ensure `None` selection returns `true`

### Issue 3: `__typename` missing
**Cause**: `__typename` being filtered

**Solution**: Always add `__typename` BEFORE checking field selection

---

## 📊 Performance Considerations

**Field filtering is O(n) where n = number of fields**:
- Typical mutation response: 5-10 fields
- Field lookup in Vec: O(n) but n is small
- Overall impact: <1ms per mutation

**If performance is a concern**:
- Convert `Vec<String>` to `HashSet<String>` for O(1) lookups
- Only do this if profiling shows it's a bottleneck

---

## 🚫 DO NOT

- ❌ Remove `__typename` from response (always required)
- ❌ Filter nested entity fields (let GraphQL handle sub-selections)
- ❌ Break backward compatibility (None selection = all fields)
- ❌ Filter special GraphQL fields starting with `__`

---

**Next**: [Phase 3: Integration & Verification](./phase-3-integration-verification.md)
