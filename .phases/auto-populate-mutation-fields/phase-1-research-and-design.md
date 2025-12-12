# Phase 1: Research and Design - Auto-Populate Mutation Fields

## Objective

Research and document the exact implementation approach for auto-populating `status`, `message`, and `errors` fields in mutation response types from database `mutation_result` data.

## TDD Stage

N/A (Research and Design Phase)

## Context

**Current State (v1.8.0)**:
- `@fraiseql.success` and `@fraiseql.failure` decorators inject `status`, `message`, `errors` fields into GraphQL schema
- Fields appear in schema but are NOT auto-populated from database responses
- Developers must manually assign these fields in every resolver

**Desired State**:
- Decorators inject fields AND framework auto-populates them from database
- Resolvers only need to provide entity-specific fields
- Backward compatible: explicit values override auto-population

**Key Files**:
- `src/fraiseql/mutations/decorators.py` - Python decorators (lines 86-162)
- `src/fraiseql/mutations/rust_executor.py` - Rust executor bridge (lines 29-250)
- `fraiseql_rs/src/mutation/response_builder.rs` - Rust response builder (lines 79-399)

## Files to Read and Analyze

### Python Layer
1. `src/fraiseql/mutations/decorators.py`
   - Lines 86-118: `success()` decorator - already injects fields with default values
   - Lines 128-162: `failure()` decorator - already injects fields with default values
   - **Key Finding**: Decorators set class-level defaults (`cls.status = "success"`, etc.)

2. `src/fraiseql/mutations/rust_executor.py`
   - Lines 29-250: `execute_mutation_rust()` function
   - Lines 134-197: Result processing (dict/tuple/string handling)
   - **Key Finding**: Database result already parsed and sent to Rust as JSON

3. `src/fraiseql/mutations/result_processor.py`
   - Check if there's post-processing logic after Rust returns response
   - Look for instance construction of Success/Error types

### Rust Layer
4. `fraiseql_rs/src/mutation/response_builder.rs`
   - Lines 79-248: `build_success_response()` - builds success response object
   - Lines 250-287: `build_error_response_with_code()` - builds error response object
   - Lines 289-311: `generate_errors_array()` - auto-generates errors array
   - **Key Finding**: Rust already auto-populates `message` (line 106), but NOT `status` or `errors` in success responses

5. `fraiseql_rs/src/mutation/mod.rs`
   - Check `MutationResult` struct definition
   - Verify fields available: `status`, `message`, `entity_id`, `updated_fields`, `metadata`

## Research Questions to Answer

### Question 1: Where are Success/Error instances constructed?
**Investigation**:
- Trace back from `execute_mutation_rust()` return value
- Find where Python classes (decorated with `@success`/`@failure`) are instantiated
- Determine if instances are built from Rust JSON response or manually in resolvers

**Expected Finding**:
- Rust returns JSON response with `__typename`, `message`, `entity`, etc.
- JSON is likely deserialized into Python class instances somewhere
- Need to find that deserialization point to inject auto-population logic

### Question 2: Does Rust already have access to status/message fields?
**Investigation**:
- Check `MutationResult` struct in `fraiseql_rs/src/mutation/mod.rs`
- Verify `result.status` and `result.message` are available in response builder
- Lines 106, 274, 277 in `response_builder.rs` show they ARE available

**Expected Finding**:
- ✅ Rust has `result.status` (line 274: `obj.insert("status", result.status.to_string())`)
- ✅ Rust has `result.message` (line 106: `obj.insert("message", result.message)`)
- ✅ Rust can generate `errors` array (line 280: `generate_errors_array()`)

**Conclusion**: Rust ALREADY has the data, just needs to add it to response object

### Question 3: Why isn't status auto-populated in success responses?
**Investigation**:
- Review `build_success_response()` (lines 87-248)
- Compare with `build_error_response_with_code()` (lines 258-287)
- Error responses add: `code`, `status`, `message`, `errors`
- Success responses add: `__typename`, `id`, `message`, `entity`, `updatedFields`, `cascade`

