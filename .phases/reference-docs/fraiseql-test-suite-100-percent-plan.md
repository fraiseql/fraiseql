# FraiseQL Test Suite: 100% Clean Plan

**Date**: December 12, 2025
**Current State**: 5,160/5,315 passing (96.9%)
**Target**: 5,315/5,315 passing (100%)
**Total Failures to Fix**: 214 tests + 2 errors + 10 warnings

---

## Executive Summary

Based on detailed analysis of test failures and the codebase state, I've identified that the test suite failures fall into **3 distinct categories** with different root causes and remediation strategies:

### Category A: v1.8.1 Semantics Mismatch (Quick Fix)
- **Count**: 16 tests
- **Effort**: 2-4 hours
- **Risk**: LOW
- **Strategy**: Update test expectations to match v1.8.1 field auto-injection semantics

### Category B: SQL Generation Bugs (Complex Investigation)
- **Count**: ~150 tests
- **Effort**: 40-60 hours
- **Risk**: HIGH
- **Strategy**: Fix core SQL generation issues with psycopg3 Composed objects

### Category C: Configuration & Cleanup (Low Priority)
- **Count**: 92 skipped + 10 warnings + 2 errors
- **Effort**: 4-6 hours
- **Risk**: NONE
- **Strategy**: CI configuration and dependency updates

---

## Critical Finding: Psycopg3 Composed Object Issue

**Root Cause Identified**: The vast majority of SQL generation failures (Categories B) are caused by a single systemic issue:

**Tests are asserting against `str(composed_object)` but psycopg3 `Composed` objects contain unhashable types (like `SQL`) and render as `repr()` instead of valid SQL**.

### Example Failure Pattern:

```python
# Test expectation (looking for valid SQL string)
assert "::daterange" in sql_str

# Actual value (repr of Composed object, NOT SQL)
"Composed([SQL('('), Literal(...), SQL(')::'), SQL('daterange')])"
```

**The problem**: Tests need to call `.as_string(connection)` or use a mock connection to render `Composed` objects to actual SQL strings.

**Impact**: ~150 tests across:
- `tests/regression/where_clause/` (~100 tests)
- `tests/core/test_special_types_tier1_core.py` (~20 tests)
- `tests/core/test_jsonb_network_casting_fix.py` (~10 tests)
- `tests/integration/repository/` (~20 tests)

---

## Phase-Based Remediation Plan

### Phase 1: Quick Wins - v1.8.1 Test Updates (Week 1)

**Objective**: Fix all tests expecting v1.8.0 field semantics

**Files to Update**:
1. `tests/unit/mutations/test_auto_populate_schema.py` (4 tests)
2. `tests/unit/decorators/test_decorators.py` (2 tests)
3. `tests/integration/graphql/mutations/test_native_error_arrays.py` (~10 tests)

**Changes Required**:

#### 1.1 Success Types (Remove `errors` field expectations)

**Current (Wrong)**:
```python
@success
class CreateMachineSuccess:
    machine: Machine

# Test expects:
assert "errors" in gql_fields  # ❌ WRONG - removed in v1.8.1
```

**Fixed (Correct)**:
```python
@success
class CreateMachineSuccess:
    machine: Machine

# Test should expect:
assert "errors" not in gql_fields  # ✅ CORRECT
assert "status" in gql_fields
assert "message" in gql_fields
assert "updated_fields" in gql_fields
assert "id" in gql_fields  # if entity field present
```

#### 1.2 Error Types (Remove `updated_fields` and `id` expectations)

**Current (Wrong)**:
```python
@failure
class CreateMachineError:
    pass

# Test expects:
assert "updated_fields" in gql_fields  # ❌ WRONG
assert "id" in gql_fields              # ❌ WRONG
```

**Fixed (Correct)**:
```python
@failure
class CreateMachineError:
    pass

# Test should expect:
assert "code" in gql_fields            # ✅ Auto-injected in v1.8.1
assert "status" in gql_fields
assert "message" in gql_fields
assert "errors" in gql_fields
assert "updated_fields" not in gql_fields  # ✅ Removed
assert "id" not in gql_fields              # ✅ Removed
```

**Verification**:
```bash
uv run pytest tests/unit/mutations/test_auto_populate_schema.py -v
uv run pytest tests/unit/decorators/test_decorators.py -v
uv run pytest tests/integration/graphql/mutations/test_native_error_arrays.py -v
```

