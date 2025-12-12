# ⚠️ CRITICAL ADDENDUM: Rust Field Selection Required

## 🚨 Architectural Discovery

**The CTO feedback was incorrect about one key assumption.**

### Original CTO Recommendation:
> "Let GraphQL executor filter fields. No manual selection checking needed."

### Reality:
**FraiseQL mutations use `RustResponseBytes` which BYPASSES GraphQL executor entirely!**

---

## 📊 How FraiseQL Mutations Actually Work

### The Pass-Through Architecture

**File**: `src/fraiseql/graphql/execute.py`

```python
# Line 218-220: RustResponseBytes pass-through
if rust_response_marker["rust_response"] is not None:
    logger.debug("Detected RustResponseBytes via middleware - returning directly")
    return rust_response_marker["rust_response"]  # ⚠️ BYPASSES GraphQL filtering!
```

**Flow**:
1. Mutation resolver calls Rust (`execute_mutation_rust()`)
2. Rust builds complete JSON response with ALL fields
3. Returns `RustResponseBytes` (raw bytes)
4. GraphQL middleware detects `RustResponseBytes`
5. **Returns it DIRECTLY to HTTP layer** - NO GraphQL field filtering!

---

## 🔍 Evidence from Code

### Rust Builds Complete Response
**File**: `fraiseql_rs/src/mutation/response_builder.rs` (lines 100-112)

```rust
pub fn build_success_response(...) -> Result<Value, String> {
    let mut obj = Map::new();

    obj.insert("__typename".to_string(), json!(success_type));

    // ⚠️ Unconditionally adds ALL fields:
    if let Some(ref entity_id) = result.entity_id {
        obj.insert("id".to_string(), json!(entity_id));
    }
    obj.insert("message".to_string(), json!(result.message));
    obj.insert("status".to_string(), json!(result.status.to_string()));
    obj.insert("errors".to_string(), json!([]));
    // ... entity ...
    obj.insert("updatedFields".to_string(), json!(transformed_fields));

    Ok(json!(obj))  // ⚠️ Returns complete object with ALL fields
}
```

### Rust Has Selection Info But Doesn't Use It
**File**: `fraiseql_rs/src/mutation/response_builder.rs` (lines 217-251)

```rust
// Phase 3: Schema validation - check that all expected fields are present
if let Some(expected_fields) = success_type_fields {
    // ⚠️ Only VALIDATES, does NOT filter!
    for field in expected_fields {
        if !obj.contains_key(field) {
            missing_fields.push(field.clone());
        }
    }

    // Just prints warnings, does NOT filter response
    if !missing_fields.is_empty() {
        eprintln!("Schema validation warning: Missing expected fields...");
    }
}

Ok(Value::Object(obj))  // ⚠️ Returns complete object anyway
```

---

## ✅ Required Fix: Two-Part Solution

### Part 1: Python Decorator (As Planned)
Add fields to `__gql_fields__` so schema is correct.

**Status**: ✅ Phase 1 is correct

### Part 2: Rust Field Selection (NEW - REQUIRED)
Filter response based on `success_type_fields` parameter.

**Status**: ❌ Phase 2 needs major revision

---

## 🔧 Corrected Phase 2: Rust Response Filtering

### Rust Code Changes Required

**File**: `fraiseql_rs/src/mutation/response_builder.rs`

**BEFORE (lines 87-94)**:
```rust
pub fn build_success_response(
    result: &MutationResult,
    success_type: &str,
    entity_field_name: Option<&str>,
    auto_camel_case: bool,
    success_type_fields: Option<&Vec<String>>,  // ⚠️ Has selections but doesn't use them!
    cascade_selections: Option<&str>,
) -> Result<Value, String> {
```

