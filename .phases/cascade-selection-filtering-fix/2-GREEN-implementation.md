# Phase 2: GREEN - Implement Selection Filtering

**Objective**: Implement minimal changes to make all RED phase tests pass.

**Status**: 🟢 GREEN (Make tests pass)

---

## Context

Tests from Phase 1 are failing because CASCADE is always included in responses. We need to:

1. Extract CASCADE selection from GraphQL query
2. Pass selection info from Python → Rust
3. Filter CASCADE response based on selection

**Key Insight**: Most infrastructure already exists in `cascade_selections.py`!

---

## Files to Modify

1. `fraiseql/mutations/executor.py` - Extract CASCADE selections, pass to Rust
2. `fraiseql_rs/src/mutation/mod.rs` - Accept and use cascade_selections param
3. `fraiseql_rs/src/mutation/response_builder.rs` - Filter CASCADE based on selections
4. `fraiseql_rs/src/mutation/cascade_filter.rs` (NEW) - Rust selection filtering logic

---

## Implementation Steps

### Step 1: Python - Extract CASCADE Selections

**File**: `fraiseql/mutations/executor.py`

**Location**: In the mutation execution logic where Rust pipeline is called

```python
# Around line 150-200 (where build_mutation_response is called)

# ADD: Import cascade selection extractor
from fraiseql.mutations.cascade_selections import extract_cascade_selections

# FIND: The call to build_mutation_response
# BEFORE:
result_bytes = build_mutation_response(
    json.dumps(result_data),
    success_type_name,
    entity_field_name,
    None,  # cascade_selections - UNUSED
    auto_camel_case,
    success_type_fields,
)

# CHANGE TO:
cascade_selections_json = None
if self.enable_cascade and info:
    cascade_selections_json = extract_cascade_selections(info)

result_bytes = build_mutation_response(
    json.dumps(result_data),
    success_type_name,
    entity_field_name,
    cascade_selections_json,  # Now passing actual selections
    auto_camel_case,
    success_type_fields,
)
```

**Changes**:

1. Import `extract_cascade_selections` from `cascade_selections.py`
2. Extract CASCADE selections from GraphQL query when `enable_cascade=True`
3. Pass extracted selections (JSON string or None) to Rust

---

### Step 2: Rust - Accept CASCADE Selections Parameter

**File**: `fraiseql_rs/src/mutation/mod.rs`

**Location**: `build_mutation_response` function signature

```rust
// FIND (around line 43-53):
pub fn build_mutation_response(
    result_json: &str,
    success_type: &str,
    entity_field_name: Option<&str>,
    _cascade_selections: Option<&str>,  // ← Prefixed with underscore (unused)
    auto_camel_case: bool,
    success_type_fields: Option<Vec<String>>,
) -> Result<Vec<u8>, String> {

// CHANGE TO:
pub fn build_mutation_response(
    result_json: &str,
    success_type: &str,
    entity_field_name: Option<&str>,
    cascade_selections: Option<&str>,  // ← Remove underscore prefix
    auto_camel_case: bool,
    success_type_fields: Option<Vec<String>>,
) -> Result<Vec<u8>, String> {
```

**Then pass to response builder**:

```rust
// FIND: Call to build_success_response (around line 120-130)
let success_value = build_success_response(
    &mutation_result,
    success_type,
    entity_field_name,
    auto_camel_case,
    success_type_fields.as_ref(),
)?;

// CHANGE TO:
let success_value = build_success_response(
    &mutation_result,
    success_type,
    entity_field_name,
    auto_camel_case,
    success_type_fields.as_ref(),
    cascade_selections,  // Pass cascade selections
)?;
```

---

### Step 3: Rust - Update Response Builder Signature

**File**: `fraiseql_rs/src/mutation/response_builder.rs`

**Location**: `build_success_response` function

```rust
// FIND (around line 15-25):
pub fn build_success_response(
    result: &MutationResult,
    success_type: &str,
    entity_field_name: Option<&str>,
    auto_camel_case: bool,
    success_type_fields: Option<&Vec<String>>,
) -> Result<Value, String> {

// CHANGE TO:
pub fn build_success_response(
    result: &MutationResult,
    success_type: &str,
    entity_field_name: Option<&str>,
    auto_camel_case: bool,
    success_type_fields: Option<&Vec<String>>,
    cascade_selections: Option<&str>,  // NEW parameter
) -> Result<Value, String> {
```

---

### Step 4: Rust - Implement CASCADE Filtering Logic

**File**: `fraiseql_rs/src/mutation/response_builder.rs`

**Location**: Around line 159-163 where CASCADE is added

