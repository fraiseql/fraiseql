# Phase 2: Consolidate Duplicate Test Files (REFACTOR)

## Objective
Merge duplicate test files into single comprehensive test files, ensuring no test coverage is lost.

## Context
During development, multiple test files were created for the same features with different suffixes (_simple, _fixed, _complex). These need to be consolidated into single, well-organized test files.

## Files to Modify
Based on inventory, we have 4 consolidation tasks:
1. Field authorization tests (4 files → 1)
2. Error array tests (2 files → 1)
3. Decorators tests (1 file → rename only, handled in Phase 3)
4. Validators tests (1 file → rename only, handled in Phase 3)

## Implementation Steps

### Step 1: Consolidate Field Authorization Tests

This is the largest consolidation task.

#### 1.1: Review all field authorization test files

```bash
# List all field auth test files
ls -lh tests/integration/auth/test_field_auth*.py
```

**Expected output**: 4 files listed
- `test_field_authorization.py`
- `test_field_authorization_simple.py`
- `test_field_authorization_fixed.py`
- `test_field_auth_complex.py`

#### 1.2: Read and understand each file

Read each file to understand what tests it contains:

```bash
# Show test function names in each file
echo "=== test_field_authorization.py ==="
grep "def test_" tests/integration/auth/test_field_authorization.py | head -20

echo "=== test_field_authorization_simple.py ==="
grep "def test_" tests/integration/auth/test_field_authorization_simple.py | head -20

echo "=== test_field_authorization_fixed.py ==="
grep "def test_" tests/integration/auth/test_field_authorization_fixed.py | head -20

echo "=== test_field_auth_complex.py ==="
grep "def test_" tests/integration/auth/test_field_auth_complex.py | head -20
```

**Expected output**: List of test function names from each file

**Action**: Note any duplicate test names - you'll need to deduplicate.

#### 1.3: Create consolidated file

Strategy:
- Use `test_field_authorization.py` as the base (KEEP file)
- Add any unique tests from the other 3 files
- Organize tests into logical sections with comments
- Remove duplicate tests (keep the most comprehensive version)

**Current state analysis**:
```bash
# Base file has 2 tests in a class:
# - test_field_auth_basic_error_handling
# - test_field_auth_integration_with_graphql

# Simple file has 3 standalone tests (no class):
# - test_field_authorization_in_graphql (DUPLICATE of base)
# - test_simple_permission_check (UNIQUE)
# - test_field_authorization_error (SIMILAR to base error handling)

# Complex file has 7 tests in TestComplexFieldAuthorization class:
# - test_nested_permission_checks (UNIQUE)
# - test_async_permission_with_database_check (UNIQUE)
# - test_permission_with_field_arguments (UNIQUE)
# - test_rate_limiting_permission (UNIQUE)
# - test_mixed_sync_async_permissions (UNIQUE)
# - test_context_based_field_visibility (UNIQUE)
# - test_permission_with_custom_error_codes (UNIQUE)
```

**Consolidation example** (before/after):