**Acceptance Criteria**:
- [ ] All 16 tests pass
- [ ] No manual `code` fields in Error types
- [ ] Success types don't expect `errors` field
- [ ] Error types don't expect `updated_fields` or `id` fields

---

### Phase 2: SQL Rendering Infrastructure Fix (Week 2)

**Objective**: Fix the core issue preventing SQL validation tests from working

**Root Cause**: Tests are calling `str()` on psycopg3 `Composed` objects, which returns `repr()` instead of valid SQL.

**Solution**: Create a test utility to properly render `Composed` objects to SQL strings.

#### 2.1 Create SQL Rendering Utility

**File**: `tests/helpers/sql_rendering.py` (NEW)

```python
"""Utilities for rendering psycopg3 Composed objects to SQL strings in tests."""

from psycopg import sql
from psycopg.sql import Composed
from unittest.mock import Mock


def render_sql_for_testing(composed_obj: Composed | sql.SQL | sql.Literal) -> str:
    """
    Render a psycopg3 Composed/SQL object to a valid SQL string for testing.

    Uses a mock connection to avoid requiring a real database connection.

    Args:
        composed_obj: A psycopg3 SQL composition object

    Returns:
        Valid SQL string suitable for assertion testing

    Example:
        >>> from psycopg.sql import SQL, Literal, Composed
        >>> obj = Composed([SQL("SELECT * FROM users WHERE id = "), Literal(123)])
        >>> render_sql_for_testing(obj)
        "SELECT * FROM users WHERE id = 123"
    """
    # Create a mock connection with minimal required attributes
    mock_conn = Mock()
    mock_conn.encoding = 'utf-8'
    mock_conn.info.encoding = 'utf-8'

    # Render the Composed object to bytes, then decode
    try:
        if isinstance(composed_obj, (Composed, sql.SQL, sql.Literal, sql.Identifier)):
            rendered_bytes = composed_obj.as_bytes(mock_conn)
            return rendered_bytes.decode('utf-8')
        else:
            # Already a string
            return str(composed_obj)
    except Exception as e:
        # Fallback: return repr for debugging
        return f"<Failed to render: {repr(composed_obj)}>"


def assert_sql_contains(composed_obj: Composed, expected_fragment: str, message: str = ""):
    """
    Assert that rendered SQL contains the expected fragment.

    Args:
        composed_obj: psycopg3 Composed object
        expected_fragment: SQL fragment to search for (e.g., "::inet", "WHERE")
        message: Custom assertion message

    Raises:
        AssertionError: If fragment not found in rendered SQL
    """
    rendered = render_sql_for_testing(composed_obj)
    assert expected_fragment in rendered, (
        message or f"Expected '{expected_fragment}' in SQL.\n"
        f"Rendered SQL: {rendered}"
    )


def assert_sql_equals(composed_obj: Composed, expected_sql: str, normalize: bool = True):
    """
    Assert that rendered SQL equals expected SQL string.

    Args:
        composed_obj: psycopg3 Composed object
        expected_sql: Expected SQL string
        normalize: If True, normalize whitespace before comparison
    """
    rendered = render_sql_for_testing(composed_obj)

    if normalize:
        # Normalize whitespace for comparison
        rendered_normalized = ' '.join(rendered.split())
        expected_normalized = ' '.join(expected_sql.split())
        assert rendered_normalized == expected_normalized, (
            f"SQL mismatch.\n"
            f"Expected: {expected_normalized}\n"
            f"Got:      {rendered_normalized}"
        )
    else:
        assert rendered == expected_sql, (
            f"SQL mismatch.\n"
            f"Expected: {expected_sql}\n"
            f"Got:      {rendered}"
        )
```

#### 2.2 Update All SQL Validation Tests

**Strategy**: Replace all instances of:
```python
sql_str = str(composed_object)  # ❌ WRONG - returns repr()
assert "::inet" in sql_str
```

With:
```python
from tests.helpers.sql_rendering import render_sql_for_testing, assert_sql_contains

# Option 1: Use utility function
sql_str = render_sql_for_testing(composed_object)
assert "::inet" in sql_str

# Option 2: Use assertion helper (cleaner)
assert_sql_contains(composed_object, "::inet", "Network type requires inet casting")
```

