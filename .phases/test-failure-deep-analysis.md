# FraiseQL Test Failure Deep Analysis

**Date**: December 12, 2025
**Analysis Type**: Comprehensive failure categorization
**Current Status**: 97 failed, 2 errors, 5169 passed (98.16% pass rate)

---

## Executive Summary

After completing Phase 1 (v1.8.1 updates) and Phase 1.5 (Schema Refresh API), we have **97 test failures** remaining. This deep analysis reveals that failures fall into **3 distinct categories** with different root causes and solutions.

### Key Findings

1. **Primary Issue** (85-90% of failures): `str()` on composed SQL objects returns `repr()` instead of rendered SQL
2. **Secondary Issue** (5-10% of failures): Integration tests with real database operations
3. **Tertiary Issue** (2-3 failures): Type extraction and performance tests

---

## Current Test Status

```
Total Tests:  5,318 (collected)
Passing:      5,169 (97.2%)
Failing:      97 (1.8%)
Errors:       2 (0.04%)
Skipped:      50 (WP-034 + others)
Pass Rate:    98.16%
```

**Target**: 100% (5,318 passing, 0 failing)

---

## Failure Categories

### Category 1: SQL Rendering Issue (PRIMARY)
**Count**: ~85 tests (87.6% of failures)
**Root Cause**: Tests use `str(composed_sql)` which returns Python `repr()` not SQL

#### Affected Test Files (by failure count)

| File | Failures | Pattern |
|------|----------|---------|
| `tests/core/test_production_fix_validation.py` | 10 | `str(result)` returns `'None'` |
| `tests/core/test_special_types_tier1_core.py` | 9 | Same pattern |
| `tests/integration/repository/test_specialized_types_repository_integration.py` | 8 | Repository SQL validation |
| `tests/regression/where_clause/test_sql_structure_validation.py` | 6 | SQL structure checks |
| `tests/regression/where_clause/test_precise_sql_validation.py` | 6 | Precise SQL validation |
| `tests/regression/where_clause/test_numeric_consistency_validation.py` | 6 | Numeric casting checks |
| `tests/regression/where_clause/test_complete_sql_validation.py` | 6 | Complete SQL validation |
| `tests/integration/repository/test_field_name_mapping_integration.py` | 6 | Field mapping SQL |
| `tests/integration/database/sql/test_issue_resolution_demonstration.py` | 6 | Issue resolution tests |
| `tests/regression/where_clause/test_industrial_where_clause_generation.py` | 5 | Industrial scenarios |
| `tests/core/test_all_special_types_fix.py` | 5 | Special types |
| Others | 12 | Various |

#### Example Failure

```python
# tests/regression/where_clause/test_sql_structure_validation.py:32
def test_numeric_casting_structure(self):
    strategy = registry.get_strategy("eq", int)
    result = strategy.build_sql(jsonb_path, "eq", 443, int)
    sql_str = str(result)  # ← PROBLEM: Returns 'None' instead of SQL

    # All assertions fail because sql_str is 'None'
    assert "::numeric" in sql_str  # ❌ FAILS: 'None' doesn't contain '::numeric'
    assert "data ->> 'port'" in sql_str  # ❌ FAILS
    assert "Literal(443)" in sql_str  # ❌ FAILS
```

**Actual Error Messages**:
```
AssertionError: Missing numeric casting for eq. Got: None
assert '::numeric' in 'None'
```

#### Solution: SQL Rendering Utility

Need to create a utility that properly renders composed SQL objects:

```python
# tests/helpers/sql_rendering.py (NEW)
from psycopg.sql import Composed

def render_sql_for_testing(sql_object) -> str:
    """Render psycopg.sql composed objects to valid SQL strings.

    Args:
        sql_object: Composed SQL object or SQL fragment

    Returns:
        Valid SQL string for assertion testing

    Example:
        >>> result = strategy.build_sql(path, "eq", 443, int)
        >>> sql = render_sql_for_testing(result)
        >>> assert "::numeric" in sql  # Now works!
    """
    if isinstance(sql_object, Composed):
        # Render with proper connection context
        return sql_object.as_string(connection)
    elif hasattr(sql_object, 'as_string'):
        return sql_object.as_string(connection)
    else:
        return str(sql_object)
```