**Finding**:
- Success responses add `message` (line 106) but NOT `status` or `errors`
- Error responses add ALL standard fields (lines 271-281)
- **Inconsistency**: Error type is complete, Success type is incomplete

### Question 4: What happens in error responses?
**Investigation**:
- Review `build_error_response_with_code()` implementation
- Check lines 264-286:
  ```rust
  obj.insert("__typename", json!(error_type));           // Line 267
  obj.insert("code", json!(code));                      // Line 271
  obj.insert("status", json!(result.status.to_string())); // Line 274
  obj.insert("message", json!(result.message));          // Line 277
  obj.insert("errors", errors);                          // Line 281
  ```

**Finding**:
- Error responses ALREADY auto-populate ALL standard fields
- This is the pattern to replicate for success responses

### Question 5: Are decorator defaults ever used?
**Investigation**:
- Check `decorators.py` lines 98-105 (success decorator)
- Sets `cls.status = "success"`, `cls.message = None`, `cls.errors = None`
- These are class-level defaults, not instance values

**Expected Finding**:
- Defaults are likely used as GraphQL schema defaults
- Not used for actual response construction (Rust builds JSON directly)
- Need to ensure Rust-built JSON includes these fields so GraphQL serialization works

## Implementation Strategy Design

### Option A: Rust-Only Solution (Recommended)

**Approach**: Extend Rust response builder to auto-populate standard fields

**Changes Required**:
1. Modify `build_success_response()` in `response_builder.rs`:
   - Add `status` field (line ~107, after `message`)
   - Add `errors` field (line ~108, after `status`)
   - Generate empty `errors` array for success (`[]`)

**Pros**:
- ✅ Consistent with existing error response pattern
- ✅ Zero Python changes (pure Rust optimization)
- ✅ Fastest performance (no Python overhead)
- ✅ All auto-population logic in one place

**Cons**:
- ❌ Requires Rust recompilation
- ❌ Users must reinstall fraiseql to get changes

**Code Location**: `fraiseql_rs/src/mutation/response_builder.rs` lines 87-248

### Option B: Python Post-Processing (Alternative)

**Approach**: Intercept Rust response JSON and inject fields in Python

**Changes Required**:
1. Create `src/fraiseql/mutations/field_injector.py`
2. Hook into `rust_executor.py` after Rust returns response
3. Parse JSON, add missing `status`/`errors` if not present
4. Re-serialize JSON

**Pros**:
- ✅ No Rust recompilation needed
- ✅ Easier to test in Python

**Cons**:
- ❌ Performance overhead (JSON parse + serialize)
- ❌ Duplicates logic (Rust already does this for errors)
- ❌ More complex (two layers doing similar work)

**Code Location**: New file + `rust_executor.py` lines 200-250

### Option C: Hybrid (Python Decorator + Rust)

**Approach**: Decorator marks fields for auto-population, Rust honors marking

**Changes Required**:
1. Decorators add `__fraiseql_auto_populate__` metadata
2. Pass field list to Rust via `build_mutation_response()`
3. Rust checks metadata and auto-populates marked fields

**Pros**:
- ✅ Most flexible (Python controls what gets auto-populated)
- ✅ Future-proof (can extend to custom fields)

**Cons**:
- ❌ Most complex (requires both Python and Rust changes)
- ❌ Over-engineered for current need
- ❌ Harder to maintain

## Recommended Approach: Option A (Rust-Only)

**Rationale**:
1. **Consistency**: Error responses already auto-populate all fields in Rust
2. **Performance**: No Python overhead, pure Rust speed
3. **Simplicity**: Single location for all auto-population logic
4. **Existing Pattern**: Rust already adds `message`, just extend to `status` and `errors`

**Backward Compatibility**:
- Explicit resolver values would need Python-side override mechanism
- BUT: Current code DOESN'T support explicit override either (Rust builds JSON directly)
- Solution: This is Phase 2 concern (make it work first, add override later if needed)

## Files to Modify (Phase 2)

### Rust Changes
1. `fraiseql_rs/src/mutation/response_builder.rs`
   - Modify `build_success_response()` function (lines 87-248)
   - Add status field insertion (after line 106)
   - Add errors field insertion (after status)
   - Generate empty errors array `[]` for success responses

