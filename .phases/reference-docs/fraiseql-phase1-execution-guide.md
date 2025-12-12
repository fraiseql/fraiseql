# FraiseQL Phase 1 Execution Guide

**Quick Start Guide for Immediate Execution**

**Phase**: Phase 1 - v1.8.1 Test Updates
**Duration**: 2-4 hours
**Tests Fixed**: 16
**Difficulty**: LOW
**Risk**: LOW

---

## Pre-Execution Checklist

- [ ] FraiseQL repository at `/home/lionel/code/fraiseql`
- [ ] On branch `feature/post-v1.8.0-improvements` (or create new branch)
- [ ] Virtual environment activated
- [ ] Current test results: 214 failures

---

## Step 1: Create Branch (5 minutes)

```bash
cd /home/lionel/code/fraiseql

# Option A: Continue on current branch
git status  # Ensure clean state

# Option B: Create new branch (recommended)
git checkout -b test-suite-100-percent
git push -u origin test-suite-100-percent
```

**Verification**:
```bash
git branch  # Should show test-suite-100-percent (or current branch)
```

---

## Step 2: Run Baseline Tests (2 minutes)

```bash
# Run the 16 tests we'll be fixing
uv run pytest tests/unit/mutations/test_auto_populate_schema.py -v
uv run pytest tests/unit/decorators/test_decorators.py -v
uv run pytest tests/integration/graphql/mutations/test_native_error_arrays.py -v

# Expected: All should FAIL (that's why we're fixing them)
```

**Expected Output**:
```
tests/unit/mutations/test_auto_populate_schema.py::test_success_decorator_adds_fields_to_gql_fields FAILED
tests/unit/mutations/test_auto_populate_schema.py::test_failure_decorator_adds_fields FAILED
tests/unit/mutations/test_auto_populate_schema.py::test_no_entity_field_no_id FAILED
tests/unit/mutations/test_auto_populate_schema.py::test_user_defined_fields_not_overridden FAILED
... (16 total failures)
```

---

## Step 3: Fix Category 1 - Success Type Tests (30-45 minutes)

### File: `tests/unit/mutations/test_auto_populate_schema.py`

**Current Issue**: Tests expect v1.8.0 field semantics (Success types had `errors` field)
**v1.8.1 Change**: Success types NO LONGER have `errors` field

#### Test 1: `test_success_decorator_adds_fields_to_gql_fields`

**Current (lines 14-34)**:
```python
def test_success_decorator_adds_fields_to_gql_fields():
    """Auto-populated fields should be in __gql_fields__ for schema generation."""

    @success
    class CreateMachineSuccess:
        machine: Machine

    gql_fields = getattr(CreateMachineSuccess, "__gql_fields__", {})

    # All auto-populated fields must be present
    assert "machine" in gql_fields, "Original field should be present"
    assert "status" in gql_fields, "Auto-injected status missing"
    assert "message" in gql_fields, "Auto-injected message missing"
    assert "errors" in gql_fields, "Auto-injected errors missing"  # ❌ WRONG
    assert "updated_fields" in gql_fields, "Auto-injected updatedFields missing"
    assert "id" in gql_fields, "Auto-injected id missing (entity detected)"

    # Verify field types
    assert gql_fields["status"].field_type == str
    assert gql_fields["message"].field_type == str | None
```

**Fixed**:
```python
def test_success_decorator_adds_fields_to_gql_fields():
    """Auto-populated fields should be in __gql_fields__ for schema generation."""

    @success
    class CreateMachineSuccess:
        machine: Machine

    gql_fields = getattr(CreateMachineSuccess, "__gql_fields__", {})

    # All auto-populated fields must be present
    assert "machine" in gql_fields, "Original field should be present"
    assert "status" in gql_fields, "Auto-injected status missing"
    assert "message" in gql_fields, "Auto-injected message missing"
    assert "errors" not in gql_fields, "Success types should NOT have errors field (v1.8.1)"  # ✅ FIXED
    assert "updated_fields" in gql_fields, "Auto-injected updatedFields missing"
    assert "id" in gql_fields, "Auto-injected id missing (entity detected)"

    # Verify field types
    assert gql_fields["status"].field_type == str
    assert gql_fields["message"].field_type == str | None
```

**Change**: Line 27 - `assert "errors" in gql_fields` → `assert "errors" not in gql_fields`

#### Test 2: `test_no_entity_field_no_id`

