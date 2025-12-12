# Input CamelCase → snake_case Conversion

**Feature**: Fix incomplete camelCase conversion for mutation inputs sent to PostgreSQL
**Issue**: `/tmp/fraiseql_camelcase_issue.md`
**Verification**: `/tmp/fraiseql_input_conversion_verification.md`
**Status**: Ready for implementation
**Estimated Complexity**: Low (3 phases, well-defined)

---

## Problem Summary

FraiseQL's `auto_camel_case=True` configuration converts snake_case → camelCase for **outputs** but does NOT convert camelCase → snake_case for **inputs** sent to PostgreSQL functions.

This causes `jsonb_populate_record()` to fail silently when populating PostgreSQL composite types, as the JSON keys (camelCase) don't match the composite type field names (snake_case).

**Example**:
```python
# GraphQL sends:
{ "contractId": "...", "startDate": "2025-01-01" }

# PostgreSQL receives (currently):
{ "contractId": "...", "startDate": "2025-01-01" }  # ❌ camelCase keys

# PostgreSQL composite type expects:
{ "contract_id": "...", "start_date": "2025-01-01" }  # ✅ snake_case keys

# Result: All fields except 'amount' and 'currency' are NULL
```

---

## Solution Architecture

### Current Flow (Broken)
```
GraphQL (camelCase)
  ↓
Python coercion (snake_case attributes) ✅
  ↓
_to_dict() (snake_case keys) ✅
  ↓
json.dumps() ❌ NO CONVERSION
  ↓
PostgreSQL (expects snake_case) ❌ MISMATCH
```

### Fixed Flow
```
GraphQL (camelCase)
  ↓
Python coercion (snake_case attributes) ✅
  ↓
_to_dict() (snake_case keys) ✅
  ↓
dict_keys_to_snake_case() ✅ NEW
  ↓
json.dumps() ✅
  ↓
PostgreSQL (snake_case) ✅ MATCH
```

---

## Implementation Phases

### Phase 1: Add Utility Function (TDD RED)
**File**: `phase-1-add-utility-function.md`

**Objective**: Write comprehensive tests for `dict_keys_to_snake_case()` utility

**Tasks**:
1. Create test file with 15+ test cases
2. Add function signature with `NotImplementedError`
3. Verify all tests FAIL

**Duration**: ~30 minutes
**Complexity**: Low

---

### Phase 2: Implement Utility (TDD GREEN)
**File**: `phase-2-implement-utility.md`

**Objective**: Implement `dict_keys_to_snake_case()` to make all tests pass

**Tasks**:
1. Implement recursive conversion logic (5 lines)
2. Run tests and verify all pass
3. Check for regressions

**Duration**: ~20 minutes
**Complexity**: Low

---

### Phase 3: Integration (TDD REFACTOR + QA)
**File**: `phase-3-integrate-into-mutations.md`

**Objective**: Integrate utility into `rust_executor.py` and add integration tests

**Tasks**:
1. Write integration test with PostgreSQL composite types
2. Add conversion call in `rust_executor.py`
3. Run full test suite
4. Verify no regressions

**Duration**: ~45 minutes
**Complexity**: Medium

---

## Files Modified

### Created
- `tests/unit/utils/test_dict_keys_to_snake_case.py` (new unit tests)
- `tests/integration/graphql/mutations/test_input_camelcase_to_snake_case.py` (new integration test)

### Modified
- `src/fraiseql/utils/casing.py` (add `dict_keys_to_snake_case()` function)
- `src/fraiseql/mutations/rust_executor.py` (integrate conversion before `json.dumps()`)

---

## Testing Strategy

### Unit Tests (Phase 1-2)
- ✅ Simple dict conversion
- ✅ Nested dicts (multiple levels)
- ✅ Lists of dicts
- ✅ Lists of primitives
- ✅ Mixed lists (dicts + primitives)
- ✅ Empty dicts/lists
- ✅ None values
- ✅ UUID, date, datetime values
- ✅ Acronyms (IP, DNS, HTTP)
- ✅ Consecutive capitals
- ✅ Single-letter keys

### Integration Tests (Phase 3)
- ✅ PostgreSQL composite type with `jsonb_populate_record()`
- ✅ Mutation with camelCase input (verifies conversion)
- ✅ Mutation with `auto_camel_case=False` (no conversion)
- ✅ Nested input objects (recursive conversion)

### Regression Tests
- ✅ All existing camelCase output tests still pass
- ✅ All existing mutation tests still pass
- ✅ No impact on query performance

---

## Rollout Plan

### Step 1: Implement in FraiseQL
1. Complete Phase 1-3
2. Run full test suite
3. Create PR with tests

### Step 2: Update Documentation
1. Update `docs/features/auto-camel-case.md`
2. Add migration guide for SQL workaround removal
3. Add changelog entry for v1.8.1

### Step 3: Deploy to PrintOptim
1. Update FraiseQL dependency
2. Remove `core.jsonb_camel_to_snake()` SQL function
3. Update all wrapper functions to remove workaround calls
4. Run full test suite
5. Deploy

---

## Risk Assessment

### Low Risk
- **Change is isolated**: Only affects mutation input serialization
- **Backward compatible**: When `auto_camel_case=False`, no conversion applied
- **Well-tested**: 15+ unit tests + 3 integration tests
- **Reversible**: Can disable with `auto_camel_case=False` if issues arise

### Mitigation
- Phase-based rollout (3 phases, each verified independently)
- Comprehensive test coverage (unit + integration)
- No changes to Rust pipeline (stable foundation)
- No changes to output conversion (already working)

---

## Success Criteria

### Phase 1
- [ ] Test file created with 15+ test cases
- [ ] All tests FAIL with `NotImplementedError`
- [ ] Linting passes

### Phase 2
- [ ] `dict_keys_to_snake_case()` implemented
- [ ] All unit tests pass
- [ ] No regressions in existing tests

### Phase 3
- [ ] Integration test created
- [ ] `rust_executor.py` integration complete
- [ ] All new tests pass
- [ ] All existing tests pass (full suite)

### Overall
- [ ] PrintOptim can remove SQL workaround
- [ ] Documentation updated
- [ ] Changelog entry added

---

## Notes

### Why This Approach

1. **TDD**: Tests first ensures correct behavior from the start
2. **Incremental**: Each phase is independently verifiable
3. **Reversible**: Small, focused changes easy to rollback if needed
4. **Low Risk**: No changes to critical paths (Rust pipeline, output conversion)

### Performance Impact

- **Negligible**: Dict conversion is O(n) where n = total keys in input
- **Typical input**: 5-20 keys → ~0.01ms overhead
- **No impact**: Queries don't use this path

### Related Issues

- PrintOptim mutation failures (all CQRS mutations affected)
- Any project using `jsonb_populate_record()` with FraiseQL mutations
- Future projects using composite types with FraiseQL

---

## Contact

**Issue Reporter**: PrintOptim team
**Verified By**: Claude Code (Architect)
**Implementation**: To be assigned