**Files to Update** (~150 tests):
- `tests/regression/where_clause/test_complete_sql_validation.py`
- `tests/regression/where_clause/test_industrial_where_clause_generation.py`
- `tests/regression/where_clause/test_numeric_consistency_validation.py`
- `tests/regression/where_clause/test_precise_sql_validation.py`
- `tests/regression/where_clause/test_sql_structure_validation.py`
- `tests/core/test_special_types_tier1_core.py`
- `tests/core/test_jsonb_network_casting_fix.py`
- `tests/core/test_production_fix_validation.py`
- `tests/integration/repository/test_*.py`

**Automation Strategy**:

Use local AI model (Ministral-3-8B-Instruct) for bulk migration:

```python
# Migration script (run via local model or manually)
import re
import ast
from pathlib import Path

def migrate_sql_assertions(file_path: Path) -> str:
    """Migrate SQL assertion patterns to use render_sql_for_testing."""

    content = file_path.read_text()

    # Pattern 1: str(composed) usage
    content = re.sub(
        r'sql_str = str\((\w+)\)',
        r'sql_str = render_sql_for_testing(\1)',
        content
    )

    # Pattern 2: Add import if not present
    if 'render_sql_for_testing' in content and 'from tests.helpers.sql_rendering import' not in content:
        # Find first import line
        lines = content.split('\n')
        import_idx = next((i for i, line in enumerate(lines) if line.startswith('import ') or line.startswith('from ')), 0)
        lines.insert(import_idx, 'from tests.helpers.sql_rendering import render_sql_for_testing, assert_sql_contains\n')
        content = '\n'.join(lines)

    return content

# Apply to all affected files
test_files = [
    'tests/regression/where_clause/test_complete_sql_validation.py',
    'tests/regression/where_clause/test_industrial_where_clause_generation.py',
    # ... etc
]

for file_path in test_files:
    migrated = migrate_sql_assertions(Path(file_path))
    Path(file_path).write_text(migrated)
```

**Verification**:
```bash
# Run all SQL validation tests
uv run pytest tests/regression/where_clause/ -v
uv run pytest tests/core/test_special_types_tier1_core.py -v
uv run pytest tests/core/test_jsonb_network_casting_fix.py -v
```

**Acceptance Criteria**:
- [ ] `tests/helpers/sql_rendering.py` created with utilities
- [ ] All ~150 SQL validation tests updated
- [ ] Tests assert against rendered SQL, not repr()
- [ ] All SQL validation tests pass

---

### Phase 3: SQL Generation Bug Fixes (Week 3)

**Objective**: Fix any remaining SQL generation bugs revealed by Phase 2

**Note**: Phase 2 may reveal that tests are now passing once we properly render SQL. If bugs remain, they will be genuine SQL generation issues.

**Potential Issues** (will be revealed after Phase 2):

1. **Network Type Strategy Selection**
   - File: `fraiseql/operators/strategies/network.py`
   - Issue: `inRange` operator not selecting `NetworkOperatorStrategy`
   - Fix: Update strategy registry or operator mapping

2. **Type Casting for Special Types**
   - Files: `fraiseql/types/scalars/*.py`
   - Issue: Missing or incorrect PostgreSQL type casting
   - Fix: Ensure proper `::inet`, `::daterange`, `::macaddr` casting

3. **Boolean Handling**
   - File: `fraiseql/operators/strategies/base.py`
   - Issue: Boolean subclass of int causing numeric casting
   - Fix: Special-case boolean type detection

**Approach**:
1. Run Phase 2 verification
2. Identify remaining failures (if any)
3. Debug each failure category
4. Fix root causes in operator strategies or type definitions
5. Re-run tests

**Estimated Effort**: 10-20 hours (depends on Phase 2 results)

**Acceptance Criteria**:
- [ ] All SQL generation tests pass
- [ ] Network types use correct strategies
- [ ] Special types (daterange, macaddr) cast correctly
- [ ] Boolean types don't get numeric casting

---

### Phase 4: Test Configuration & Cleanup (Week 4)

**Objective**: Handle skipped tests, warnings, and errors

#### 4.1 Skipped Tests (92 total)

**Strategy**: Configure separate test suites