**Current (lines 53-71)**:
```python
def test_no_entity_field_no_id():
    """ID should not be added when no entity field present."""

    @success
    class DeleteSuccess:
        """Deletion confirmation without entity."""
        pass

    gql_fields = getattr(DeleteSuccess, "__gql_fields__", {})

    # Standard fields should be present
    assert "status" in gql_fields
    assert "message" in gql_fields
    assert "errors" in gql_fields  # ❌ WRONG
    assert "updated_fields" in gql_fields

    # But NOT id (no entity field detected)
    assert "id" not in gql_fields
```

**Fixed**:
```python
def test_no_entity_field_no_id():
    """ID should not be added when no entity field present."""

    @success
    class DeleteSuccess:
        """Deletion confirmation without entity."""
        pass

    gql_fields = getattr(DeleteSuccess, "__gql_fields__", {})

    # Standard fields should be present
    assert "status" in gql_fields
    assert "message" in gql_fields
    assert "errors" not in gql_fields  # ✅ FIXED - Success types don't have errors
    assert "updated_fields" in gql_fields

    # But NOT id (no entity field detected)
    assert "id" not in gql_fields
```

**Change**: Line 67 - `assert "errors" in gql_fields` → `assert "errors" not in gql_fields`

#### Test 3: `test_user_defined_fields_not_overridden`

**Current (lines 74-89)**:
```python
def test_user_defined_fields_not_overridden():
    """User's explicit field definitions should be preserved."""

    @success
    class CreateMachineSuccess:
        machine: Machine
        status: str = "custom_success"

    gql_fields = getattr(CreateMachineSuccess, "__gql_fields__", {})

    # User-defined status should be preserved
    assert "status" in gql_fields
    # But auto-injected fields should still be added
    assert "message" in gql_fields
    assert "errors" in gql_fields  # ❌ WRONG
```

**Fixed**:
```python
def test_user_defined_fields_not_overridden():
    """User's explicit field definitions should be preserved."""

    @success
    class CreateMachineSuccess:
        machine: Machine
        status: str = "custom_success"

    gql_fields = getattr(CreateMachineSuccess, "__gql_fields__", {})

    # User-defined status should be preserved
    assert "status" in gql_fields
    # But auto-injected fields should still be added
    assert "message" in gql_fields
    assert "errors" not in gql_fields  # ✅ FIXED - Success types don't have errors
    assert "updated_fields" in gql_fields  # Auto-injected for success types
    assert "id" in gql_fields  # Auto-injected when entity field present
```

**Changes**:
- Line 88: `assert "errors" in gql_fields` → `assert "errors" not in gql_fields`
- Add lines 89-90 to test other auto-injected fields

**Verify**:
```bash
uv run pytest tests/unit/mutations/test_auto_populate_schema.py::test_success_decorator_adds_fields_to_gql_fields -v
uv run pytest tests/unit/mutations/test_auto_populate_schema.py::test_no_entity_field_no_id -v
uv run pytest tests/unit/mutations/test_auto_populate_schema.py::test_user_defined_fields_not_overridden -v

# Expected: 3 tests PASS
```

---

## Step 4: Fix Category 2 - Error Type Tests (30-45 minutes)

### Same File: `tests/unit/mutations/test_auto_populate_schema.py`

**Current Issue**: Tests expect v1.8.0 field semantics (Error types had `updated_fields` and `id`)
**v1.8.1 Change**: Error types NO LONGER have `updated_fields` or `id` (semantically incorrect)

#### Test 4: `test_failure_decorator_adds_fields`

**Current (lines 36-51)**:
```python
def test_failure_decorator_adds_fields():
    """Failure types should also get auto-populated fields."""

    @failure
    class CreateMachineError:
        error_code: str

    gql_fields = getattr(CreateMachineError, "__gql_fields__", {})

    assert "status" in gql_fields
    assert "message" in gql_fields
    assert "errors" in gql_fields
    assert "updated_fields" in gql_fields  # ❌ WRONG
    # Has entity field (error_code), so id should be added
    assert "id" in gql_fields  # ❌ WRONG
```

**Fixed**:
```python
def test_failure_decorator_adds_fields():
    """Failure types should also get auto-populated fields."""

    @failure
    class CreateMachineError:
        error_code: str

    gql_fields = getattr(CreateMachineError, "__gql_fields__", {})

    assert "status" in gql_fields
    assert "message" in gql_fields
    assert "errors" in gql_fields
    assert "code" in gql_fields  # ✅ NEW - Auto-injected in v1.8.1
    assert "updated_fields" not in gql_fields  # ✅ FIXED - Errors don't update
    assert "id" not in gql_fields  # ✅ FIXED - Errors don't create entities

    # Verify field types
    assert gql_fields["code"].field_type == int  # Error code is integer
```

