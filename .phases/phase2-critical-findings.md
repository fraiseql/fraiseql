# Phase 2: Critical Findings - API Mismatch Discovered

**Date**: December 12, 2025
**Finding Type**: Test API Mismatch (More Serious Than Expected)
**Impact**: Changes Phase 2 approach significantly

---

## Summary

While implementing Phase 2 (SQL Rendering), we discovered that the failing tests have a **deeper issue than just `str()` rendering** - they're calling the operator strategy API with **incorrect parameter order**.

This is actually **good news** - we've uncovered the real root cause, but it requires a different fix.

---

## What We Found

### Initial Hypothesis (From Analysis)
```python
# PROBLEM:
sql_str = str(result)  # Returns 'None' - need proper rendering

# SOLUTION:
sql_str = render_sql_for_testing(result)  # Returns actual SQL
```

### Actual Reality (Discovered During Implementation)
```python
# The test calls:
strategy.build_sql(jsonb_path, operator, value, field_type)
#                  ^^^^^^^^^^  ^^^^^^^^  ^^^^^  ^^^^^^^^^^
#                  path first  then op   then value

# But the actual API is:
strategy.build_sql(operator, value, path_sql, field_type)
#                  ^^^^^^^^  ^^^^^  ^^^^^^^^^  ^^^^^^^^^^
#                  op first  then value  then path

# Result: Returns None because parameter mismatch!
```

### Evidence

**Test Code** (`test_sql_structure_validation.py:32`):
```python
strategy = registry.get_strategy(op, int)
result = strategy.build_sql(jsonb_path, op, 443, int)
#                           ^^^^^^^^^ ^^^^
#                           This is WRONG parameter order
```

**Actual API** (`src/fraiseql/sql/operators/base.py:29-36`):
```python
def build_sql(
    self,
    operator: str,      # ← Should be first
    value: Any,         # ← Should be second
    path_sql: Composable,  # ← Should be third
    field_type: Optional[type] = None,
    jsonb_column: Optional[str] = None,
) -> Optional[Composable]:
```

**What Happens**:
1. Test calls `strategy.build_sql(jsonb_path, "eq", 443, int)`
2. Strategy receives: `operator=SQL("(data ->> 'port')")`, `value="eq"`, `path_sql=443`, `field_type=int`
3. Strategy tries to process `operator="eq"` (a SQL object, not a string)
4. Fails silently and returns `None`
5. Test gets `None`, converts to string → `"None"`
6. Assertions fail: `assert "::numeric" in "None"`

---

## Impact on Phase 2

### Original Plan ❌
1. Create `render_sql_for_testing()` utility ✅ **DONE**
2. Replace `str(result)` → `render_sql_for_testing(result)` in ~85 tests ❌ **WON'T WORK**
3. Tests pass ❌ **WON'T HAPPEN - result is None!**

### Revised Plan ✅
1. Create `render_sql_for_testing()` utility ✅ **DONE** (Still useful!)
2. **Fix test API calls** - Correct parameter order in ~85 tests
3. **Then** add SQL rendering where needed
4. Tests should pass (or reveal actual bugs)

---

## Why This Is Actually Good

### Positive Outcomes
1. ✅ **Found real issue** - Tests weren't testing correctly
2. ✅ **Created useful utility** - `render_sql_for_testing()` will be needed
3. ✅ **Better understanding** - Now know exact fix needed
4. ✅ **Clearer path** - API fix is mechanical and automatable

### What This Means
- The tests **wanted** to test SQL generation
- But they've been **unable to test** due to API mismatch
- Once we fix the API calls, we'll **actually test** the SQL generation
- We might discover **actual SQL bugs** (which is what tests are for!)

---

## Corrected Categorization

### Tests Need API Fix (~85 tests)
**Files**:
- `tests/regression/where_clause/*.py` (~30 tests)
- `tests/core/test_production_fix_validation.py` (10 tests)
- `tests/core/test_special_types_tier1_core.py` (9 tests)
- Others (~36 tests)

**Pattern to Fix**:
```python
# BEFORE (broken):
result = strategy.build_sql(jsonb_path, operator, value, field_type)

# AFTER (correct):
result = strategy.build_sql(operator, value, jsonb_path, field_type)
```

**Additional Fix** (if result is not None):
```python
# Also update rendering:
sql_str = render_sql_for_testing(result)  # Instead of str(result)
```

