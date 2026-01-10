# Rename Plan: mutation_result_v2 → mutation_response

## Executive Summary

**Goal**: Rename `mutation_result_v2` to `mutation_response` across the entire codebase.

**Status**: Planning phase
**Estimated Complexity**: Medium (2-3 implementation units)
**Breaking Changes**: None (no external users yet)
**Timeline**: 1-2 days

## Why This Rename?

### Problems with `mutation_result_v2`
1. **Version suffix implies iteration** - Suggests there will be v3, v4, etc.
2. **Not descriptive** - Doesn't convey semantic meaning
3. **Awkward in documentation** - "The mutation_result_v2 type" sounds unfinished
4. **Pre-release opportunity** - No external users = perfect time to fix

### Benefits of `mutation_response`
1. **Semantic clarity** - It's a response from a mutation
2. **Professional** - Aligns with industry naming (Hasura uses `mutation_response` pattern)
3. **Future-proof** - Breaking changes handled via migrations, not version suffixes
4. **Clean** - Simpler to document and explain

## Scope Analysis

Based on codebase search, `mutation_result_v2` appears in:

### PostgreSQL Files (5 files)
- `migrations/trinity/005_add_mutation_result_v2.sql` - Type definition + helper functions
- `examples/mutations_demo/v2_init.sql` - Example functions
- `examples/mutations_demo/v2_mutation_functions.sql` - Demo functions

### Rust Files (2 files)
- `fraiseql_rs/src/mutation/mod.rs` - Core mutation handling
- `fraiseql_rs/src/lib.rs` - Public exports

### Python Files (2 files)
- `src/fraiseql/mutations/entity_flattener.py` - Result parsing
- `src/fraiseql/mutations/rust_executor.py` - Rust FFI calls

### Documentation Files (4 files)
- `docs/mutations/status-strings.md` - Status taxonomy docs
- `docs/features/sql-function-return-format.md` - Function return format guide
- `docs/features/mutation-result-reference.md` - API reference
- `docs/features/graphql-cascade.md` - Cascade documentation
- `CHANGELOG.md` - Change history

### Test Files (3 files)
- `tests/fixtures/cascade/conftest.py` - Test fixtures
- `tests/test_mutations/test_status_taxonomy.py` - Status taxonomy tests
- `tests/integration/graphql/mutations/test_unified_camel_case.py` - Integration tests

## Implementation Phases

### Phase 0: Pre-Implementation Checklist

**Before starting, verify:**
- [ ] No external users (confirmed)
- [ ] Git working tree is clean
- [ ] All tests pass
- [ ] Create backup branch: `git checkout -b backup/before-mutation-response-rename`
- [ ] Create working branch: `git checkout -b refactor/rename-to-mutation-response`

---

### Phase 1: PostgreSQL Migration Files

**Objective**: Rename the PostgreSQL composite type and all helper functions.

#### Task 1.1: Rename Main Migration File

**File**: `migrations/trinity/005_add_mutation_result_v2.sql`

**Actions**:
1. Rename file to: `migrations/trinity/005_add_mutation_response.sql`
2. Update migration header:
   ```sql
   -- Migration: Add mutation_response type and helper functions
   -- Description: Creates PostgreSQL composite type and helper functions for consistent mutation results
   -- Version: 0.1.0
   -- Date: 2025-01-25
   ```

3. Rename type definition (line ~12):
   ```sql
   -- OLD
   CREATE TYPE mutation_result_v2 AS (

   -- NEW
   CREATE TYPE mutation_response AS (
   ```

4. Update all helper function return types:
   - `mutation_success()` - line ~34: `RETURNS mutation_response`
   - `mutation_created()` - line ~61: `RETURNS mutation_response`
   - `mutation_updated()` - line ~87: `RETURNS mutation_response`
   - `mutation_deleted()` - line ~114: `RETURNS mutation_response`
   - `mutation_noop()` - line ~138: `RETURNS mutation_response`
   - `mutation_validation_error()` - line ~162: `RETURNS mutation_response`
   - `mutation_not_found()` - line ~197: `RETURNS mutation_response`
   - `mutation_conflict()` - line ~234: `RETURNS mutation_response`
   - `mutation_error()` - line ~261: `RETURNS mutation_response`

5. Update all `ROW(...)::<type>` casts:
   ```sql
   -- OLD
   )::mutation_result_v2;

   -- NEW
   )::mutation_response;
   ```