**Migration Pattern**:
```python
# BEFORE (broken):
sql_str = str(result)

# AFTER (fixed):
from tests.helpers.sql_rendering import render_sql_for_testing
sql_str = render_sql_for_testing(result)
```

---

### Category 2: Integration Database Tests
**Count**: ~8-10 tests (8-10% of failures)
**Root Cause**: Tests interact with real database and have various issues

#### Affected Files
- `tests/integration/repository/test_specialized_types_repository_integration.py` (8)
- `tests/integration/repository/test_field_name_mapping_integration.py` (6)
- `tests/integration/repository/test_repository_find_where_processing.py` (4)

#### Issues Observed
1. Some tests may be using `str()` pattern (Category 1 overlap)
2. Some may have fixture/database setup issues
3. Some may have actual SQL generation bugs

**Strategy**: Fix Category 1 first, then re-run to see what remains.

---

### Category 3: Other Specific Issues
**Count**: 3-5 tests (3-5% of failures)

#### 3.1 Field Type Extraction (1 failure)
- `tests/unit/graphql/test_field_type_extraction.py::TestNetworkFieldTypeIntegration::test_field_type_enables_proper_sql_casting`
- **Issue**: Network type field extraction logic
- **Requires**: Specific investigation

#### 3.2 Performance Tests (2 errors)
- `tests/performance/test_rustresponsebytes_performance.py` (2 errors)
- **Issue**: Performance test configuration or Rust binding issues
- **Low Priority**: Performance tests can be marked or fixed last

---

## Phase 2 Strategy: SQL Rendering Fix

### Implementation Plan (16-20 hours)

#### Step 1: Create SQL Rendering Utility (2-3 hours)

**File**: `tests/helpers/sql_rendering.py`

**Requirements**:
1. Handle `psycopg.sql.Composed` objects
2. Handle `psycopg.sql.SQL` objects
3. Handle `psycopg.sql.Literal` objects
4. Provide connection context for rendering
5. Handle nested composition
6. Return valid SQL strings for assertions

**Tests**: Unit tests for the utility itself

#### Step 2: Migrate WHERE Clause Tests (6-8 hours)

**Priority Order**:
1. **Regression tests** (most focused, ~30 tests)
   - `tests/regression/where_clause/test_sql_structure_validation.py` (6)
   - `tests/regression/where_clause/test_precise_sql_validation.py` (6)
   - `tests/regression/where_clause/test_numeric_consistency_validation.py` (6)
   - `tests/regression/where_clause/test_complete_sql_validation.py` (6)
   - `tests/regression/where_clause/test_industrial_where_clause_generation.py` (5)

2. **Core tests** (fundamental, ~24 tests)
   - `tests/core/test_production_fix_validation.py` (10)
   - `tests/core/test_special_types_tier1_core.py` (9)
   - `tests/core/test_all_special_types_fix.py` (5)

3. **Integration tests** (may have additional issues, ~18 tests)
   - Wait until after Steps 1-2 to see which issues remain

**Delegation to Local AI**:
- Use Ministral-3-8B for bulk search & replace
- Pattern: `str(result)` → `render_sql_for_testing(result)`
- Claude reviews and fixes edge cases

#### Step 3: Verify and Identify Remaining Issues (2-3 hours)

After SQL rendering fix:
1. Run full test suite
2. Categorize any remaining failures
3. Determine if they're:
   - Missed SQL rendering locations
   - Actual SQL generation bugs
   - Integration test fixtures
   - Other issues

#### Step 4: Document and Plan Phase 3 (1-2 hours)