**AFTER (NEW)**:
```rust
pub fn build_success_response(
    result: &MutationResult,
    success_type: &str,
    entity_field_name: Option<&str>,
    auto_camel_case: bool,
    success_type_fields: Option<&Vec<String>>,  // ✅ Now use this to filter!
    cascade_selections: Option<&str>,
) -> Result<Value, String> {
    let mut obj = Map::new();

    // Always add __typename (special GraphQL field)
    obj.insert("__typename".to_string(), json!(success_type));

    // ✅ NEW: Check if field selection filtering is active
    let should_filter = success_type_fields.is_some();
    let selected_fields = success_type_fields.unwrap_or(&vec![]);

    // ✅ Helper function to check if field is selected
    let is_selected = |field_name: &str| -> bool {
        !should_filter || selected_fields.contains(&field_name.to_string())
    };

    // Add id ONLY if selected
    if is_selected("id") {
        if let Some(ref entity_id) = result.entity_id {
            obj.insert("id".to_string(), json!(entity_id));
        }
    }

    // Add message ONLY if selected
    if is_selected("message") {
        obj.insert("message".to_string(), json!(result.message));
    }

    // Add status ONLY if selected
    if is_selected("status") {
        obj.insert("status".to_string(), json!(result.status.to_string()));
    }

    // Add errors ONLY if selected
    if is_selected("errors") {
        obj.insert("errors".to_string(), json!([]));
    }

    // Entity field - always check if selected
    if let Some(entity) = &result.entity {
        let entity_type = result.entity_type.as_deref().unwrap_or("Entity");
        let field_name = entity_field_name
            .map(|name| {
                if auto_camel_case { to_camel_case(name) } else { name.to_string() }
            })
            .unwrap_or_else(|| {
                if auto_camel_case { to_camel_case(&entity_type.to_lowercase()) }
                else { entity_type.to_lowercase() }
            });

        // Only add entity if field is selected
        if is_selected(&field_name) {
            let transformed = transform_entity(entity, entity_type, auto_camel_case);
            obj.insert(field_name, transformed);
        }
    }

    // Add updatedFields ONLY if selected
    if is_selected("updatedFields") {
        if let Some(fields) = &result.updated_fields {
            let transformed_fields: Vec<Value> = fields
                .iter()
                .map(|f| json!(if auto_camel_case { to_camel_case(f) } else { f.to_string() }))
                .collect();
            obj.insert("updatedFields".to_string(), json!(transformed_fields));
        }
    }

    // Cascade - only add if selected (existing logic already checks this)
    add_cascade_if_selected(&mut obj, result, cascade_selections, auto_camel_case)?;

    Ok(Value::Object(obj))
}
```

---

## 📋 Updated Implementation Timeline

### Phase 1: Python Decorator Fix (1.5h)
**No changes** - as originally planned

### Phase 2: Rust Field Selection (2h - NEW)
**Major changes required**:
1. Modify `build_success_response()` to filter based on `success_type_fields`
2. Modify `build_error_response_with_code()` similarly
3. Write Rust unit tests for field filtering
4. Rebuild with `maturin develop`
5. Test integration

### Phase 3: Integration & Verification (1h)
**Updated tests** - verify both Python schema AND Rust filtering

### Phase 4: Documentation & Commit (30m)
**No changes** - as originally planned

**NEW TOTAL**: 5 hours (not 3 hours)

---

## 🎯 Why This Is Required

### Without Rust Filtering:
```graphql
mutation {
  createMachine(input: {...}) {
    ... on CreateMachineSuccess {
      machine { id }  # Only request machine
    }
  }
}
```

**Current (WRONG)**:
```json
{
  "__typename": "CreateMachineSuccess",
  "id": "123",              // ⚠️ Not requested
  "message": "Created",     // ⚠️ Not requested
  "status": "success",      // ⚠️ Not requested
  "errors": [],             // ⚠️ Not requested
  "updatedFields": [],      // ⚠️ Not requested
  "machine": {...}          // ✅ Requested
}
```

**Expected (CORRECT)**:
```json
{
  "__typename": "CreateMachineSuccess",
  "machine": {...}  // ✅ Only requested field
}
```

---

## ✅ Corrected Success Criteria

- [ ] Python: Fields in `__gql_fields__` (schema correct)
- [ ] Rust: Fields filtered based on query selection
- [ ] Only requested fields in response (GraphQL spec)
- [ ] PrintOptim tests pass
- [ ] Performance not degraded (filtering is cheap)

---

## 🚀 Next Steps

1. **STOP** - Don't implement Phase 2 as originally written
2. **READ** this addendum completely
3. **REVISE** Phase 2 to include Rust filtering
4. **IMPLEMENT** both Python (Phase 1) and Rust (revised Phase 2)
5. **TEST** thoroughly to ensure correct field filtering

---

## 📝 Questions to Resolve

1. **Is `success_type_fields` parameter always populated?**
   - Need to check where it's set
   - If sometimes None, treat as "select all fields" (backward compat)

2. **What about nested entity fields?**
   - If entity is selected but user only wants some entity fields
   - Example: `machine { id }` - should whole machine be returned or filtered?
   - Likely: Return whole entity, let GraphQL executor handle nested filtering
     (entity is a sub-selection, not RustResponseBytes)

3. **Performance impact of filtering?**
   - Field lookup is O(1), filtering is O(n) where n = fields
   - Negligible for mutation responses (<20 fields typically)

---

**Status**: ⚠️ **IMPLEMENTATION BLOCKED** - Phase 2 needs complete revision

**Action Required**: Revise `.phases/mutation-schema-fix-v2/phase-2-integration-verification.md` to include Rust filtering implementation.

**Estimated NEW Timeline**: 5 hours total (was 3 hours)