```python
# BEFORE: test_field_authorization.py (base)
class TestFieldAuthorization:
    def test_field_auth_basic_error_handling(self) -> None:
        """Test error handling when field authorization fails."""
        ...

    def test_field_auth_integration_with_graphql(self) -> None:
        """Test field authorization works with GraphQL queries."""
        ...

# BEFORE: test_field_authorization_simple.py (merge from here)
def test_field_authorization_in_graphql() -> None:
    """Test field authorization in GraphQL queries."""
    # DUPLICATE - similar to test_field_auth_integration_with_graphql
    # DECISION: Skip this, keep base version
    ...

def test_simple_permission_check() -> None:
    """Test simple permission checks on fields."""
    # UNIQUE - need to copy this
    ...

# BEFORE: test_field_auth_complex.py (merge from here)
class TestComplexFieldAuthorization:
    def test_nested_permission_checks(self) -> None:
        """Test permissions on nested object fields."""
        # UNIQUE - need to copy this
        ...

    # ... (6 more unique tests)

# ====================================================================
# AFTER: test_field_authorization.py (consolidated)
# ====================================================================

class TestFieldAuthorization:
    # ============================================================
    # Basic Field Authorization
    # ============================================================

    def test_field_auth_basic_error_handling(self) -> None:
        """Test error handling when field authorization fails."""
        # FROM: base file (original)
        ...

    def test_simple_permission_check(self) -> None:
        """Test simple permission checks on fields."""
        # FROM: test_field_authorization_simple.py
        ...

    def test_field_auth_integration_with_graphql(self) -> None:
        """Test field authorization works with GraphQL queries."""
        # FROM: base file (original)
        # NOTE: Skipped duplicate from _simple file
        ...

    # ============================================================
    # Advanced Field Authorization
    # ============================================================

    def test_nested_permission_checks(self) -> None:
        """Test permissions on nested object fields."""
        # FROM: test_field_auth_complex.py
        ...

    def test_permission_with_field_arguments(self) -> None:
        """Test that field arguments are considered in permissions."""
        # FROM: test_field_auth_complex.py
        ...

    def test_context_based_field_visibility(self) -> None:
        """Test field visibility changes based on request context."""
        # FROM: test_field_auth_complex.py
        ...

    def test_permission_with_custom_error_codes(self) -> None:
        """Test custom error codes in permission failures."""
        # FROM: test_field_auth_complex.py
        ...

    # ============================================================
    # Async & Database Integration
    # ============================================================

    async def test_async_permission_with_database_check(self) -> None:
        """Test async permission checks with database queries."""
        # FROM: test_field_auth_complex.py
        ...

    async def test_mixed_sync_async_permissions(self) -> None:
        """Test mixing sync and async permission checks."""
        # FROM: test_field_auth_complex.py
        ...

    # ============================================================
    # Special Cases
    # ============================================================

    def test_rate_limiting_permission(self) -> None:
        """Test rate limiting applied via field permissions."""
        # FROM: test_field_auth_complex.py
        ...
```

**Deduplication decision rules**:
1. **Identical names + similar assertions** → Keep one (usually from base)
2. **Similar names but different assertions** → Keep both, clarify names if needed
3. **Different names testing same feature** → Keep the more comprehensive one
4. **Unique tests** → Always copy to consolidated file

**Manual steps**:
1. Open `tests/integration/auth/test_field_authorization.py` in your editor
2. Open the three other files in separate tabs/windows
3. Create section comments in the base file (Basic, Advanced, Async, Special Cases)
4. For each test in _simple file:
   - `test_field_authorization_in_graphql` → SKIP (duplicate of base)
   - `test_simple_permission_check` → COPY to "Basic" section
   - `test_field_authorization_error` → Compare with `test_field_auth_basic_error_handling`, merge if different
5. For each test in _complex file:
   - Copy all 7 tests to appropriate sections based on complexity
   - Group async tests together
   - Group database tests together
6. Delete any `# FROM:` comments after consolidation is verified
7. Ensure all imports are present (copy from merged files if needed)
8. Ensure consistent indentation and style

#### 1.4: Verify no tests were lost

```bash
# Count test functions before consolidation
echo "Before consolidation:"
grep -c "def test_" tests/integration/auth/test_field_authorization.py
grep -c "def test_" tests/integration/auth/test_field_authorization_simple.py
grep -c "def test_" tests/integration/auth/test_field_authorization_fixed.py
grep -c "def test_" tests/integration/auth/test_field_auth_complex.py

# After consolidation (manually count in your editor)
echo "After consolidation (count unique tests in consolidated file):"
grep -c "def test_" tests/integration/auth/test_field_authorization.py
```

**Expected**: Total test count should be equal or slightly less (due to deduplication)

#### 1.5: Test the consolidated file

```bash
# Run only the consolidated field auth tests
uv run pytest tests/integration/auth/test_field_authorization.py -v
```

**Expected output**: All tests pass

**If tests fail**:
- Check imports are correct
- Check fixtures are present
- Check for copy-paste errors

#### 1.6: Delete the merged files

Only after tests pass:

```bash
git rm tests/integration/auth/test_field_authorization_simple.py
git rm tests/integration/auth/test_field_authorization_fixed.py
git rm tests/integration/auth/test_field_auth_complex.py
```

### Step 2: Consolidate Error Array Tests

This is simpler - one file is just placeholders.

#### 2.1: Review error array test files

```bash
ls -lh tests/integration/graphql/mutations/test_*error*.py
```

**Expected output**: 2 files
- `test_native_error_arrays.py` (keep this one)
- `test_error_arrays.py` (delete this one)

#### 2.2: Verify test_error_arrays.py only has placeholders

```bash
grep "assert True" tests/integration/graphql/mutations/test_error_arrays.py
```