### Python Changes (if needed)
None required for basic functionality. Decorator defaults are sufficient.

### Documentation Changes (Phase 4)
1. Update mutation tutorial examples
2. Add migration guide for v1.8.0 → v1.9.0
3. Update API reference

## Verification Commands

```bash
# Read key files
cat src/fraiseql/mutations/decorators.py
cat src/fraiseql/mutations/rust_executor.py
cat fraiseql_rs/src/mutation/response_builder.rs

# Check MutationResult struct definition
cat fraiseql_rs/src/mutation/mod.rs | grep -A 20 "struct MutationResult"

# Find where Success/Error instances are constructed
grep -r "Success(" src/fraiseql/mutations/ --include="*.py"
grep -r "Error(" src/fraiseql/mutations/ --include="*.py"
```

## Expected Outcome

### Research Should Reveal:
- ✅ Exact location where status/message/errors need to be added
- ✅ Whether Rust or Python layer is the right place for changes
- ✅ Any blockers or edge cases to handle
- ✅ Backward compatibility concerns

### Design Document Should Include:
- Detailed implementation plan for Phase 2
- Code snippets showing exact changes
- Test cases for Phase 3
- Migration notes for Phase 4

## Acceptance Criteria

- [ ] Confirmed Rust has access to `result.status`, `result.message`, `result.metadata`
- [ ] Identified exact function/lines to modify in `response_builder.rs`
- [ ] Documented why success responses don't include status/errors currently
- [ ] Verified error responses already auto-populate all fields (reference implementation)
- [ ] Determined if Python changes are needed (likely not)
- [ ] Designed test strategy for Phase 3

## DO NOT

- **DO NOT write any code changes yet** - this is research only
- **DO NOT modify any files** - only read and analyze
- **DO NOT run tests** - just document what needs testing later
- **DO NOT assume implementation details** - verify everything in code

## Notes

### Key Code References

**Success Response Building** (`response_builder.rs:87-248`):
```rust
pub fn build_success_response(...) -> Result<Value, String> {
    let mut obj = Map::new();
    obj.insert("__typename".to_string(), json!(success_type));

    if let Some(ref entity_id) = result.entity_id {
        obj.insert("id".to_string(), json!(entity_id));
    }

    obj.insert("message".to_string(), json!(result.message));  // Line 106
    // ⭐ MISSING: status field
    // ⭐ MISSING: errors field

    // ... entity, updatedFields, cascade handling ...

    Ok(Value::Object(obj))
}
```

**Error Response Building** (`response_builder.rs:250-287`):
```rust
pub fn build_error_response_with_code(...) -> Result<Value, String> {
    let mut obj = Map::new();
    obj.insert("__typename".to_string(), json!(error_type));
    obj.insert("code".to_string(), json!(code));
    obj.insert("status".to_string(), json!(result.status.to_string())); // ✅ Has status
    obj.insert("message".to_string(), json!(result.message));           // ✅ Has message

    let errors = generate_errors_array(result, code)?;
    obj.insert("errors".to_string(), errors);                            // ✅ Has errors

    Ok(Value::Object(obj))
}
```

**Pattern to Follow**: Success responses should match error response completeness.

### Critical Insight

The decorators (Python) inject fields into the **GraphQL schema** (lines 98-105, 139-149).
The response builder (Rust) builds the **actual response JSON** (lines 87-287).

These are TWO SEPARATE concerns:
1. **Schema**: What fields are AVAILABLE (Python decorators handle this) ✅ Already done
2. **Data**: What values fields HAVE (Rust response builder handles this) ❌ Incomplete for success

**Fix**: Make Rust response builder populate the fields that Python decorators declare.

### Next Phase Preview

Phase 2 will:
1. Add 2-3 lines to `build_success_response()` in Rust
2. Insert `status` and `errors` fields into response object
3. Keep it simple: `status = "success"` for all success responses
4. Keep it simple: `errors = []` for all success responses
5. Recompile Rust extension
6. Test with existing integration tests

Estimated complexity: **LOW** (following existing pattern)
Estimated time: **30 minutes** (code + compile + basic test)