**Changes**:
- Line 47: `assert "updated_fields" in gql_fields` → `assert "updated_fields" not in gql_fields`
- Line 49: `assert "id" in gql_fields` → `assert "id" not in gql_fields`
- Add line 46: `assert "code" in gql_fields` (new auto-injected field)
- Add lines 52-53: Verify code field type

**Verify**:
```bash
uv run pytest tests/unit/mutations/test_auto_populate_schema.py::test_failure_decorator_adds_fields -v

# Expected: 1 test PASS
```

---

## Step 5: Fix Category 3 - Field Order Tests (30 minutes)

### File: `tests/unit/decorators/test_decorators.py`

**Current Issue**: Field order changed due to v1.8.1 auto-injection changes

**Strategy**: Run the test, see what order fields are actually in, update expectations.

```bash
# Run test to see actual field order
uv run pytest tests/unit/decorators/test_decorators.py::test_success_decorator_field_order -v

# Look at assertion error to see actual vs expected order
```

**Expected Output**:
```
AssertionError: Expected field order: ['machine', 'status', 'message', 'errors', 'updated_fields', 'id']
Actual field order: ['machine', 'status', 'message', 'updated_fields', 'id']
```

**Fix**: Update expected order to match actual (remove 'errors'):

```python
def test_success_decorator_field_order():
    """Fields should appear in deterministic order."""

    @success
    class CreateMachineSuccess:
        machine: Machine

    gql_fields = getattr(CreateMachineSuccess, "__gql_fields__", {})
    field_names = list(gql_fields.keys())

    # Expected order: user fields first, then auto-injected fields
    expected_order = ['machine', 'status', 'message', 'updated_fields', 'id']  # ✅ FIXED - removed 'errors'

    assert field_names == expected_order, (
        f"Field order incorrect.\n"
        f"Expected: {expected_order}\n"
        f"Actual:   {field_names}"
    )
```

**Repeat for error decorator**:

```bash
uv run pytest tests/unit/decorators/test_decorators.py::test_failure_decorator_field_order -v
```

Update based on actual output (likely: remove 'updated_fields' and 'id', add 'code').

**Verify**:
```bash
uv run pytest tests/unit/decorators/test_decorators.py -v

# Expected: All field order tests PASS
```

---

## Step 6: Fix Category 4 - Integration Tests (45-60 minutes)

### File: `tests/integration/graphql/mutations/test_native_error_arrays.py`

**Current Issue**: Tests query for `updatedFields` and `id` on Error types (removed in v1.8.1)

**Strategy**: Remove these fields from GraphQL query fragments for Error types.

**Pattern to Find**:
```graphql
... on CreateMachineError {
    code
    status
    message
    errors { identifier message }
    updatedFields  # ❌ Remove this
    id             # ❌ Remove this
}
```

**Pattern to Replace**:
```graphql
... on CreateMachineError {
    code
    status
    message
    errors { identifier message }
    # updatedFields removed - errors don't update
    # id removed - errors don't create entities
}
```

**Search and Replace**:
```bash
# Search for error fragments with updatedFields
grep -n "updatedFields" tests/integration/graphql/mutations/test_native_error_arrays.py

# Manually review and remove from error type fragments
# Keep updatedFields on SUCCESS type fragments
```

**Verify Each Change**:
```bash
# After each fragment update, run the test
uv run pytest tests/integration/graphql/mutations/test_native_error_arrays.py::test_specific_mutation -v
```

---

## Step 7: Final Verification (10 minutes)

```bash
# Run all 16 tests we fixed
uv run pytest tests/unit/mutations/test_auto_populate_schema.py -v
uv run pytest tests/unit/decorators/test_decorators.py -v
uv run pytest tests/integration/graphql/mutations/test_native_error_arrays.py -v

# Expected: ALL PASS (16/16)
```

**Success Criteria**:
- [ ] All 16 tests pass
- [ ] No new failures introduced
- [ ] Tests reflect v1.8.1 semantics:
  - [ ] Success types: NO `errors` field
  - [ ] Error types: NO `updated_fields` or `id` fields
  - [ ] Error types: YES `code` field (auto-injected)