- Document SQL rendering utility usage
- Create migration guide for future tests
- Plan Phase 3 based on remaining issues

---

## Phase 3 Preview: Remaining Issues

**Estimated Remaining After Phase 2**: 5-15 tests

Likely categories:
1. Integration tests with database issues
2. SQL generation bugs revealed by proper rendering
3. Field type extraction issues
4. Network type handling edge cases

**Effort**: 8-12 hours (depends on Phase 2 results)

---

## Local AI Delegation Strategy

### Tasks Suitable for Ministral-3-8B

✅ **Excellent** (95%+ success rate):
1. **Search & Replace**: `str(result)` → `render_sql_for_testing(result)`
2. **Import Addition**: Add `from tests.helpers.sql_rendering import render_sql_for_testing`
3. **Pattern Application**: Apply fix to 10-20 files consistently

**Prompt Template**:
```
Task: Update SQL rendering in test file

Current pattern:
```python
sql_str = str(result)
```

New pattern:
```python
from tests.helpers.sql_rendering import render_sql_for_testing
sql_str = render_sql_for_testing(result)
```

Apply this change to all occurrences in the file.
Show only the modified lines.
```

❌ **Not Suitable**:
1. Creating the SQL rendering utility itself (requires understanding psycopg internals)
2. Debugging edge cases where rendering fails
3. Understanding why specific tests fail

### Workflow

1. **Claude**: Create SQL rendering utility + tests (2-3 hours)
2. **Claude**: Identify pattern in 2-3 test files, create template (30 min)
3. **Local AI**: Apply pattern to 20-30 test files (batch 1) (30 min)
4. **Claude**: Review batch, run tests, fix issues (1 hour)
5. **Local AI**: Apply pattern to next 20-30 files (batch 2) (30 min)
6. **Claude**: Review, test, fix (1 hour)
7. Repeat until complete

**Estimated time savings**: 30-40% (vs Claude doing all manually)

---

## Risk Assessment

### Phase 2 Risks

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| SQL rendering utility incomplete | MEDIUM | HIGH | Comprehensive unit tests, incremental rollout |
| Bulk migration introduces errors | MEDIUM | MEDIUM | Batch testing (10-20 files at a time) |
| Tests reveal actual SQL bugs | HIGH | MEDIUM | Budget Phase 3 time appropriately |
| Connection context issues | LOW | MEDIUM | Research psycopg rendering first |

### Success Factors

✅ **High Confidence**:
- Root cause clearly identified (`str()` vs proper rendering)
- Solution is straightforward (utility function)
- Pattern is repetitive (good for automation)
- Can verify incrementally (batch testing)

⚠️ **Medium Confidence**:
- Some integration tests may have additional issues
- SQL rendering utility needs to handle all edge cases
- Connection context requirement may be tricky

---

## Detailed File Analysis

### High-Priority Files (Should Fix First)

#### 1. `tests/regression/where_clause/test_sql_structure_validation.py` (6 failures)
**Pattern**: All use `str(result)` on lines 32, 62, 92, etc.
**Fix**: Replace with `render_sql_for_testing(result)`
**Expected Outcome**: 6/6 tests pass after fix

#### 2. `tests/core/test_production_fix_validation.py` (10 failures)
**Pattern**: Similar `str()` usage in production fix validation
**Fix**: Same pattern replacement
**Expected Outcome**: 8-10/10 tests pass (may reveal 0-2 actual bugs)

#### 3. `tests/core/test_special_types_tier1_core.py` (9 failures)
**Pattern**: Special type SQL validation
**Fix**: SQL rendering utility
**Expected Outcome**: 7-9/9 tests pass

### Medium-Priority Files (Fix After High-Priority)

Integration tests that may have mixed issues - wait until after SQL rendering fix to assess.

---

## Validation Commands