##### ShellCheck Dependency (1 skip)
```bash
# Option 1: Install shellcheck
sudo pacman -S shellcheck  # or apt-get install shellcheck

# Option 2: Make test optional
# In tests/grafana/test_import_script.py:
@pytest.mark.skipif(not shutil.which("shellcheck"), reason="shellcheck not installed")
def test_script_passes_shellcheck():
    ...
```

##### Performance Tests (90+ skips)
```bash
# Create separate pytest marker
# In pyproject.toml:
[tool.pytest.ini_options]
markers = [
    "performance: marks tests as performance tests (deselect with '-m \"not performance\"')",
    "integration: marks tests requiring full database setup"
]

# Mark performance tests:
@pytest.mark.performance
def test_rustresponsebytes_performance():
    ...

# Run tests without performance:
uv run pytest -m "not performance"

# Run ONLY performance tests:
uv run pytest -m "performance"
```

##### Partition Tests (1 skip)
```bash
# Keep skipped - requires specific PostgreSQL configuration
# Document in README that full integration requires partitioned DB
```

**Acceptance Criteria**:
- [ ] Shellcheck either installed or test marked optional
- [ ] Performance tests have `@pytest.mark.performance` marker
- [ ] Can run core tests without performance: `pytest -m "not performance"`
- [ ] Partition test documented as requiring special setup

#### 4.2 Warnings (10 total)

**Strategy**: Update deprecated API usage

```bash
# Run tests with warnings enabled
uv run pytest -W default 2>&1 | grep -A 3 "DeprecationWarning"

# Common fixes:
# 1. Update import paths (e.g., collections.abc instead of collections)
# 2. Update library API calls to new versions
# 3. Update type annotations to modern style (list[str] vs List[str])
```

**Estimated Effort**: 2-4 hours

**Acceptance Criteria**:
- [ ] No deprecation warnings in test output
- [ ] Dependencies updated to compatible versions
- [ ] Modern Python 3.10+ type annotations used

#### 4.3 Errors (2 total)

**File**: `tests/performance/test_rustresponsebytes_performance.py`

**Issue**: Performance test fixture setup failures

**Strategy**: Fix or mark as conditional

```python
# Option 1: Fix the fixture
@pytest.fixture
def performance_data():
    # Ensure proper setup
    try:
        # setup code
        yield data
    except Exception as e:
        pytest.skip(f"Performance test setup failed: {e}")

# Option 2: Mark as performance test (handled in 4.1)
@pytest.mark.performance
@pytest.mark.skipif(not has_rust_extension(), reason="Rust extension required")
def test_isinstance_check_overhead_rust_bytes():
    ...
```

**Acceptance Criteria**:
- [ ] Performance test fixtures work or are properly skipped
- [ ] No ERROR status in test runs (only PASSED/FAILED/SKIPPED)

---

## Implementation Timeline

### Week 1: Quick Wins (Phase 1)
- **Days 1-2**: Update v1.8.1 test expectations (16 tests)
- **Day 3**: Verification and commit
- **Outcome**: 16 failures → 0 failures

### Week 2: SQL Infrastructure (Phase 2)
- **Day 1**: Create `tests/helpers/sql_rendering.py`
- **Days 2-3**: Migrate ~150 SQL validation tests (use local AI model)
- **Day 4**: Verification - identify remaining bugs (if any)
- **Day 5**: Document SQL rendering patterns
- **Outcome**: ~150 failures → 0-20 failures (remaining bugs)

### Week 3: Bug Fixes (Phase 3)
- **Days 1-2**: Fix network type strategy issues
- **Days 3-4**: Fix special type casting issues
- **Day 5**: Fix boolean handling issues
- **Outcome**: 0-20 failures → 0 failures

### Week 4: Cleanup (Phase 4)
- **Day 1**: Configure performance test markers
- **Day 2**: Fix deprecation warnings
- **Day 3**: Fix performance test errors
- **Days 4-5**: Full test suite verification and documentation
- **Outcome**: 100% clean test suite

---

## Task Delegation Strategy

### Use Claude (You) For:
- ✅ Phase 1: Test expectation updates (requires understanding v1.8.1 semantics)
- ✅ Phase 2.1: Creating `sql_rendering.py` utility (architecture decision)
- ✅ Phase 3: Debugging SQL generation bugs (complex reasoning)
- ✅ Phase 4: Test configuration and strategy decisions

