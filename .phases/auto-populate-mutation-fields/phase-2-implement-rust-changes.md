# Phase 2: Implement Rust Changes - Auto-Populate Status and Errors

## Objective

Modify Rust response builder to auto-populate `status` and `errors` fields in success responses, matching the completeness of error responses.

## TDD Stage

GREEN (Implementing functionality to make behavior work)

## Context

**From Phase 1 Research**:
- Error responses already auto-populate: `code`, `status`, `message`, `errors` ✅
- Success responses only populate: `__typename`, `id`, `message` (partial) ❌
- Rust has access to `result.status` and can generate empty errors array
- Python decorators already inject fields into GraphQL schema

**This Phase**:
- Add `status` field to success responses
- Add `errors` field to success responses (always empty array for success)
- Follow existing error response pattern for consistency

**Next Phase**:
- Phase 3: Test the changes with integration tests
- Phase 4: Document behavior and create migration guide

## Files to Modify

1. `fraiseql_rs/src/mutation/response_builder.rs`
   - Function: `build_success_response()` (lines 87-248)
   - Changes: Add status and errors field insertion
   - Location: After line 106 (message insertion)

## Implementation Steps

### Step 1: Locate the exact insertion point

**File**: `fraiseql_rs/src/mutation/response_builder.rs`

**Current code** (lines 98-118):
```rust
let mut obj = Map::new();

// Add __typename
obj.insert("__typename".to_string(), json!(success_type));

// Add id from entity_id if present
if let Some(ref entity_id) = result.entity_id {
    obj.insert("id".to_string(), json!(entity_id));
}

// Add message
obj.insert("message".to_string(), json!(result.message));

// v1.8.0: SUCCESS MUST HAVE ENTITY (non-null guarantee)
if result.entity.is_none() {
    return Err(format!(
        "Success type '{}' requires non-null entity. ...",
        success_type, result.status.to_string()
    ));
}
```

**Insertion point**: After line 106 (`obj.insert("message"...)`)

### Step 2: Add status field insertion

**Add after line 106**:
```rust
// Add status (always "success" for success responses)
obj.insert("status".to_string(), json!(result.status.to_string()));
```

**Rationale**:
- `result.status` contains the mutation status from database
- For success responses, this will be `"success"` or `"success:message"`
- Convert to string with `to_string()` method (same as error responses, line 274)
- Matches GraphQL schema field type: `status: String!`

### Step 3: Add errors field insertion

**Add after status field**:
```rust
// Add errors (always empty array for success responses)
obj.insert("errors".to_string(), json!([]));
```

**Rationale**:
- Success responses have no errors, so always return empty array `[]`
- Matches GraphQL schema field type: `errors: [Error!]` (nullable, but if present, array of non-null errors)
- Consistent with decorator default: `cls.errors = []` (line 149 in `decorators.py`)
- Frontend-friendly: always returns array, never null (easier to iterate)

### Step 4: Verify complete field list

**After changes, success response will include** (in order):
1. `__typename` - GraphQL type name (e.g., "CreateUserSuccess")
2. `id` - Entity ID from database (if present)
3. `message` - Success message from database
4. `status` - Status string (always "success" or "success:*") ⭐ NEW
5. `errors` - Empty errors array ⭐ NEW
6. `{entityFieldName}` - Entity object with data
7. `updatedFields` - Array of field names that changed (if present)
8. `cascade` - Cascade data (if present and selected)

**Matches error response structure**: ✅ Consistent API

### Step 5: Update code with exact line numbers

**File**: `fraiseql_rs/src/mutation/response_builder.rs`

**Find this code block** (lines ~95-108):
```rust
let mut obj = Map::new();

// Add __typename
obj.insert("__typename".to_string(), json!(success_type));

// Add id from entity_id if present
if let Some(ref entity_id) = result.entity_id {
    obj.insert("id".to_string(), json!(entity_id));
}

// Add message
obj.insert("message".to_string(), json!(result.message));
```

**Replace with**:
```rust
let mut obj = Map::new();

// Add __typename
obj.insert("__typename".to_string(), json!(success_type));

// Add id from entity_id if present
if let Some(ref entity_id) = result.entity_id {
    obj.insert("id".to_string(), json!(entity_id));
}

// Add message
obj.insert("message".to_string(), json!(result.message));

// Add status (always "success" for success responses)
obj.insert("status".to_string(), json!(result.status.to_string()));

// Add errors (always empty array for success responses)
obj.insert("errors".to_string(), json!([]));
```

**Changes summary**:
- Added 4 lines (2 comments + 2 insertions)
- No other code touched
- Minimal, surgical change

### Step 6: Compile Rust extension

**Commands**:
```bash
# Navigate to Rust directory
cd fraiseql_rs

# Build in release mode for production
cargo build --release

# Or build in debug mode for development (faster compile)
cargo build

# Return to project root
cd ..

# Install updated extension
uv pip install -e .
```

**Expected output**:
```
   Compiling fraiseql_rs v1.8.0
    Finished release [optimized] target(s) in 30.2s
```

**If compilation fails**:
- Check for syntax errors (missing semicolons, brackets)
- Verify `json!()` macro is available (imported at top of file)
- Check that `result.status.to_string()` method exists (it does, used in line 274)

### Step 7: Verify compilation success

**Commands**:
```bash
# Check Rust extension is importable
python3 -c "import fraiseql._fraiseql_rs; print('✅ Rust extension loaded')"

# Check module exports
python3 -c "from fraiseql._fraiseql_rs import build_mutation_response; print('✅ Function available')"
```

**Expected output**:
```
✅ Rust extension loaded
✅ Function available
```

## Code Examples

### Complete Modified Function