6. Update utility function parameter types (lines 281-319):
   ```sql
   -- OLD
   CREATE OR REPLACE FUNCTION mutation_is_success(result mutation_result_v2) RETURNS boolean

   -- NEW
   CREATE OR REPLACE FUNCTION mutation_is_success(result mutation_response) RETURNS boolean
   ```

7. Update example usage comments (lines 536-707):
   - Change `mutation_result_v2` → `mutation_response` in all function definitions
   - Update `RETURNS mutation_result_v2` → `RETURNS mutation_response`

**Verification**:
```bash
# Check file renamed
ls -la migrations/trinity/005_add_mutation_response.sql

# Check no v2 references remain
! grep -i "mutation_result_v2" migrations/trinity/005_add_mutation_response.sql

# Check mutation_response exists
grep -c "mutation_response" migrations/trinity/005_add_mutation_response.sql
# Expected: 30+ occurrences
```

#### Task 1.2: Update Example SQL Files

**Files**:
- `examples/mutations_demo/v2_init.sql`
- `examples/mutations_demo/v2_mutation_functions.sql`

**Actions for each file**:
1. Update file header comments:
   ```sql
   -- OLD
   -- Updated init.sql using mutation_result_v2 format

   -- NEW
   -- Updated init.sql using mutation_response format
   ```

2. Global find/replace: `mutation_result_v2` → `mutation_response`

3. Update all function return types:
   ```sql
   -- OLD
   RETURNS mutation_result_v2 AS $$

   -- NEW
   RETURNS mutation_response AS $$
   ```

4. Update all type casts:
   ```sql
   -- OLD
   )::mutation_result_v2;

   -- NEW
   )::mutation_response;
   ```

**Verification**:
```bash
# Check no v2 references remain
! grep -i "mutation_result_v2" examples/mutations_demo/v2_init.sql
! grep -i "mutation_result_v2" examples/mutations_demo/v2_mutation_functions.sql

# Check mutation_response exists
grep -c "mutation_response" examples/mutations_demo/v2_init.sql
# Expected: 10+ occurrences
```

**Acceptance Criteria**:
- [ ] Main migration file renamed and updated
- [ ] All helper functions return `mutation_response`
- [ ] All example SQL files updated
- [ ] No `mutation_result_v2` references in SQL files
- [ ] SQL syntax is valid (no typos in replacements)

---

### Phase 2: Rust Layer Updates

**Objective**: Update Rust code to use the new type name.

#### Task 2.1: Update Core Mutation Module

**File**: `fraiseql_rs/src/mutation/mod.rs`

**Actions**:
1. Update module documentation (line ~3):
   ```rust
   //! Mutation result transformation module
   //!
   //! Transforms PostgreSQL mutation_response JSON into GraphQL responses.
   ```

2. Update function documentation (line ~16):
   ```rust
   /// Supports TWO formats:
   /// 1. **Simple format**: Just entity JSONB (no status field) - auto-detected
   /// 2. **Full format**: Complete mutation_response with status, message, etc.
   ```

3. Search for `mutation_result_v2` in comments and update to `mutation_response`

4. Check if there are any string literals that need updating:
   ```rust
   // Search for any hardcoded "mutation_result_v2" strings
   // Example: logging, error messages, etc.
   ```

**Note**: The Rust code likely doesn't hardcode the PostgreSQL type name in string literals, since it parses JSON directly. Most changes will be in comments/documentation.

**Verification**:
```bash
# Check for any v2 references
! grep -i "mutation_result_v2" fraiseql_rs/src/mutation/mod.rs

# Check mutation_response in comments
grep "mutation_response" fraiseql_rs/src/mutation/mod.rs
```

#### Task 2.2: Update Library Exports

**File**: `fraiseql_rs/src/lib.rs`

**Actions**:
1. Check for any public exports or documentation mentioning `mutation_result_v2`
2. Update any module-level documentation

**Verification**:
```bash
! grep -i "mutation_result_v2" fraiseql_rs/src/lib.rs
```

#### Task 2.3: Rebuild Rust Library

**Actions**:
```bash
cd fraiseql_rs
cargo clean
cargo build --release
cargo test
```

**Acceptance Criteria**:
- [ ] All Rust documentation updated
- [ ] No `mutation_result_v2` references in Rust code
- [ ] Rust builds successfully
- [ ] Rust tests pass

---

### Phase 3: Python Layer Updates

**Objective**: Update Python code to use the new type name.

#### Task 3.1: Update Entity Flattener

