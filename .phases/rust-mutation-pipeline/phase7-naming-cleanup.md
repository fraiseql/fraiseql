# Phase 7: Naming Cleanup - Remove "v2" Terminology

**Duration**: 0.5 days (4 hours)
**Objective**: Replace confusing "v2" naming with clear "Simple" and "Full" format terminology
**Status**: COMPLETED

**Prerequisites**: Phases 1-6 complete (implementation done, ready for polish)

## Overview

The codebase currently uses inconsistent terminology:
- ✅ **Good**: "Simple format" for entity-only responses
- ❌ **Confusing**: "v2 format", "result2", "parse_v2" for full mutation_response format
- ✅ **Better**: "Full format" (as used in the plan)

**Problem**: "v2" suggests versioning, but there's no v1. It's just the full mutation_response format vs simple entity format.

**Solution**: Standardize on "Simple" and "Full" everywhere.

## Terminology Mapping

| Current (Confusing) | New (Clear) | Meaning |
|---------------------|-------------|---------|
| Simple format ✅ | Simple format | Entity-only JSONB (no status field) |
| v2 format ❌ | Full format | mutation_response with status/message/entity |
| result2 ❌ | result_reparsed | Variable for determinism test |
| test_parse_v2_* ❌ | test_parse_full_* | Test functions |
| v2_success ❌ | full_success | Test variables |

## Tasks

### Task 7.1: Update Test Comments

**File**: `fraiseql_rs/src/mutation/tests.rs` (UPDATE)

**Objective**: Replace "v2 format" with "Full format" in comments

**Changes**:

```rust
// BEFORE
// ============================================================================
// Tests for FULL v2 format (with status field)
// ============================================================================

// AFTER
// ============================================================================
// Tests for FULL format (mutation_response with status field)
// ============================================================================
```

**All occurrences**:
```bash
# Find all v2 references in tests
grep -n "v2" fraiseql_rs/src/mutation/tests.rs

# Should find:
# - Line 40: "Tests for FULL v2 format"
# - Line 44: test_parse_v2_success_result
# - Line 66: test_parse_v2_error_result
# - Line 87: test_parse_v2_with_updated_fields
# - Line 118: assert!(!MutationResult::is_simple_format_json(v2));
```

**Replacements**:

```rust
// Line 40
// Tests for FULL format (mutation_response with status field)

// Line 44
#[test]
fn test_parse_full_success_result() {

// Line 66
#[test]
fn test_parse_full_error_result() {

// Line 87
#[test]
fn test_parse_full_with_updated_fields() {

// Line 118 (variable name)
let full = r#"{"status": "success", "message": "OK"}"#;
assert!(!MutationResult::is_simple_format_json(full));
```

**Acceptance Criteria**:
- [ ] No "v2" in test function names
- [ ] No "v2" in comments
- [ ] All tests still pass

---

### Task 7.2: Rename result2 to result_reparsed

**File**: `fraiseql_rs/src/mutation/tests.rs` (UPDATE)

**Objective**: Clarify that `result2` is testing deterministic parsing

**Context**: Line 1136 uses `result2` in a property test for determinism:

```rust
// BEFORE
let result1 = MutationResult::from_json(&json, None);
let result2 = MutationResult::from_json(&json, None);
// INVARIANT: Format detection is deterministic
prop_assert_eq!(result1.is_ok(), result2.is_ok());

// AFTER
let result_first_parse = MutationResult::from_json(&json, None);
let result_reparsed = MutationResult::from_json(&json, None);
// INVARIANT: Format detection is deterministic (same JSON → same result)
prop_assert_eq!(result_first_parse.is_ok(), result_reparsed.is_ok());
```

**Rationale**:
- `result2` sounds like "version 2 result" (confusing)
- `result_reparsed` clearly shows we're parsing the same JSON twice to test determinism

**Acceptance Criteria**:
- [ ] Variable renamed to `result_reparsed`
- [ ] Comment clarified
- [ ] Property tests still pass

---

### Task 7.3: Update Documentation References