**File**: `fraiseql_rs/src/mutation/response_builder.rs`

**Function**: `build_success_response()` (lines 87-248, modified)

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

    // Add __typename
    obj.insert("__typename".to_string(), json!(success_type));

    // Add id from entity_id if present
    if let Some(ref entity_id) = result.entity_id {
        obj.insert("id".to_string(), json!(entity_id));
    }

    // Add message
    obj.insert("message".to_string(), json!(result.message));

    // ⭐ NEW: Add status (always "success" for success responses)
    obj.insert("status".to_string(), json!(result.status.to_string()));

    // ⭐ NEW: Add errors (always empty array for success responses)
    obj.insert("errors".to_string(), json!([]));

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

    // ... rest of function unchanged (entity, updatedFields, cascade handling) ...

    Ok(Value::Object(obj))
}
```

**Lines changed**: Only lines 106-109 (4 new lines after message insertion)

### Comparison: Before and After

**BEFORE** (v1.8.0):
```json
{
  "data": {
    "createUser": {
      "__typename": "CreateUserSuccess",
      "id": "123e4567-e89b-12d3-a456-426614174000",
      "message": "User created successfully",
      "user": { "id": "123...", "email": "test@example.com" },
      "updatedFields": ["email", "name"]
    }
  }
}
```

**AFTER** (v1.9.0):
```json
{
  "data": {
    "createUser": {
      "__typename": "CreateUserSuccess",
      "id": "123e4567-e89b-12d3-a456-426614174000",
      "message": "User created successfully",
      "status": "success",
      "errors": [],
      "user": { "id": "123...", "email": "test@example.com" },
      "updatedFields": ["email", "name"]
    }
  }
}
```

**Changes**:
- ✅ Added `status: "success"`
- ✅ Added `errors: []`
- ✅ All other fields unchanged
- ✅ Backward compatible (clients ignoring new fields still work)

## Verification Commands

```bash
# Compile Rust extension
cd fraiseql_rs && cargo build --release && cd ..

# Install updated extension
uv pip install -e .

# Verify extension loads
python3 -c "import fraiseql._fraiseql_rs; print('✅ Loaded')"

# Check function signature unchanged
python3 -c "from fraiseql._fraiseql_rs import build_mutation_response; print('✅ Available')"
```

## Expected Outcome

### Compilation Should:
- ✅ Complete without errors
- ✅ Take 20-60 seconds (depending on machine)
- ✅ Generate updated `.so` file (Linux) or `.dylib` (macOS)

### Code Should:
- ✅ Add exactly 4 lines (2 comments + 2 field insertions)
- ✅ Not modify any other logic
- ✅ Follow existing code style (same pattern as error responses)
- ✅ Compile without warnings

### Extension Should:
- ✅ Import successfully in Python
- ✅ Export `build_mutation_response` function
- ✅ Not break any existing functionality

## Acceptance Criteria

- [ ] Added `status` field insertion after line 106 in `response_builder.rs`
- [ ] Added `errors` field insertion after `status` insertion
- [ ] `status` uses `result.status.to_string()` (matches line 274 pattern)
- [ ] `errors` uses `json!([])` (empty array for success)
- [ ] Rust code compiles without errors
- [ ] Rust code compiles without warnings
- [ ] Python can import `fraiseql._fraiseql_rs` module
- [ ] No other functions modified
- [ ] Code style matches surrounding code (indentation, comments)

## DO NOT

- **DO NOT modify error response logic** - it already works correctly
- **DO NOT add conditional logic** - always include status and errors for success
- **DO NOT change field order** - add after message, before entity logic
- **DO NOT add tests yet** - Phase 3 will handle testing
- **DO NOT update documentation yet** - Phase 4 will handle docs
- **DO NOT commit yet** - wait for Phase 3 verification

## Notes

### Why Empty Array Instead of Null?

**Option 1: null** (nullable in GraphQL schema)
```rust
obj.insert("errors".to_string(), json!(null));
```
**Cons**: Client code must check `if (errors !== null)` before iterating

**Option 2: []** (empty array) ✅ RECOMMENDED
```rust
obj.insert("errors".to_string(), json!([]));
```
**Pros**: Client code can always iterate: `errors.forEach(...)` works without check

**Decision**: Use empty array for better DX (developer experience)

### Why Not Make Status Dynamic?

The database can return:
- `"success"` - Simple success
- `"success:created"` - Success with detail
- `"success:updated"` - Success with detail

**Current implementation**: Use actual database status value
```rust
obj.insert("status".to_string(), json!(result.status.to_string()));
```

This preserves semantic information from database while still indicating success.

**Alternative** (not recommended): Hardcode `"success"`
```rust
obj.insert("status".to_string(), json!("success"));
```
This loses information and doesn't match error response pattern.

### Rust Compilation Tips

**Fast iteration during development**:
```bash
# Use debug build (faster compile, slower runtime)
cargo build

# Use release build (slower compile, faster runtime)
cargo build --release
```

**If you see linker errors**:
- Make sure `pyproject.toml` has correct Rust build config
- Try: `uv pip install --force-reinstall -e .`

**If imports fail**:
- Check `.so` file exists: `ls fraiseql_rs/target/release/*.so`
- Verify Python finds it: `python3 -c "import sys; print(sys.path)"`

### Next Steps Preview

**Phase 3** will:
1. Run existing integration tests (should all pass)
2. Write specific tests for new fields
3. Test with real mutations from PrintOptim backend
4. Verify error responses still work (unchanged)

**Phase 4** will:
1. Update CHANGELOG with feature description
2. Write migration guide for v1.8.0 → v1.9.0 users
3. Update mutation tutorial with simplified examples
4. Add API reference documentation
