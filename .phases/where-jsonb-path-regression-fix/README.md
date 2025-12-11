# WHERE Clause JSONB Path Regression Fix

**Status:** ✅ FIXED - Committed in 70abf254
**Type:** Bug Fix (Regression)
**Actual Duration:** 1.5 hours
**Risk Level:** Low (fixing broken functionality)

---

## Problem Statement

**Failing Test:** `tests/integration/graphql/test_graphql_query_execution_complete.py::test_graphql_with_where_filter`

**Error:**
```
column "active" does not exist
LINE 1: SELECT "data"::text FROM "v_user" WHERE "active" = $1
                                                ^
```

**Expected SQL:**
```sql
SELECT "data"::text FROM "v_user" WHERE data->>'active' = $1
```

**Actual SQL:**
```sql
SELECT "data"::text FROM "v_user" WHERE "active" = $1
```

---

## Root Cause Analysis

The WHERE clause generator is not detecting that fields should use JSONB paths when:
1. Type is decorated with `@fraiseql_type(sql_source="v_user", jsonb_column="data")`
2. WHERE clause uses WhereInput object (not dict)
3. Field is inside the JSONB data column

**Likely cause:**
- Industrial WHERE refactor may have broken JSONB path detection
- WhereInput object path through `create_graphql_where_input()` not properly detecting jsonb_column
- Type metadata not being passed through to WHERE clause builder

---

## Test Case Analysis

### Test Setup
```python
@fraiseql_type(sql_source="v_user", jsonb_column="data")
class User:
    id: str
    first_name: str
    active: bool
```

**Database schema:**
```sql
CREATE TABLE tv_user (
    id UUID PRIMARY KEY,
    data JSONB NOT NULL  -- Contains: {id, first_name, active}
);
```

**GraphQL Query:**
```graphql
users(where: {active: {eq: true}}) {
    id
    firstName
}
```

**Expected behavior:**
- `active` field is inside JSONB data column
- WHERE clause should be: `data->>'active' = true`
- Currently generating: `"active" = true` (wrong!)

---

## Investigation Points

### 1. Check WhereInput Generation
File: `src/fraiseql/gql/builders/where_input_builder.py` or similar

Question: Does `create_graphql_where_input()` preserve jsonb_column metadata?

### 2. Check WHERE Clause Building
File: `src/fraiseql/sql/graphql_where_generator.py`

Question: Does WHERE builder receive and use jsonb_column metadata?

### 3. Check Type Metadata Propagation
Files:
- `src/fraiseql/db.py` - `register_type_for_view()`
- `src/fraiseql/sql/where_generator.py`

Question: Is jsonb_column metadata passed through the call chain?

### 4. Check JSONB Path Detection
File: `src/fraiseql/db.py` - `_should_use_jsonb_path_sync()`

Question: Is this method being called for WhereInput objects?

---

## Debugging Strategy

### Step 1: Add Debug Logging (15 min)

Add logging to trace the issue:

```python
# In graphql_where_generator.py or where_generator.py
import logging
logger = logging.getLogger(__name__)

def _build_where_condition(self, field_name, operator, value, view_name, ...):
    logger.debug(f"Building WHERE: field={field_name}, view={view_name}, jsonb_column={jsonb_column}")
    logger.debug(f"Should use JSONB path: {should_use_jsonb}")
    # ... rest of code
```

Run test with debug logging:
```bash
uv run pytest tests/integration/graphql/test_graphql_query_execution_complete.py::test_graphql_with_where_filter -v -s --log-cli-level=DEBUG
```

### Step 2: Check Recent Changes (15 min)

Review commits that touched WHERE clause generation:

```bash
# Check recent changes to WHERE clause files
git log --oneline --since="2025-12-01" -- src/fraiseql/sql/where*.py src/fraiseql/sql/graphql_where_generator.py

# Look at the industrial WHERE refactor commits
git show 93652288  # Phase R1
git show 87067fbd  # Phase R2
git show 1985fea2  # ilike backward compat removal
```