### Use Local AI Model For:
- ✅ Phase 2.2: Bulk migration of SQL assertion patterns (150 tests)
  - Pattern: `str(obj)` → `render_sql_for_testing(obj)`
  - Very repetitive, well-defined transformation
  - Perfect for Ministral-3-8B-Instruct with clear prompts

**Example Prompt for Local Model**:
```
Replace all instances of:
  sql_str = str(composed_object)
With:
  sql_str = render_sql_for_testing(composed_object)

And add this import at the top if not present:
  from tests.helpers.sql_rendering import render_sql_for_testing

File to process: [paste file content]

Show complete updated file.
```

---

## Success Metrics

### Primary Metrics
- **Test Pass Rate**: 96.9% → 100%
- **Failed Tests**: 214 → 0
- **Errors**: 2 → 0
- **Warnings**: 10 → 0

### Quality Metrics
- **Test Execution Time**: < 30 seconds (unit tests)
- **Coverage**: Maintain > 90%
- **Flakiness**: 0 flaky tests
- **Documentation**: All test patterns documented

### Completion Checklist
- [ ] All 16 v1.8.1 semantic tests pass
- [ ] All 150 SQL validation tests pass
- [ ] `tests/helpers/sql_rendering.py` created
- [ ] Performance tests configured with markers
- [ ] No deprecation warnings
- [ ] No test errors
- [ ] 100% test pass rate (5,315/5,315)
- [ ] CI/CD configured for separate test suites
- [ ] Documentation updated

---

## Risk Assessment

### High Risk (Mitigated)
**Phase 3: SQL Generation Bugs**
- **Risk**: Uncovering deep architectural issues in SQL generation
- **Mitigation**: Phase 2 infrastructure allows rapid testing and iteration
- **Fallback**: Can merge Phases 1-2 even if Phase 3 takes longer

### Medium Risk (Acceptable)
**Phase 2: Bulk Test Migration**
- **Risk**: Automated migration introduces errors
- **Mitigation**: Use local AI model for initial pass, then Claude review
- **Verification**: Run test suite after each batch of 20-30 files

### Low Risk (Negligible)
**Phase 1: Test Expectation Updates**
- **Risk**: Very low - pure test updates matching documented v1.8.1 behavior
- **Verification**: Tests themselves will validate correctness

**Phase 4: Cleanup**
- **Risk**: Very low - configuration and optional test organization
- **Verification**: Existing passing tests remain passing

---

## Dependencies & Prerequisites

### Required
- [x] FraiseQL v1.8.1 (commit `06939d09` or later)
- [x] psycopg 3.x (already installed)
- [x] pytest with markers support (already configured)
- [ ] `tests/helpers/sql_rendering.py` (Phase 2.1)

### Optional
- [ ] shellcheck (for Grafana script test)
- [ ] Local AI model access (for bulk migration in Phase 2.2)
- [ ] Performance test markers configured

---

## Next Steps

### Immediate Action (Today)
1. **Review this plan** - Ensure alignment with project goals
2. **Approve Phase 1** - Quick wins to reduce failure count
3. **Start Phase 1 execution** - Update 16 tests for v1.8.1 semantics

### This Week
1. Complete Phase 1 (2-4 hours)
2. Begin Phase 2.1 - Create SQL rendering utility (4 hours)
3. Start Phase 2.2 - Begin migrating SQL tests (8-12 hours)

### Next Week
1. Complete Phase 2.2 - Finish SQL test migration
2. Verify Phase 2 - Run full SQL validation suite
3. Begin Phase 3 - Fix any remaining SQL bugs

### Week 3-4
1. Complete Phase 3 - SQL bug fixes
2. Execute Phase 4 - Cleanup and configuration
3. Final verification and documentation

---

## Conclusion

This plan provides a **systematic, phased approach** to achieving 100% test pass rate:

1. **Phase 1** (Quick Wins): 16 failures fixed in 2-4 hours
2. **Phase 2** (Infrastructure): ~150 failures fixed with SQL rendering utility
3. **Phase 3** (Bug Fixes): Remaining bugs fixed with proper debugging
4. **Phase 4** (Cleanup): Professional test suite configuration

**Total Effort**: 20-30 hours over 4 weeks
**Confidence**: HIGH (phases are independent and low-risk)
**Outcome**: Professional, maintainable, 100% passing test suite

**Ready to begin Phase 1 immediately.**