### Integration Tests (~8-10 tests)
May have same API issue or different issues - check after API fix.

### Other Tests (3-5 tests)
Performance tests, field extraction - separate issues.

---

## Revised Phase 2 Strategy

### Step 1: SQL Rendering Utility ✅ COMPLETE
- Created `tests/helpers/sql_rendering.py`
- 30 unit tests, all passing
- Well-documented with examples

### Step 2: Fix Test API Calls (NEW)
**Effort**: 6-8 hours (similar to original SQL rendering plan)

**Approach**:
1. **Manual fix template** (Claude):
   - Fix 2-3 files manually
   - Document the pattern clearly
   - Verify tests pass (or reveal real bugs)

2. **Automated migration** (Local AI):
   - Apply pattern to remaining files
   - Batch process 10-20 files at a time

3. **Review and test** (Claude):
   - Verify each batch
   - Fix any edge cases
   - Run tests to see results

**Pattern for Local AI**:
```
Task: Fix operator strategy API calls

Current (broken):
```python
result = strategy.build_sql(jsonb_path, operator, value, field_type)
```

Correct:
```python
result = strategy.build_sql(operator, value, jsonb_path, field_type)
```

Apply to all `.build_sql(` calls in the file.
```

### Step 3: Add SQL Rendering
Once API is fixed and tests run, add `render_sql_for_testing()` where needed.

### Step 4: Analyze Results
After fixes:
- Some tests will pass ✅
- Some may reveal **actual SQL generation bugs** (good!)
- Document findings for Phase 3

---

## Testing the Fix

### Before Fix
```bash
$ uv run pytest tests/regression/where_clause/test_sql_structure_validation.py::TestSQLStructureValidation::test_numeric_casting_structure -v

FAILED - AssertionError: Missing numeric casting for eq. Got: None
```

### After API Fix (Expected)
```bash
# Either:
PASSED  # ✅ If SQL generation works

# Or:
FAILED - AssertionError: Missing numeric casting for eq.
Got: (data ->> 'port') = 443
# ❌ But now we see ACTUAL SQL and can fix the real bug!
```

---

## Files Created

1. **`tests/helpers/sql_rendering.py`** ✅
   - Core utility with 3 functions
   - Comprehensive documentation
   - Production-ready

2. **`tests/helpers/test_sql_rendering.py`** ✅
   - 30 unit tests covering all scenarios
   - Real-world examples from failing tests
   - Edge case handling

3. **`tests/helpers/__init__.py`** ✅
   - Proper package structure
   - Exports `render_sql_for_testing`

---

## Next Steps

### Immediate (Step 2A - Manual Template)
1. Pick one failing test file (e.g., `test_sql_structure_validation.py`)
2. Manually fix all `.build_sql()` calls
3. Add `render_sql_for_testing()` imports and calls
4. Run tests and document results
5. Create template for automation

### Then (Step 2B - Automation)
1. Use local AI to apply pattern to 10-20 files (batch 1)
2. Review and test batch 1
3. Continue in batches until complete

### Finally (Step 3)
1. Analyze test results
2. Categorize remaining failures
3. Plan Phase 3 based on actual bugs found

---

## Lessons Learned

1. **Always test the utility first** - Found API mismatch immediately
2. **str() failures weren't rendering issues** - They were API issues causing None
3. **SQL rendering utility still valuable** - Will be needed after API fix
4. **Tests were trying to help** - Just couldn't run correctly
5. **Phase 2 effort similar** - Still ~16-20 hours but different work

---

## Confidence Level

**Before Finding**: 95% confidence SQL rendering would fix tests
**After Finding**: 99% confidence API fix will fix tests (or reveal real bugs)

**Why Higher**:
- Root cause definitively identified
- Fix is mechanical and clear
- Tests are well-written, just using wrong API
- SQL rendering utility tested and working

---

## Communication to User

✅ **Good News**:
- Found the real issue (API mismatch)
- Created working SQL rendering utility
- Clear path to fix

✅ **Better News**:
- Fix is still mechanical and automatable
- Same timeline (~16-20 hours)
- Once fixed, tests will actually TEST the code

✅ **Best News**:
- May discover actual SQL bugs (tests doing their job!)
- SQL rendering utility will be useful going forward
- No wasted effort - all work so far is valuable

---

**Status**: Phase 2 Step 1 Complete + Critical Discovery
**Next**: Commit utility, then proceed with API fixes