**Expected output**: Several `assert True` placeholder tests found

**Action**: Confirm this file has no real test logic.

#### 2.3: Delete the placeholder file

```bash
git rm tests/integration/graphql/mutations/test_error_arrays.py
```

**Note**: We'll rename `test_native_error_arrays.py` → `test_error_arrays.py` in Phase 3.

### Step 3: Verify Consolidation

Run all integration tests to ensure nothing broke:

```bash
# Run full integration test suite
uv run pytest tests/integration/ -v --tb=short
```

**Expected output**: All tests pass

**If tests fail**:
1. Check which test file failed
2. Review the consolidation for that file
3. Look for missing imports, fixtures, or test logic
4. Fix and re-run

### Step 4: Check Coverage Maintained

```bash
# Run tests with coverage report (optional but recommended)
uv run pytest tests/integration/ --cov=fraiseql --cov-report=term-missing --tb=short
```

**Expected output**: Coverage percentage should be same or better than before

## Verification Commands

```bash
# Verify files deleted
test ! -f tests/integration/auth/test_field_authorization_simple.py && echo "✓ simple deleted" || echo "✗ simple still exists"
test ! -f tests/integration/auth/test_field_authorization_fixed.py && echo "✓ fixed deleted" || echo "✗ fixed still exists"
test ! -f tests/integration/auth/test_field_auth_complex.py && echo "✓ complex deleted" || echo "✗ complex still exists"
test ! -f tests/integration/graphql/mutations/test_error_arrays.py && echo "✓ error_arrays deleted" || echo "✗ error_arrays still exists"

# Verify consolidated files exist
test -f tests/integration/auth/test_field_authorization.py && echo "✓ field_authorization exists" || echo "✗ field_authorization missing"
test -f tests/integration/graphql/mutations/test_native_error_arrays.py && echo "✓ native_error_arrays exists" || echo "✗ native_error_arrays missing"

# Run tests
uv run pytest tests/integration/auth/test_field_authorization.py -v
uv run pytest tests/integration/graphql/mutations/test_native_error_arrays.py -v
```

**Expected output**: All verifications pass

## Acceptance Criteria

- [ ] Field authorization tests consolidated into single file
- [ ] Placeholder error array test file deleted
- [ ] All consolidated tests pass
- [ ] No test functions were lost (except duplicates)
- [ ] Git shows 4 files deleted
- [ ] Full integration test suite passes

## Commit

After verification passes:

```bash
git add tests/integration/
git commit -m "refactor(tests): consolidate duplicate integration test files [REFACTOR]

Merge duplicate test files into single comprehensive files:
- Field authorization: 4 files → 1 file
- Error arrays: delete placeholder file (keep native implementation)

No test coverage lost. All tests passing."
```

## DO NOT

- ❌ Delete files before verifying consolidated tests pass
- ❌ Skip running the full test suite
- ❌ Lose any unique test cases during consolidation
- ❌ Proceed to Phase 3 without committing this phase

## Troubleshooting

**Problem**: Consolidated tests fail with import errors
**Solution**: Check that all imports from merged files are present in consolidated file

**Problem**: Can't decide which version of a duplicate test to keep
**Solution**: Keep the most comprehensive version (most assertions, best coverage)

**Problem**: Tests pass individually but fail in suite
**Solution**: Check for test isolation issues, fixture conflicts, or shared state

## Notes for Junior Engineers

**Why consolidate instead of just deleting?**
We want to keep all the test coverage. Each file might test different edge cases.

**How do I know if tests are duplicates?**
- Same test name = probably duplicate (check assertions to confirm)
- Same assertions = definitely duplicate (keep the better-documented version)
- Different assertions for same feature = NOT duplicates, keep both
- Example: `test_field_auth_integration_with_graphql` (base) vs `test_field_authorization_in_graphql` (_simple)
  - Read both test bodies
  - If they query the same thing and assert the same results → duplicate
  - If they test different aspects → keep both with clearer names

**What if consolidated file is too large?**
That's okay for now. If a single test file has >500 lines, consider splitting by feature area (not by development iteration).

**How to organize tests in consolidated file?**
Group by feature complexity:
1. Basic functionality tests
2. Tests with relations/joins
3. Edge cases and error conditions
4. Performance/integration tests

**Time estimate**: ~2 hours
- Field authorization consolidation: ~1.5 hours
- Error arrays: ~15 minutes
- Verification: ~15 minutes