**File**: `src/fraiseql/mutations/entity_flattener.py`

**Actions**:
1. Update docstrings and comments:
   ```python
   # OLD
   """Parse mutation_result_v2 format from PostgreSQL."""

   # NEW
   """Parse mutation_response format from PostgreSQL."""
   ```

2. Update any inline comments mentioning the type

3. Check for string literals (unlikely, but verify)

**Verification**:
```bash
! grep -i "mutation_result_v2" src/fraiseql/mutations/entity_flattener.py
grep "mutation_response" src/fraiseql/mutations/entity_flattener.py
```

#### Task 3.2: Update Rust Executor

**File**: `src/fraiseql/mutations/rust_executor.py`

**Actions**:
1. Update docstrings mentioning the type format
2. Update any comments explaining the data structure

**Verification**:
```bash
! grep -i "mutation_result_v2" src/fraiseql/mutations/rust_executor.py
```

**Acceptance Criteria**:
- [ ] All Python docstrings updated
- [ ] No `mutation_result_v2` references in Python code
- [ ] Python imports still work

---

### Phase 4: Documentation Updates

**Objective**: Update all user-facing documentation.

#### Task 4.1: Update Status Strings Documentation

**File**: `docs/mutations/status-strings.md`

**Actions**:
1. Global find/replace: `mutation_result_v2` → `mutation_response`
2. Review examples to ensure they make sense with new name
3. Update any code blocks showing PostgreSQL functions

**Verification**:
```bash
! grep -i "mutation_result_v2" docs/mutations/status-strings.md
grep -c "mutation_response" docs/mutations/status-strings.md
```

#### Task 4.2: Update SQL Function Return Format Guide

**File**: `docs/features/sql-function-return-format.md`

**Actions**:
1. Update all references to the type name
2. Update diagrams or tables showing the type structure
3. Update example SQL functions

**Verification**:
```bash
! grep -i "mutation_result_v2" docs/features/sql-function-return-format.md
```

#### Task 4.3: Update Mutation Result Reference

**File**: `docs/features/mutation-result-reference.md`

**Actions**:
1. Update API reference showing type name
2. Update field descriptions
3. Update return type examples

**Verification**:
```bash
! grep -i "mutation_result_v2" docs/features/mutation-result-reference.md
```

#### Task 4.4: Update GraphQL Cascade Documentation

**File**: `docs/features/graphql-cascade.md`

**Actions**:
1. Update references to mutation response format
2. Update examples showing cascade data

**Verification**:
```bash
! grep -i "mutation_result_v2" docs/features/graphql-cascade.md
```

#### Task 4.5: Update CHANGELOG

**File**: `CHANGELOG.md`

**Actions**:
1. Add new entry:
   ```markdown
   ## [Unreleased]

   ### Changed
   - **BREAKING (Pre-release only)**: Renamed `mutation_result_v2` to `mutation_response`
     - PostgreSQL composite type renamed
     - All helper functions updated
     - Migration file renamed: `005_add_mutation_result_v2.sql` → `005_add_mutation_response.sql`
     - **Impact**: None (no external users yet)
     - **Migration**: Update any custom PostgreSQL functions to return `mutation_response` instead of `mutation_result_v2`
   ```

**Acceptance Criteria**:
- [ ] All documentation files updated
- [ ] No `mutation_result_v2` references in docs
- [ ] CHANGELOG entry added
- [ ] Examples are clear and correct

---

### Phase 5: Test Files Updates

**Objective**: Update test files and fixtures.

#### Task 5.1: Update Cascade Test Fixtures

**File**: `tests/fixtures/cascade/conftest.py`

**Actions**:
1. Update fixture docstrings
2. Update any SQL strings creating test functions
3. Update comments explaining test data format

**Verification**:
```bash
! grep -i "mutation_result_v2" tests/fixtures/cascade/conftest.py
```

#### Task 5.2: Update Status Taxonomy Tests

**File**: `tests/test_mutations/test_status_taxonomy.py`

**Actions**:
1. Update test docstrings
2. Update SQL function definitions in test fixtures
3. Update comments

**Verification**:
```bash
! grep -i "mutation_result_v2" tests/test_mutations/test_status_taxonomy.py
```

#### Task 5.3: Update Integration Tests

**File**: `tests/integration/graphql/mutations/test_unified_camel_case.py`

**Actions**:
1. Update test SQL functions
2. Update test docstrings