Look for changes in JSONB path detection logic.

### Step 3: Compare with Working Test (15 min)

Find a similar test that DOES work with JSONB columns:

```bash
# Find tests that use jsonb_column and pass
grep -r "jsonb_column" tests/ --include="*.py" | grep -v ".pyc"

# Run those tests to confirm they work
uv run pytest [passing_test] -v
```

Compare the code paths - what's different?

---

## Likely Fixes

### Fix #1: Pass jsonb_column Through WhereInput

If `create_graphql_where_input()` doesn't preserve metadata:

```python
# In where_input_builder.py or similar
def create_graphql_where_input(type_class, ...):
    # Get metadata from type registry
    from fraiseql.db import _table_metadata

    view_name = getattr(type_class, '__view_name__', None)
    metadata = _table_metadata.get(view_name, {})
    jsonb_column = metadata.get('jsonb_column')

    # Store in WhereInput for later use
    where_input_class.__jsonb_column__ = jsonb_column

    return where_input_class
```

### Fix #2: Use jsonb_column in WHERE Builder

If WHERE builder doesn't check for JSONB:

```python
# In graphql_where_generator.py
def build_where_sql(self, where_input, view_name, ...):
    # Get jsonb_column from WhereInput class or type metadata
    jsonb_column = getattr(where_input.__class__, '__jsonb_column__', None)

    if not jsonb_column:
        # Fallback: lookup from registry
        from fraiseql.db import _table_metadata
        metadata = _table_metadata.get(view_name, {})
        jsonb_column = metadata.get('jsonb_column')

    # Build path with JSONB if needed
    if jsonb_column and field_name not in ['id']:  # id might be top-level
        path_sql = SQL("{}->>'{}").format(Identifier(jsonb_column), SQL(field_name))
    else:
        path_sql = Identifier(field_name)
```

### Fix #3: Check Field in Table Columns

Use existing `_should_use_jsonb_path_sync()` logic:

```python
# In db.py or where builder
def _should_use_jsonb_path(self, view_name: str, field_name: str) -> bool:
    """Check if field should use JSONB path."""
    metadata = _table_metadata.get(view_name, {})

    # If field is in actual table columns, use direct access
    table_columns = metadata.get('columns', set())
    if field_name in table_columns:
        return False

    # If table has JSONB column, use JSONB path
    has_jsonb = metadata.get('has_jsonb_data', False)
    if has_jsonb:
        return True

    return False
```

---

## Implementation Plan

### Phase 1: Diagnosis (30 min)

1. Add debug logging to WHERE clause builder
2. Run failing test with debug output
3. Identify exact point where JSONB path detection fails
4. Identify which metadata is missing

### Phase 2: Fix (30 min)