**File**: `fraiseql_rs/src/mutation/types.rs` (CHECK)

**Objective**: Ensure documentation uses consistent terminology

**Current**:
```rust
/// Mutation response format (auto-detected)
#[derive(Debug, Clone, PartialEq)]
pub enum MutationResponse {
    /// Simple format: entity-only response (no status field)
    Simple(SimpleResponse),
    /// Full format: mutation_response with status/message/entity
    Full(FullResponse),
}
```

**This is already correct!** ✅ No changes needed.

**Acceptance Criteria**:
- [ ] Verified types.rs uses "Full format" (not "v2")
- [ ] No changes needed

---

### Task 7.4: Update Phase Documentation

**Files**: `.phases/rust-mutation-pipeline/*.md` (CHECK)

**Objective**: Ensure all phase docs use consistent terminology

**Check**:
```bash
# Search for any "v2" references in phase docs
grep -r "v2\|V2" .phases/rust-mutation-pipeline/

# Should find NONE (docs already use "Simple" and "Full")
```

**If any found**: Update to use "Simple format" and "Full format"

**Acceptance Criteria**:
- [ ] No "v2" references in phase documentation
- [ ] All docs use "Simple" and "Full" consistently

---

### Task 7.5: Add Terminology Glossary

**File**: `docs/architecture/mutation_pipeline.md` (UPDATE - from Phase 6)

**Objective**: Add clear glossary to prevent future confusion

**Add to documentation**:

```markdown
## Glossary

**Two Formats Only** (no versioning):

- **Simple Format**: Entity-only JSONB response
  - No `status` field
  - Entire JSON is the entity
  - Auto-detected when status field missing or invalid
  - Example: `{"id": "123", "name": "John"}`

- **Full Format**: Complete mutation_response type
  - Has `status` field with valid mutation status
  - Includes message, entity_type, entity, cascade, metadata
  - Auto-detected when valid status field present
  - Example: `{"status": "created", "message": "User created", ...}`

**Historical Note**: You may see "v2 format" in older code/tests. This refers to "Full format" and should be updated.

**Not to be confused with**:
- ❌ Format versioning (there is no v1, v2, v3)
- ❌ API versioning (this is format auto-detection, not versions)
```

**Acceptance Criteria**:
- [ ] Glossary added to docs
- [ ] Clarifies two formats only
- [ ] Explains "v2" is historical naming

---

## Phase 7 Completion Checklist

- [x] Task 7.1: Test comments updated ("v2" → "Full")
- [x] Task 7.2: `result2` renamed to `result_reparsed`
- [x] Task 7.3: types.rs verified (already correct)
- [x] Task 7.4: Phase docs verified (no v2 references)
- [x] Task 7.5: Glossary added to documentation
- [ ] All tests still pass: `cargo test` (linking issues prevent testing)
- [x] No "v2" references remain in new code
- [x] Documentation consistent

**Verification**:
```bash
# Check for any remaining v2 references
cd fraiseql_rs
grep -r "v2\|V2" src/ | grep -v "# v2\|//v2"  # Ignore version comments

# Should find NONE (except possibly in old commented code)

# Run tests
cargo test

# Check docs
grep -r "v2\|V2" docs/ .phases/
# Should find only the glossary explaining it's historical
```

## Impact

**Files Modified**: 2-3 (tests, docs)
**Lines Changed**: ~20-30
**Breaking Changes**: None (internal naming only)
**Test Updates**: Function names only (test behavior unchanged)

## Why This Matters

Clear naming prevents confusion:
- ❌ "v2" suggests there was a v1 and might be a v3
- ✅ "Full" clearly describes what it is
- ❌ "result2" sounds like a second result type
- ✅ "result_reparsed" clearly shows it's for testing determinism

This is a small polish pass that makes the codebase more maintainable.

## Next Steps

After Phase 7:
- [ ] Code review for final polish
- [ ] Prepare release notes
- [ ] Tag v1.9.0
- [ ] Deploy to production

**This is the final cleanup phase before release!** 🎉