**Verification**:
```bash
! grep -i "mutation_result_v2" tests/integration/graphql/mutations/test_unified_camel_case.py
```

#### Task 5.4: Run Full Test Suite

**Actions**:
```bash
# Run all tests
uv run pytest tests/ -v

# Run specific test categories
uv run pytest tests/test_mutations/ -v
uv run pytest tests/integration/graphql/mutations/ -v
uv run pytest tests/fixtures/cascade/ -v
```

**Acceptance Criteria**:
- [ ] All test files updated
- [ ] No `mutation_result_v2` references in tests
- [ ] All tests pass
- [ ] No test failures due to rename

---

### Phase 6: Final Verification

**Objective**: Ensure rename is complete and correct.

#### Task 6.1: Global Verification

**Actions**:
```bash
# 1. Search for any remaining v2 references
grep -r "mutation_result_v2" /home/lionel/code/fraiseql/src/
grep -r "mutation_result_v2" /home/lionel/code/fraiseql/fraiseql_rs/
grep -r "mutation_result_v2" /home/lionel/code/fraiseql/docs/
grep -r "mutation_result_v2" /home/lionel/code/fraiseql/tests/
grep -r "mutation_result_v2" /home/lionel/code/fraiseql/examples/
grep -r "mutation_result_v2" /home/lionel/code/fraiseql/migrations/

# 2. Count mutation_response occurrences
grep -r "mutation_response" /home/lionel/code/fraiseql/ --include="*.py" --include="*.rs" --include="*.sql" --include="*.md" | wc -l
# Expected: 50+ occurrences

# 3. Check migration file exists
ls -la migrations/trinity/005_add_mutation_response.sql

# 4. Check old migration file is gone
! ls -la migrations/trinity/005_add_mutation_result_v2.sql
```

#### Task 6.2: Functional Testing

**Actions**:
```bash
# 1. Run full test suite
uv run pytest tests/ -v --tb=short

# 2. Test specific mutation scenarios
uv run pytest tests/test_mutations/ -v

# 3. Test cascade functionality
uv run pytest tests/integration/graphql/mutations/test_unified_camel_case.py -v

# 4. Test status taxonomy
uv run pytest tests/test_mutations/test_status_taxonomy.py -v
```

#### Task 6.3: Build and Type Check

**Actions**:
```bash
# 1. Python type checking
uv run mypy src/fraiseql/mutations/

# 2. Python linting
uv run ruff check src/fraiseql/mutations/

# 3. Rust build
cd fraiseql_rs && cargo build --release

# 4. Rust tests
cd fraiseql_rs && cargo test
```

**Acceptance Criteria**:
- [ ] No `mutation_result_v2` references anywhere
- [ ] At least 50+ `mutation_response` references found
- [ ] Old migration file deleted
- [ ] New migration file exists
- [ ] All tests pass
- [ ] No type errors
- [ ] No linting errors
- [ ] Rust builds successfully
- [ ] Rust tests pass

---

## Rollback Plan

If issues are discovered during implementation:

### Option 1: Revert Commits
```bash
# If committed incrementally
git log --oneline | head -10  # Find commit before rename
git revert <commit-hash>
```

### Option 2: Restore Backup Branch
```bash
# If rename is complete but problematic
git checkout backup/before-mutation-response-rename
git branch -D refactor/rename-to-mutation-response
git checkout -b refactor/rename-to-mutation-response-v2
# Start over with lessons learned
```

### Option 3: Cherry-pick Good Changes
```bash
# If some phases are good but others need work
git checkout -b refactor/mutation-response-partial
git cherry-pick <good-commit-1> <good-commit-2>
```

---

## Post-Rename Actions

### Update Related Documentation

After rename is complete and tested:

1. **Update README.md** (if it mentions the type)
2. **Update CONTRIBUTING.md** (if it has developer guidelines)
3. **Update any tutorial files** in `docs/tutorials/`

### Communication Plan

Since no external users exist yet:

1. **Internal team notification**: "We've renamed mutation_result_v2 → mutation_response for clarity"
2. **Update any internal wikis or notes**
3. **Mark as complete in project tracker**

### Git Commit Strategy

**Recommended approach**: Commit each phase separately for easy rollback