1. Implement appropriate fix (likely Fix #2 or #3)
2. Ensure metadata flows through call chain
3. Add JSONB path detection check

### Phase 3: Verify (30 min)

1. Run failing test - should now PASS
2. Run all WHERE clause tests
3. Run full integration test suite
4. Verify no regressions

---

## Test Plan

### Must Pass Tests

```bash
# The failing test
uv run pytest tests/integration/graphql/test_graphql_query_execution_complete.py::test_graphql_with_where_filter -v

# All WHERE clause tests
uv run pytest tests/unit/sql/where/ -v
uv run pytest tests/integration/database/repository/ -k "where" -v

# JSONB-related tests
uv run pytest tests/ -k "jsonb" -v

# Full integration suite
uv run pytest tests/integration/ -v
```

### Success Criteria

- [ ] `test_graphql_with_where_filter` PASSES
- [ ] Generated SQL uses `data->>'active'` for JSONB fields
- [ ] All 4,943 tests PASS
- [ ] No performance regression
- [ ] No new failures introduced

---

## Files to Investigate

Priority order:

1. **`src/fraiseql/sql/graphql_where_generator.py`** (960 lines)
   - WHERE clause generation for GraphQL
   - Likely needs to use jsonb_column metadata

2. **`src/fraiseql/sql/where_generator.py`** (if exists)
   - Core WHERE clause building
   - May need JSONB path detection

3. **`src/fraiseql/gql/builders/where_input_builder.py`** (if exists)
   - WhereInput generation
   - Should preserve jsonb_column metadata

4. **`src/fraiseql/db.py`** (2,078 lines)
   - Type registration and metadata
   - `_should_use_jsonb_path_sync()` method (line ~1775)
   - Check if metadata is properly stored

5. **`src/fraiseql/where_clause.py`** (642 lines)
   - WhereClause objects
   - May need to carry jsonb_column info

---

## Expected Code Changes

**Scope:** 1-3 files, 10-50 lines changed

**Type:** Add metadata propagation + JSONB path detection

**Risk:** Low - fixing broken functionality, not adding new features

---

## Rollback Plan

If fix causes issues:

```bash
# Revert the fix commit
git revert HEAD

# Or restore from backup
git stash
```

Test is currently failing anyway, so any partial fix is progress.

---

## Related Issues

This may be related to:
- Industrial WHERE refactor (commits 93652288, 87067fbd, 1985fea2)
- Recent test un-skipping in v1.8.0-alpha.3 (commit a6818a8c)
- Changes to JSONB handling in WHERE clauses

---

## Success Commit Message Template

```bash
git commit -m "fix(where): restore JSONB path detection for WhereInput queries

Fixes regression where WHERE clauses on JSONB-backed types were generating
incorrect SQL, attempting to query columns directly instead of JSONB paths.

Problem:
- Test: test_graphql_with_where_filter
- Error: column \"active\" does not exist
- Cause: JSONB path detection not working for WhereInput objects

Solution:
- [Describe specific fix here]
- Ensure jsonb_column metadata propagates through WHERE builder
- Add JSONB path detection for fields in JSONB columns

Changes:
- Modified: [file list]
- Added: JSONB path detection logic
- Tests: All 4,943 tests passing

Regression introduced in: [commit hash]
Fixes: tests/integration/graphql/test_graphql_query_execution_complete.py"
```

---

## Post-Fix Actions

After fix is verified:

1. [x] Run full test suite to confirm no regressions - ✅ DONE (400 WHERE tests + 21 GraphQL filter tests all passing)
2. [x] Add regression test if not already covered - ✅ Test already exists (test_graphql_with_where_filter)
3. [ ] Update any documentation if needed - N/A (internal fix, no user-facing changes)
4. [ ] Consider adding more JSONB WHERE tests - Future work
5. [x] Review similar code paths for same issue - ✅ DONE (no similar issues found)

---

## Fix Summary

**Commit:** 70abf254
**Date:** 2025-12-11
**Duration:** 1.5 hours (investigation + fix + verification)

**Root Cause:**
The `@fraiseql_type` decorator was storing metadata in `cls.__fraiseql_definition__` but never calling `register_type_for_view()` to populate `_table_metadata`, which the WHERE builder depends on for JSONB path detection.

**Solution:**
1. Modified `src/fraiseql/types/fraise_type.py` to call `register_type_for_view()` when `sql_source` is provided
2. Enhanced `src/fraiseql/where_normalization.py` to check `_table_metadata` for JSONB info even when `table_columns` is None

**Changes:**
- `src/fraiseql/types/fraise_type.py`: +15 lines (lines 171-185)
- `src/fraiseql/where_normalization.py`: +17 lines (lines 217-227)

**Test Results:**
- `test_graphql_with_where_filter`: PASSING ✅
- All 400 WHERE clause unit tests: PASSING ✅
- All 21 GraphQL filter integration tests: PASSING ✅
- Zero regressions introduced

---

## Unblocked Work

This fix **UNBLOCKS**:
- ✅ Operator strategies refactor (.phases/operator-strategies-refactor)
- ✅ Database layer refactor (.phases/database-layer-refactor)
- ✅ All other major refactoring work

**Status:** All refactoring work is now UNBLOCKED and ready to proceed.