---

## Step 8: Commit Changes (10 minutes)

```bash
# Review changes
git status
git diff

# Stage changes
git add tests/unit/mutations/test_auto_populate_schema.py
git add tests/unit/decorators/test_decorators.py
git add tests/integration/graphql/mutations/test_native_error_arrays.py

# Commit with descriptive message
git commit -m "$(cat <<'EOF'
test(mutations): update tests for v1.8.1 field semantics [Phase 1]

Updated mutation decorator tests to match FraiseQL v1.8.1 auto-injection semantics:

**Success Types** (v1.8.1 changes):
- ❌ REMOVED: `errors` field (semantically incorrect - success doesn't have errors)
- ✅ KEPT: `status`, `message`, `updated_fields`, `id` (conditional)

**Error Types** (v1.8.1 changes):
- ✅ ADDED: `code` field (auto-injected, replaces manual definition)
- ❌ REMOVED: `updated_fields` field (errors don't update entities)
- ❌ REMOVED: `id` field (errors don't create entities)
- ✅ KEPT: `status`, `message`, `errors`

**Files Updated**:
- tests/unit/mutations/test_auto_populate_schema.py (4 tests)
- tests/unit/decorators/test_decorators.py (2 tests)
- tests/integration/graphql/mutations/test_native_error_arrays.py (~10 tests)

**Impact**: 16 tests fixed, 198 failures remaining

**Phase**: 1/4 (Quick Wins)
**Related**: .phases/fraiseql-auto-injection-redesign/
EOF
)"

# Push to remote
git push origin test-suite-100-percent
```

---

## Step 9: Document Progress (5 minutes)

```bash
# Update progress tracker
echo "## Phase 1 Complete - $(date)" >> /tmp/fraiseql-test-progress.md
echo "" >> /tmp/fraiseql-test-progress.md
echo "- Tests fixed: 16" >> /tmp/fraiseql-test-progress.md
echo "- Failures remaining: 198" >> /tmp/fraiseql-test-progress.md
echo "- Time taken: [FILL IN]" >> /tmp/fraiseql-test-progress.md
echo "- Commit: $(git rev-parse HEAD)" >> /tmp/fraiseql-test-progress.md
echo "" >> /tmp/fraiseql-test-progress.md

# Run full test suite to see new baseline
uv run pytest --tb=no -q 2>&1 | tail -5
```

---

## Troubleshooting

### Issue: Tests still failing after updates

**Check**:
1. Did you update ALL instances of `assert "errors" in gql_fields` for Success types?
2. Did you update ALL instances for Error types (`updated_fields`, `id`)?
3. Did you add `assert "code" in gql_fields` for Error types?

**Debug**:
```python
# Add debugging to see actual fields
gql_fields = getattr(CreateMachineSuccess, "__gql_fields__", {})
print(f"Actual fields: {list(gql_fields.keys())}")
```

### Issue: Integration tests still failing

**Check**:
1. Did you remove `updatedFields` and `id` from ERROR fragments only (not Success)?
2. Did you keep `code` in Error fragments?

**Debug**:
```bash
# Search for remaining issues
grep -B 5 -A 5 "updatedFields" tests/integration/graphql/mutations/test_native_error_arrays.py | grep -A 5 "Error"
```

### Issue: Field order tests failing

**Solution**: Run test, see actual order, update expected order to match.

```bash
# See actual vs expected
uv run pytest tests/unit/decorators/test_decorators.py::test_success_decorator_field_order -v 2>&1 | grep -A 3 "AssertionError"
```

---

## Success Metrics

**Before Phase 1**:
- Failed: 214
- Passed: 5,160
- Rate: 96.0%

**After Phase 1 (Target)**:
- Failed: 198
- Passed: 5,176
- Rate: 96.3%

**Progress**: 16 tests fixed (7.5% of all failures)

---

## Next Phase Preview

**Phase 2** (Week 2): SQL Rendering Infrastructure
- Create `tests/helpers/sql_rendering.py`
- Fix ~150 SQL validation test failures
- Effort: 16-20 hours

**Preparation for Phase 2**:
- Read psycopg3 documentation on `Composed` objects
- Familiarize with `tests/regression/where_clause/` test structure
- Consider setting up local AI model for bulk migration

---

**Phase 1 Execution Time**: 2-4 hours
**Ready to Execute**: ✅ YES
**Next Action**: Run Step 1 (Create Branch)