```bash
# Phase 1
git add migrations/ examples/
git commit -m "refactor(db): rename mutation_result_v2 to mutation_response in PostgreSQL

- Rename migration file to 005_add_mutation_response.sql
- Update all helper functions return types
- Update example SQL files
- Update all type casts and comments"

# Phase 2
git add fraiseql_rs/
git commit -m "refactor(rust): update mutation_response references in Rust layer

- Update module documentation
- Update function comments
- Rebuild and test Rust library"

# Phase 3
git add src/fraiseql/mutations/
git commit -m "refactor(py): update mutation_response references in Python layer

- Update entity_flattener.py docstrings
- Update rust_executor.py comments
- Verify imports still work"

# Phase 4
git add docs/ CHANGELOG.md
git commit -m "docs: update mutation_response references in documentation

- Update all docs files
- Add CHANGELOG entry
- Update examples and references"

# Phase 5
git add tests/
git commit -m "test: update mutation_response references in tests

- Update test fixtures
- Update test SQL functions
- Verify all tests pass"
```

---

## File Checklist

### PostgreSQL Files
- [ ] `migrations/trinity/005_add_mutation_result_v2.sql` → `005_add_mutation_response.sql`
- [ ] `examples/mutations_demo/v2_init.sql`
- [ ] `examples/mutations_demo/v2_mutation_functions.sql`

### Rust Files
- [ ] `fraiseql_rs/src/mutation/mod.rs`
- [ ] `fraiseql_rs/src/lib.rs`

### Python Files
- [ ] `src/fraiseql/mutations/entity_flattener.py`
- [ ] `src/fraiseql/mutations/rust_executor.py`

### Documentation Files
- [ ] `docs/mutations/status-strings.md`
- [ ] `docs/features/sql-function-return-format.md`
- [ ] `docs/features/mutation-result-reference.md`
- [ ] `docs/features/graphql-cascade.md`
- [ ] `CHANGELOG.md`

### Test Files
- [ ] `tests/fixtures/cascade/conftest.py`
- [ ] `tests/test_mutations/test_status_taxonomy.py`
- [ ] `tests/integration/graphql/mutations/test_unified_camel_case.py`

### Files That Should NOT Change
- [ ] No changes to `src/fraiseql/gql/` (GraphQL schema builders)
- [ ] No changes to core business logic
- [ ] No changes to API surface (only internal naming)

---

## Success Metrics

### Technical Metrics
- [ ] Zero `mutation_result_v2` references in codebase
- [ ] 100% test pass rate
- [ ] No type checking errors
- [ ] No linting errors
- [ ] Rust builds without warnings

### Code Quality Metrics
- [ ] All documentation updated
- [ ] CHANGELOG entry clear and complete
- [ ] Git history clean with logical commits
- [ ] No commented-out code left behind

### Confidence Metrics
- [ ] Rename completed in < 2 days
- [ ] No rollbacks needed
- [ ] Team confident in new naming

---

## Timeline Estimate

### Optimistic (1 day)
- Phase 1: PostgreSQL - 2 hours
- Phase 2: Rust - 1 hour
- Phase 3: Python - 1 hour
- Phase 4: Documentation - 2 hours
- Phase 5: Tests - 1 hour
- Phase 6: Verification - 1 hour
- **Total**: 8 hours (1 working day)

### Realistic (1.5 days)
- Add 50% buffer for unexpected issues
- **Total**: 12 hours (1.5 working days)

### Pessimistic (2 days)
- Account for test failures, build issues, documentation clarity
- **Total**: 16 hours (2 working days)

---

## Open Questions

### Resolved
- ✅ **Q**: Does `mutation_response` conflict with other libraries?
  **A**: No - Hasura uses per-table namespacing (`article_mutation_response`), no conflict

- ✅ **Q**: Should we version the migration file?
  **A**: No - keep the same number (005), just change the filename

### Pending
- ⚠️ **Q**: Are there any generated files we need to update?
  **A**: Need to check for codegen outputs, protobuf definitions, etc.

- ⚠️ **Q**: Do we need to update any CI/CD configuration?
  **A**: Likely not, but verify no hardcoded strings in CI scripts

---

## Related Work

### Before This Rename
- Status taxonomy implementation (completed)
- Cascade tracking implementation (completed)
- Entity flattener refactor (completed)

### After This Rename
- Continue with cascade mandatory tracking plan (if desired)
- Or proceed with other feature work

### Blocked By This Rename
- None - this is purely internal cleanup

---

**Plan Status**: ✅ Ready for Implementation
**Next Step**: Execute Phase 0 (Pre-Implementation Checklist)
**Estimated Start Date**: At team's discretion
**Estimated Completion**: 1-2 days after start