### Check Current Failures
```bash
# Count by category
uv run pytest tests/ --tb=no -q 2>&1 | grep "FAILED" | \
  awk -F'::' '{print $1}' | sort | uniq -c | sort -rn

# Get specific failure details
uv run pytest tests/regression/where_clause/test_sql_structure_validation.py \
  -v --tb=short

# Run just regression tests
uv run pytest tests/regression/ -v --tb=line | grep "FAILED\|AssertionError"
```

### After Phase 2 Implementation
```bash
# Verify SQL rendering utility
uv run pytest tests/helpers/test_sql_rendering.py -v

# Test batch of migrated files
uv run pytest tests/regression/where_clause/ -v

# Check progress
uv run pytest tests/ --tb=no -q | tail -1
```

---

## Next Immediate Actions

1. **Create SQL rendering utility** (2-3 hours)
   - Research psycopg SQL rendering with connection context
   - Implement utility in `tests/helpers/sql_rendering.py`
   - Write unit tests for utility
   - Verify with 2-3 sample test files

2. **Migrate high-priority regression tests** (3-4 hours)
   - Start with `test_sql_structure_validation.py`
   - Use local AI for pattern application
   - Verify each file passes after migration

3. **Continue migration in batches** (8-10 hours)
   - Batch 1: Remaining regression tests
   - Batch 2: Core tests
   - Batch 3: Integration tests (carefully)

4. **Assess remaining failures** (2-3 hours)
   - Categorize what didn't get fixed
   - Plan Phase 3 accordingly

---

## Success Criteria

### Phase 2 Complete When:
- [ ] SQL rendering utility created and tested
- [ ] 85-90 tests migrated to use utility
- [ ] Regression tests passing (30+ tests)
- [ ] Core tests passing (20+ tests)
- [ ] Remaining failures < 15 tests
- [ ] All migration documented

### Phase 3 Plan Ready When:
- [ ] Remaining failures categorized
- [ ] Root causes identified
- [ ] Effort estimated
- [ ] Approach documented

---

## Appendix A: Sample Failures

### Example 1: SQL Structure Validation
```
FAILED tests/regression/where_clause/test_sql_structure_validation.py::TestSQLStructureValidation::test_numeric_casting_structure
AssertionError: Missing numeric casting for eq. Got: None
assert '::numeric' in 'None'
```

**Root Cause**: Line 32 `sql_str = str(result)` returns `'None'`
**Fix**: `sql_str = render_sql_for_testing(result)`

### Example 2: Boolean Text Comparison
```
FAILED tests/regression/where_clause/test_sql_structure_validation.py::TestSQLStructureValidation::test_boolean_text_comparison_structure
AssertionError: Missing JSONB field extraction. Got: None
assert "data ->> 'is_active'" in 'None'
```

**Root Cause**: Same pattern, line 62
**Fix**: Same solution

### Example 3: Type Error in Fallback
```
FAILED tests/regression/where_clause/test_sql_structure_validation.py::TestSQLStructureValidation::test_hostname_text_structure
TypeError: unhashable type: 'SQL'
/home/lionel/code/fraiseql/src/fraiseql/sql/operators/fallback/comparison_operators.py:54
```

**Root Cause**: May be actual bug in fallback operator (investigate after SQL rendering fix)
**Priority**: After Phase 2

---

## Appendix B: Test Count Verification

```bash
# Total tests collected
uv run pytest tests/ --co -q 2>&1 | grep "tests collected"
# Output: 5318 tests collected

# Current run results
uv run pytest tests/ --tb=no -q 2>&1 | tail -1
# Output: 97 failed, 5169 passed, 50 skipped, 10 warnings, 2 errors in 94.63s

# Math check
# 97 + 5169 + 50 + 2 = 5318 ✓
```

---

**Recommendation**: Proceed with Phase 2 - SQL Rendering Infrastructure
**Confidence Level**: HIGH (>90% of failures have clear root cause and solution)
**Estimated Impact**: 85-90 tests fixed (87-93% of remaining failures)