```rust
// FIND:
// Add cascade if present (add __typename for GraphQL)
if let Some(cascade) = &result.cascade {
    let cascade_with_typename = transform_cascade(cascade, auto_camel_case);
    obj.insert("cascade".to_string(), cascade_with_typename);
}

// REPLACE WITH:
// Add cascade if present AND requested in selection
if let Some(cascade) = &result.cascade {
    if let Some(selections_json) = cascade_selections {
        // Parse selections from JSON
        let selections: CascadeSelections = serde_json::from_str(selections_json)
            .map_err(|e| format!("Failed to parse cascade selections: {}", e))?;

        // Filter cascade based on selections
        let filtered_cascade = filter_cascade_by_selections(
            cascade,
            &selections,
            auto_camel_case
        )?;

        obj.insert("cascade".to_string(), filtered_cascade);
    }
}
```

---

### Step 5: Rust - Create CASCADE Filter Module

**File**: `fraiseql_rs/src/mutation/cascade_filter.rs` (NEW FILE)

```rust
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

#[derive(Debug, Deserialize)]
pub struct CascadeSelections {
    pub fields: Vec<String>,
    #[serde(default)]
    pub updated: Option<FieldSelections>,
    #[serde(default)]
    pub deleted: Option<FieldSelections>,
    #[serde(default)]
    pub invalidations: Option<FieldSelections>,
    #[serde(default)]
    pub metadata: Option<FieldSelections>,
}

#[derive(Debug, Deserialize)]
pub struct FieldSelections {
    pub fields: Vec<String>,
}

pub fn filter_cascade_by_selections(
    cascade: &Value,
    selections: &CascadeSelections,
    auto_camel_case: bool,
) -> Result<Value, String> {
    let mut filtered = Map::new();

    if let Value::Object(cascade_obj) = cascade {
        for field_name in &selections.fields {
            let key = if auto_camel_case {
                to_camel_case(field_name)
            } else {
                field_name.clone()
            };

            if let Some(value) = cascade_obj.get(&key) {
                match field_name.as_str() {
                    "updated" | "deleted" | "invalidations" | "metadata" => {
                        filtered.insert(key, value.clone());
                    }
                    _ => {
                        filtered.insert(key, value.clone());
                    }
                }
            }
        }
    }

    Ok(Value::Object(filtered))
}

fn to_camel_case(s: &str) -> String {
    s.to_string()
}
```

**Add to module exports**:

**File**: `fraiseql_rs/src/mutation/mod.rs`

```rust
// At top of file, add:
mod cascade_filter;

// Add import:
use cascade_filter::{filter_cascade_by_selections, CascadeSelections};
```

---

### Step 6: Handle Empty Selections (No CASCADE Requested)

**File**: `fraiseql_rs/src/mutation/response_builder.rs`

Ensure the code handles the case where `cascade_selections` is `None`:

```rust
// Add cascade if present AND requested in selection
if let Some(cascade) = &result.cascade {
    // Only include CASCADE if selections were provided
    // If cascade_selections is None, it means CASCADE was not requested at all
    if let Some(selections_json) = cascade_selections {
        // ... filtering logic ...
    }
    // If cascade_selections is None, don't add cascade field
}
```

This ensures:

- No `cascade` in GraphQL query → `cascade_selections = None` → CASCADE not added to response
- `cascade { ... }` in query → `cascade_selections = Some(json)` → Filtered CASCADE added

---

## Verification Commands

```bash
# Run tests - should now PASS
uv run pytest tests/integration/test_cascade_selection_filtering.py -xvs

# Expected results:
# ✅ test_cascade_not_returned_when_not_requested: PASS
# ✅ test_cascade_returned_when_requested: PASS
# ✅ test_partial_cascade_selection_updated_only: PASS
# ✅ test_partial_cascade_selection_metadata_only: PASS
# ✅ test_multiple_mutations_with_different_cascade_selections: PASS

# Run existing CASCADE tests to ensure no regression
uv run pytest tests/integration/test_graphql_cascade.py -xvs

# Build Rust to check for compilation errors
cd fraiseql-rs && cargo build
```

---

## Acceptance Criteria

- ✅ All Phase 1 tests pass
- ✅ No regressions in existing CASCADE tests
- ✅ Rust code compiles without errors
- ✅ Code is minimal and focused on making tests pass
- ✅ No over-engineering or extra features

---

## DO NOT

- ❌ Add extra features not required by tests
- ❌ Refactor existing code (that's REFACTOR phase)
- ❌ Add extensive error handling beyond basic cases
- ❌ Optimize performance (that's later)
- ❌ Add comments explaining the fix

---

## Potential Issues & Solutions

### Issue 1: Cascade Selections JSON Format Mismatch

**Symptom**: Rust deserialization fails

**Solution**: Check `cascade_selections.py` output format matches Rust struct

```bash
# Debug: Print cascade selections JSON
# In executor.py, temporarily add:
print(f"CASCADE_SELECTIONS: {cascade_selections_json}")
```

### Issue 2: Existing Tests Fail

**Symptom**: Tests expecting CASCADE always present now fail

**Solution**: Update test expectations (will be done in QA phase)

### Issue 3: camelCase Handling

**Symptom**: Field names don't match (updated vs Updated)

**Solution**: Use `auto_camel_case` parameter consistently in filter logic

---

## Next Phase

After this phase completes:
→ **Phase 3: REFACTOR** - Clean up code, improve structure, add error handling
